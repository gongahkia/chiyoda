//! Chiyoda's reference language, executable semantics, and deterministic runtime.
//!
//! The runtime is a research reference implementation. It deliberately exposes
//! every parameter and records every state transition; it is not certified for
//! regulatory, operational, or life-safety decisions.

pub mod benchmark;
pub mod bundle;
pub mod calibration;
pub mod evidence;
pub mod experiment;
pub mod formatter;
pub mod generator;
pub mod layout;
pub mod model;
pub mod parser;
pub mod reference;
pub mod runtime;
pub mod sensitivity;
pub mod validate;

pub use benchmark::{BenchmarkManifest, BenchmarkValidationError, validate_manifest};
pub use bundle::{
    AgentState, InformationDeliveryMetrics, InformationInterventionKind, RunBundle, bundle_hash,
};
pub use calibration::{
    CalibrationError, PlatformCalibrationReport, calibrate_eindhoven_platform, verify_catalog_files,
};
pub use evidence::{EvidenceCatalog, EvidencePurpose, EvidenceValidationError, validate_catalog};
pub use experiment::{
    ExperimentAssumption, ExperimentManifest, ExperimentValidationError,
    validate_experiment_manifest,
};
pub use formatter::format_scenario;
pub use layout::{
    OpenStreetMapLayoutReport, OsmInspectionLimits, OsmLayoutError, inspect_openstreetmap_layout,
    verify_openstreetmap_layout_report,
};
pub use model::{CanonicalScenario, Scenario};
pub use parser::{ParseError, parse};
pub use reference::{
    CrowdQueueReferenceReport, ReferenceDataError, VruReferenceReport,
    summarize_crowd_queue_reference, summarize_vru_trajectory_reference,
};
pub use runtime::{RunOptions, run};
pub use sensitivity::{
    AssumptionBasis, SensitivityCondition, SensitivityDerivedReport, SensitivityDesign,
    SensitivityError, SensitivityFactor, SensitivityManifest, SensitivityReference,
    SensitivityStudy, SensitivityTarget, plan_sensitivity,
};
pub use validate::{ValidationError, validate};

/// Increment this when the canonical IR or runtime trace contract changes.
pub const LANGUAGE_VERSION: &str = "0.19";
pub const RUNTIME_VERSION: &str = env!("CARGO_PKG_VERSION");
