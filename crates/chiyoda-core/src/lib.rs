//! Chiyoda's reference language, executable semantics, and deterministic runtime.
//!
//! The runtime is a research reference implementation. It deliberately exposes
//! every parameter and records every state transition; it is not certified for
//! regulatory, operational, or life-safety decisions.

pub mod benchmark;
pub mod bundle;
pub mod calibration;
pub mod evidence;
pub mod formatter;
pub mod generator;
pub mod model;
pub mod parser;
pub mod runtime;
pub mod validate;

pub use benchmark::{BenchmarkManifest, BenchmarkValidationError, validate_manifest};
pub use bundle::{AgentState, RunBundle, bundle_hash};
pub use calibration::{
    CalibrationError, PlatformCalibrationReport, calibrate_eindhoven_platform, verify_catalog_files,
};
pub use evidence::{EvidenceCatalog, EvidenceValidationError, validate_catalog};
pub use formatter::format_scenario;
pub use model::{CanonicalScenario, Scenario};
pub use parser::{ParseError, parse};
pub use runtime::{RunOptions, run};
pub use validate::{ValidationError, validate};

/// Increment this when the canonical IR or runtime trace contract changes.
pub const LANGUAGE_VERSION: &str = "0.2";
pub const RUNTIME_VERSION: &str = env!("CARGO_PKG_VERSION");
