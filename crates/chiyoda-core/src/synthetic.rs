//! Deterministic synthetic exercises for the local avoidance kernel.
//!
//! These cases are regression and inspectability fixtures. They deliberately do
//! not use observed trajectories, fit runtime parameters, or claim pedestrian
//! validity.

use crate::avoidance::{AvoidanceAgent, Vec2, choose_velocity};
use serde::{Deserialize, Serialize};

const TIME_HORIZONS_S: &[f64] = &[0.5, 1.0, 2.5, 5.0];
const TIMESTEP_S: f64 = 0.1;

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
