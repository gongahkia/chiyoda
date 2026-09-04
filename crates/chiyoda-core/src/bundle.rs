use crate::{CanonicalScenario, RUNTIME_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunMetrics {
    pub total_agents: u32,
    pub evacuated_agents: u32,
    pub clearance_time_s: Option<f64>,
    pub mean_exit_time_s: Option<f64>,
    pub queued_for_lift_agents: u32,
    pub queued_for_connector_agents: u32,
    pub queued_for_gate_agents: u32,
    pub queued_for_exit_agents: u32,
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
        metrics.mean_exit_time_s = metrics.mean_exit_time_s.map(canonical_number);
        let scenario_hash = canonical_hash(&scenario);
        let mut bundle = Self {
            bundle_version: "0.14".to_owned(),
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
