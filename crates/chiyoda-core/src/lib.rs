//! Chiyoda's deterministic scenario language and reference runtime.
//!
//! The runtime records its authored inputs and every simulated state transition
//! so a compatible installation can reproduce a run bundle.

mod avoidance;
pub mod bundle;
pub mod formatter;
pub mod generator;
pub mod model;
pub mod parser;
pub mod runtime;
pub mod validate;

pub use bundle::{
    AgentState, InformationDeliveryMetrics, InformationInterventionKind, MovementMetrics,
    OnSurfaceClearanceMetrics, QueueMetrics, QueueResourceBreakdown, QueueResourceMetrics,
    RunBundle, SweptOnSurfaceClearanceMetrics, bundle_hash,
};
pub use formatter::format_scenario;
pub use model::{CanonicalScenario, Scenario};
pub use parser::{ParseError, parse};
pub use runtime::{BundleVerification, RunOptions, integration_step_count, run, verify_run_bundle};
pub use validate::{ValidationError, validate};

/// Increment this when the canonical IR or runtime trace contract changes.
pub const LANGUAGE_VERSION: &str = "0.29";
pub const RUNTIME_VERSION: &str = env!("CARGO_PKG_VERSION");
