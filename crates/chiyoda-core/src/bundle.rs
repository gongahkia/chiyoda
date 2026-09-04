use crate::{CanonicalScenario, RUNTIME_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

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
    WaitingForLift,
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
        trace: Vec<TraceFrame>,
        events: Vec<RunEvent>,
        metrics: RunMetrics,
    ) -> Self {
        let scenario_hash = canonical_hash(&scenario);
        let mut bundle = Self {
            bundle_version: "0.1".to_owned(),
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
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
