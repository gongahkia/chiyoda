use crate::{CanonicalScenario, RUNTIME_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write};

pub const BUNDLE_VERSION: &str = "0.24";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub id: String,
    pub group: String,
    pub surface: String,
    pub x_m: f64,
    pub y_m: f64,
    pub z_m: f64,
    pub state: AgentState,
    pub beliefs: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Moving,
    WaitingToDepart,
    WaitingAtWaypoint,
    WaitingForRoute,
    WaitingForLift,
    WaitingForConnector,
    WaitingForGate,
    WaitingForExit,
    InTransit,
    Evacuated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceFrame {
    pub step: u64,
    pub time_s: f64,
    pub agents: Vec<AgentSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunEvent {
    pub time_s: f64,
    pub kind: String,
    pub subject: String,
    pub detail: String,
}

/// Aggregate delivery and acceptance counts for one authored information
/// intervention. Counts are deterministic runtime observations, not survey or
/// behavioral measurements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InformationInterventionKind {
    Message,
    Countermeasure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InformationDeliveryMetrics {
    pub kind: InformationInterventionKind,
    pub received_agents: u32,
    pub accepted_agents: u32,
}

/// Discrete telemetry for one modeled capacity queue. These values describe
/// only the reference runtime's timestep states; they are not measurements of
/// a physical queue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueResourceMetrics {
    /// Agents that entered this modeled queue at least once during the run.
    pub ever_queued_agents: u32,
    /// Sum of authored integration-step time spent in this modeled queue.
    pub cumulative_wait_agent_seconds: f64,
    /// Largest number of agents observed waiting in this modeled queue at one
    /// reference-runtime step boundary.
    pub peak_waiting_agents: u32,
}

/// Queue telemetry by capacity mechanism. Resources with no authored capacity
/// remain present with zero values so a current run is unambiguous.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueMetrics {
    pub lift: QueueResourceMetrics,
    pub connector: QueueResourceMetrics,
    pub gate: QueueResourceMetrics,
    pub exit: QueueResourceMetrics,
    /// Per-authored-resource discrete telemetry for current bundles. The
    /// aggregate fields remain unique-agent counts across a mechanism; an
    /// agent may therefore appear in more than one entry in one map.
    /// Omission preserves inspection of 0.22 telemetry bundles, which did not
    /// expose the resource attribution needed to reconstruct this breakdown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_resource: Option<QueueResourceBreakdown>,
}

/// Per-resource queue telemetry. Each map has every authored resource whose
/// declared runtime semantics can queue: lifts, capacity-limited non-lift
/// connectors, gates, and capacity-limited exits. Entries remain present at
/// zero when a resource was not reached.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueResourceBreakdown {
    pub lifts: BTreeMap<String, QueueResourceMetrics>,
    pub connectors: BTreeMap<String, QueueResourceMetrics>,
    pub gates: BTreeMap<String, QueueResourceMetrics>,
    pub exits: BTreeMap<String, QueueResourceMetrics>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunMetrics {
    pub total_agents: u32,
    pub evacuated_agents: u32,
    /// Counts completed evacuations by the final exit selected at runtime.
    ///
    /// The omission rule keeps bundles made before exit attribution
    /// hash-verifiable after deserialization.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub evacuated_by_exit: BTreeMap<String, u32>,
    /// Final non-evacuated agent states at the end of the configured duration.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub remaining_by_state: BTreeMap<String, u32>,
    /// Counts agents reached by and accepting each authored message or
    /// countermeasure. Omission preserves compatibility with older bundles.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub information_delivery: BTreeMap<String, InformationDeliveryMetrics>,
    /// Time at which the final agent evacuated, present only when every agent
    /// completed evacuation during the configured duration.
    pub clearance_time_s: Option<f64>,
    /// Time at which the last observed evacuation occurred, whether or not
    /// agents remained when the configured duration ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_exit_time_s: Option<f64>,
    pub mean_exit_time_s: Option<f64>,
    pub queued_for_lift_agents: u32,
    pub queued_for_connector_agents: u32,
    pub queued_for_gate_agents: u32,
    pub queued_for_exit_agents: u32,
    /// Detailed, discrete queue telemetry. The four legacy exposure fields
    /// above mirror each resource's `ever_queued_agents` value for continuity.
    /// Omission preserves inspection of bundles emitted before version 0.22.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_metrics: Option<QueueMetrics>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunBundle {
    pub bundle_version: String,
    pub runtime_version: String,
    pub scenario_hash: String,
    pub scenario: CanonicalScenario,
    pub options: BTreeMap<String, String>,
    pub trace: Vec<TraceFrame>,
    pub events: Vec<RunEvent>,
    pub metrics: RunMetrics,
    pub bundle_hash: String,
}

impl RunBundle {
    #[must_use]
    pub fn new(
        scenario: CanonicalScenario,
        options: BTreeMap<String, String>,
        mut trace: Vec<TraceFrame>,
        mut events: Vec<RunEvent>,
        mut metrics: RunMetrics,
    ) -> Self {
        normalize_trace(&mut trace);
        for event in &mut events {
            event.time_s = canonical_number(event.time_s);
        }
        metrics.clearance_time_s = metrics.clearance_time_s.map(canonical_number);
        metrics.last_exit_time_s = metrics.last_exit_time_s.map(canonical_number);
        metrics.mean_exit_time_s = metrics.mean_exit_time_s.map(canonical_number);
        if let Some(queue_metrics) = &mut metrics.queue_metrics {
            queue_metrics.lift.cumulative_wait_agent_seconds =
                canonical_number(queue_metrics.lift.cumulative_wait_agent_seconds);
            queue_metrics.connector.cumulative_wait_agent_seconds =
                canonical_number(queue_metrics.connector.cumulative_wait_agent_seconds);
            queue_metrics.gate.cumulative_wait_agent_seconds =
                canonical_number(queue_metrics.gate.cumulative_wait_agent_seconds);
            queue_metrics.exit.cumulative_wait_agent_seconds =
                canonical_number(queue_metrics.exit.cumulative_wait_agent_seconds);
            if let Some(by_resource) = &mut queue_metrics.by_resource {
                for resource in by_resource
                    .lifts
                    .values_mut()
                    .chain(by_resource.connectors.values_mut())
                    .chain(by_resource.gates.values_mut())
                    .chain(by_resource.exits.values_mut())
                {
                    resource.cumulative_wait_agent_seconds =
                        canonical_number(resource.cumulative_wait_agent_seconds);
                }
            }
        }
        let scenario_hash = canonical_hash(&scenario);
        let mut bundle = Self {
            bundle_version: BUNDLE_VERSION.to_owned(),
            runtime_version: RUNTIME_VERSION.to_owned(),
            scenario_hash,
            scenario,
            options,
            trace,
            events,
            metrics,
            bundle_hash: String::new(),
        };
        bundle.bundle_hash = bundle_hash(&bundle);
        bundle
    }

    #[must_use]
    pub fn verifies_hash(&self) -> bool {
        self.bundle_hash == bundle_hash(self)
    }
}

fn normalize_trace(trace: &mut [TraceFrame]) {
    for frame in trace {
        frame.time_s = canonical_number(frame.time_s);
        for agent in &mut frame.agents {
            agent.x_m = canonical_number(agent.x_m);
            agent.y_m = canonical_number(agent.y_m);
            agent.z_m = canonical_number(agent.z_m);
            for belief in agent.beliefs.values_mut() {
                *belief = canonical_number(*belief);
            }
        }
    }
}

/// Decimal quantization makes a JSON run bundle hash stable across its own
/// parse/serialize round trip. Internal integration remains `f64`; only the
/// persisted research artifact is normalized to nanometre/nanosecond scale.
fn canonical_number(value: f64) -> f64 {
    (value * 1_000_000_000.0).round() / 1_000_000_000.0
}

#[must_use]
pub fn canonical_hash<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("canonical Chiyoda values are serializable");
    hex_digest(&bytes)
}

#[must_use]
pub fn bundle_hash(bundle: &RunBundle) -> String {
    let mut unsigned = bundle.clone();
    unsigned.bundle_hash.clear();
    canonical_hash(&unsigned)
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut text, "{byte:02x}").expect("writing to a string cannot fail");
    }
    text
}
