//! Deterministic synthetic exercises for reference-runtime semantics.
//!
//! These cases are regression and inspectability fixtures. They deliberately do
//! not use observed trajectories, fit runtime parameters, or claim pedestrian
//! validity.

use crate::{
    BundleVerification, ParseError, RunError, RunOptions,
    avoidance::{AvoidanceAgent, Vec2, choose_velocity},
    bundle::{RunBundle, RunMetrics},
    parse, run, verify_run_bundle,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const TIME_HORIZONS_S: &[f64] = &[0.5, 1.0, 2.5, 5.0];
const TIMESTEP_S: f64 = 0.1;
const SYSTEM_SOURCE: &str = include_str!("../../../examples/synthetic/system-contract.chy");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyntheticAvoidanceAgent {
    pub id: String,
    pub position_m: [f64; 2],
    pub velocity_mps: [f64; 2],
    pub preferred_velocity_mps: [f64; 2],
    pub radius_m: f64,
    pub max_speed_mps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyntheticAvoidanceDecision {
    pub id: String,
    pub selected_velocity_mps: [f64; 2],
    pub constraint_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyntheticAvoidanceCase {
    pub id: String,
    pub description: String,
    pub timestep_s: f64,
    pub time_horizon_s: f64,
    pub agents: Vec<SyntheticAvoidanceAgent>,
    pub decisions: Vec<SyntheticAvoidanceDecision>,
    /// Center-to-center separation after exactly one synthetic local step.
    /// This is an implementation trace, not observed clearance or safety data.
    pub minimum_next_center_distance_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyntheticAvoidanceReport {
    pub schema_version: String,
    pub runtime_version: String,
    pub cases: Vec<SyntheticAvoidanceCase>,
    pub status: String,
    pub claim_boundary: String,
}

/// One observed queue mechanism in the fixed synthetic integration fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyntheticQueueObservation {
    /// The authored resource's runtime mechanism, such as `connector` or
    /// `exit`; this is not a classification of a physical queue.
    pub mechanism: String,
    pub resource: String,
    pub ever_queued_agents: u32,
    pub cumulative_wait_agent_seconds: f64,
    pub peak_waiting_agents: u32,
}

/// Reproducibility facts about the synthetic integration execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntheticSelfReplay {
    /// `reconstructed` means the persisted bundle hash and a fresh execution
    /// of its embedded scenario agreed under the installed runtime contract.
    pub bundle_verification: String,
    /// This means an immediate second execution produced the same reference
    /// artifact. It is model self-consistency, not a future forecast.
    pub exact_self_replay: bool,
    pub trace_every_steps: u32,
}

/// A fixed, end-to-end structural exercise of existing reference semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyntheticSystemReport {
    pub schema_version: String,
    pub runtime_version: String,
    /// The complete fixture source is embedded to make this report inspectable
    /// without relying on an ambient checkout.
    pub source: String,
    pub source_sha256: String,
    pub scenario_hash: String,
    pub bundle_hash: String,
    pub replay_bundle_hash: String,
    pub trace_frame_count: u64,
    /// Names of the authored surfaces reached by at least one traced agent.
    pub observed_surfaces: Vec<String>,
    /// Exact reference-runtime event counts, grouped by event kind.
    pub event_counts: BTreeMap<String, u64>,
    /// Per-resource, timestep-level queue telemetry from the fixture. It is
    /// not an observed queue measurement.
    pub queue_resources: BTreeMap<String, SyntheticQueueObservation>,
    pub metrics: RunMetrics,
    pub self_replay: SyntheticSelfReplay,
    pub status: String,
    pub claim_boundary: String,
}

/// Failure to execute the embedded synthetic integration fixture.
#[derive(Debug, thiserror::Error)]
pub enum SyntheticSystemError {
    #[error("synthetic system fixture did not parse: {0}")]
    Parse(#[from] ParseError),
    #[error("synthetic system fixture did not execute: {0}")]
    Run(#[from] RunError),
    #[error("synthetic system fixture changed across identical executions")]
    NonDeterministicSelfReplay,
    #[error("synthetic system fixture is not reconstructable under the installed runtime")]
    LegacyVerification,
}

/// Exercise symmetric head-on, crossing, and deterministic co-location paths
/// at a small, fixed set of horizons. The report is reproducible from the
/// installed local-motion implementation and does not modify it.
#[must_use]
pub fn synthetic_avoidance_report() -> SyntheticAvoidanceReport {
    let templates = [
        (
            "head-on",
            "Two equal agents approach each other on one horizontal line.",
            vec![
                fixture_agent("left", [-1.0, 0.0], [1.0, 0.0], [1.0, 0.0]),
                fixture_agent("right", [1.0, 0.0], [-1.0, 0.0], [-1.0, 0.0]),
            ],
        ),
        (
            "crossing",
            "Two equal agents approach the same point from perpendicular directions.",
            vec![
                fixture_agent("west", [-1.0, 0.0], [1.0, 0.0], [1.0, 0.0]),
                fixture_agent("south", [0.0, -1.0], [0.0, 1.0], [0.0, 1.0]),
            ],
        ),
        (
            "co-located-tie",
            "Two agents begin at the same point to exercise the stable identifier tie rule.",
            vec![
                fixture_agent("alpha", [0.0, 0.0], [0.0, 0.0], [1.0, 0.0]),
                fixture_agent("beta", [0.0, 0.0], [0.0, 0.0], [-1.0, 0.0]),
            ],
        ),
    ];
    let cases = templates
        .into_iter()
        .flat_map(|(id, description, agents)| {
            TIME_HORIZONS_S
                .iter()
                .map(move |time_horizon_s| run_case(id, description, &agents, *time_horizon_s))
        })
        .collect();
    SyntheticAvoidanceReport {
        schema_version: "chiyoda.synthetic-avoidance.v1".to_owned(),
        runtime_version: crate::RUNTIME_VERSION.to_owned(),
        cases,
        status: "synthetic_conformance_only".to_owned(),
        claim_boundary: "These are fixed, deterministic exercises of the current two-dimensional ORCA kernel. They contain no observed pedestrians, facility geometry, routing, queues, stairs, population estimate, calibration objective, held-out score, prediction, operational result, or safety claim. They do not select or change the runtime time horizon.".to_owned(),
    }
}

/// Execute the fixed multi-surface interchange fixture and independently
/// reconstruct it. This covers the existing queue, staircase, rerouting, and
/// scheduled-state mechanisms in one artifact. It deliberately does not
/// establish behavioral, predictive, operational, or facility validity.
pub fn synthetic_system_report() -> Result<SyntheticSystemReport, SyntheticSystemError> {
    let scenario = parse(SYSTEM_SOURCE)?;
    let options = RunOptions {
        trace_every_steps: 1,
    };
    let bundle = run(&scenario, options)?;
    let replay = run(&scenario, options)?;
    if bundle != replay {
        return Err(SyntheticSystemError::NonDeterministicSelfReplay);
    }
    if !matches!(verify_run_bundle(&bundle)?, BundleVerification::Reconstructed) {
        return Err(SyntheticSystemError::LegacyVerification);
    }

    Ok(SyntheticSystemReport {
        schema_version: "chiyoda.synthetic-system.v1".to_owned(),
        runtime_version: crate::RUNTIME_VERSION.to_owned(),
        source: SYSTEM_SOURCE.to_owned(),
        source_sha256: sha256(SYSTEM_SOURCE.as_bytes()),
        scenario_hash: bundle.scenario_hash.clone(),
        bundle_hash: bundle.bundle_hash.clone(),
        replay_bundle_hash: replay.bundle_hash,
        trace_frame_count: u64::try_from(bundle.trace.len())
            .expect("synthetic trace frame count fits u64"),
        observed_surfaces: observed_surfaces(&bundle),
        event_counts: event_counts(&bundle),
        queue_resources: queue_resources(&bundle),
        metrics: bundle.metrics,
        self_replay: SyntheticSelfReplay {
            bundle_verification: "reconstructed".to_owned(),
            exact_self_replay: true,
            trace_every_steps: options.trace_every_steps,
        },
        status: "synthetic_integration_conformance_only".to_owned(),
        claim_boundary: "This one authored, structural fixture exercises the installed reference runtime's queue, routing, staircase, multi-surface, and scheduled operational-state semantics. Its generated agents, geometry, release cadence, service limits, closure, and capacity change are not observations or a station model. Exact self-replay establishes only deterministic model self-consistency; it is not a future prediction, calibration result, held-out evaluation, crowd-behavior result, operational recommendation, facility validation, or safety claim.".to_owned(),
    })
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn observed_surfaces(bundle: &RunBundle) -> Vec<String> {
    bundle
        .trace
        .iter()
        .flat_map(|frame| frame.agents.iter().map(|agent| agent.surface.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn event_counts(bundle: &RunBundle) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for event in &bundle.events {
        *counts.entry(event.kind.clone()).or_default() += 1;
    }
    counts
}

fn queue_resources(bundle: &RunBundle) -> BTreeMap<String, SyntheticQueueObservation> {
    let Some(queue_metrics) = &bundle.metrics.queue_metrics else {
        return BTreeMap::new();
    };
    let Some(by_resource) = &queue_metrics.by_resource else {
        return BTreeMap::new();
    };
    let mut resources = BTreeMap::new();
    for (mechanism, entries) in [
        ("lift", &by_resource.lifts),
        ("connector", &by_resource.connectors),
        ("gate", &by_resource.gates),
        ("exit", &by_resource.exits),
    ] {
        for (resource, metrics) in entries {
            resources.insert(
                format!("{mechanism}:{resource}"),
                SyntheticQueueObservation {
                    mechanism: mechanism.to_owned(),
                    resource: resource.clone(),
                    ever_queued_agents: metrics.ever_queued_agents,
                    cumulative_wait_agent_seconds: metrics.cumulative_wait_agent_seconds,
                    peak_waiting_agents: metrics.peak_waiting_agents,
                },
            );
        }
    }
    resources
}

fn fixture_agent(
    id: &str,
    position_m: [f64; 2],
    velocity_mps: [f64; 2],
    preferred_velocity_mps: [f64; 2],
) -> SyntheticAvoidanceAgent {
    SyntheticAvoidanceAgent {
        id: id.to_owned(),
        position_m,
        velocity_mps,
        preferred_velocity_mps,
        radius_m: 0.3,
        max_speed_mps: 1.0,
    }
}

fn run_case(
    id: &str,
    description: &str,
    agents: &[SyntheticAvoidanceAgent],
    time_horizon_s: f64,
) -> SyntheticAvoidanceCase {
    let snapshot = agents
        .iter()
        .map(|agent| AvoidanceAgent {
            id: agent.id.clone(),
            position: Vec2 {
                x: agent.position_m[0],
                y: agent.position_m[1],
            },
            velocity: Vec2 {
                x: agent.velocity_mps[0],
                y: agent.velocity_mps[1],
            },
            radius_m: agent.radius_m,
            max_speed_mps: agent.max_speed_mps,
            queue_priority: None,
        })
        .collect::<Vec<_>>();
    let decisions = agents
        .iter()
        .enumerate()
        .map(|(index, agent)| {
            let decision = choose_velocity(
                &snapshot,
                index,
                Vec2 {
                    x: agent.preferred_velocity_mps[0],
                    y: agent.preferred_velocity_mps[1],
                },
                agent.max_speed_mps,
                time_horizon_s,
                TIMESTEP_S,
            );
            SyntheticAvoidanceDecision {
                id: agent.id.clone(),
                selected_velocity_mps: [decision.velocity.x, decision.velocity.y],
                constraint_fallback: decision.constraint_fallback,
            }
        })
        .collect::<Vec<_>>();
    let next_positions = agents
        .iter()
        .zip(&decisions)
        .map(|(agent, decision)| {
            [
                agent.position_m[0] + decision.selected_velocity_mps[0] * TIMESTEP_S,
                agent.position_m[1] + decision.selected_velocity_mps[1] * TIMESTEP_S,
            ]
        })
        .collect::<Vec<_>>();
    SyntheticAvoidanceCase {
        id: format!("{id}-horizon-{time_horizon_s:.1}s"),
        description: description.to_owned(),
        timestep_s: TIMESTEP_S,
        time_horizon_s,
        agents: agents.to_vec(),
        decisions,
        minimum_next_center_distance_m: minimum_pair_distance(&next_positions),
    }
}

fn minimum_pair_distance(positions: &[[f64; 2]]) -> f64 {
    let mut minimum = f64::INFINITY;
    for (index, position) in positions.iter().enumerate() {
        for other in &positions[index + 1..] {
            minimum = minimum.min(
                (position[0] - other[0])
                    .mul_add(
                        position[0] - other[0],
                        (position[1] - other[1]) * (position[1] - other[1]),
                    )
                    .sqrt(),
            );
        }
    }
    minimum
}

#[cfg(test)]
mod tests {
    use super::synthetic_avoidance_report;

    #[test]
    fn synthetic_suite_is_deterministic_and_discloses_its_boundary() {
        let first = synthetic_avoidance_report();
        let repeated = synthetic_avoidance_report();

        assert_eq!(first, repeated);
        assert_eq!(first.cases.len(), 12);
        assert!(
            first
                .cases
                .iter()
                .all(|case| case.minimum_next_center_distance_m.is_finite())
        );
        assert_eq!(first.status, "synthetic_conformance_only");
        assert!(first.claim_boundary.contains("no observed pedestrians"));
    }
}
