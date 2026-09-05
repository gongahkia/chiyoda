//! Chiyoda's reference language, executable semantics, and deterministic runtime.
//!
//! The runtime is a research reference implementation. It deliberately exposes
//! every parameter and records every state transition; it is not certified for
//! regulatory, operational, or life-safety decisions.

mod avoidance;
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
    AgentState, InformationDeliveryMetrics, InformationInterventionKind, MovementMetrics,
    OnSurfaceClearanceMetrics, QueueMetrics, QueueResourceBreakdown, QueueResourceMetrics,
    RunBundle, SweptOnSurfaceClearanceMetrics, bundle_hash,
};
pub use calibration::{
    CalibrationError, PlatformCalibrationReport, calibrate_eindhoven_platform, verify_catalog_files,
};
pub use evidence::{EvidenceCatalog, EvidencePurpose, EvidenceValidationError, validate_catalog};
pub use experiment::{
    ExperimentAssumption, ExperimentAssumptionTarget, ExperimentManifest,
    ExperimentSensitivityStudy, ExperimentSourceAttestation, ExperimentValidationError,
    validate_experiment_manifest,
};
pub use formatter::format_scenario;
pub use layout::{
    GeographicPoint, OpenStreetMapLayoutReport, OpenStreetMapLocalProjectionReport,
    OsmInspectionLimits, OsmLayoutError, OsmScenarioAnchorManifest, OsmScenarioAnchorReport,
    OsmScenarioAnchorSource, OsmScenarioAnchorTarget, OsmScenarioAnchorValidationError,
    OsmScenarioCoordinateAnchor, ResolvedOsmScenarioCoordinateAnchor, anchor_osm_scenario,
    inspect_openstreetmap_layout, project_openstreetmap_layout_report,
    validate_osm_scenario_anchor_manifest, verify_openstreetmap_layout_catalog_contract,
    verify_openstreetmap_layout_report, verify_openstreetmap_local_projection_report,
    verify_osm_scenario_anchor_report,
};
pub use model::{CanonicalScenario, Scenario};
pub use parser::{ParseError, parse};
pub use reference::{
    CrowdQueueReferenceReport, ReferenceDataError, VruReferenceReport,
    summarize_crowd_queue_reference, summarize_vru_trajectory_reference,
};
pub use runtime::{BundleVerification, RunOptions, integration_step_count, run, verify_run_bundle};
pub use sensitivity::{
    AssumptionBasis, SensitivityCondition, SensitivityDerivedReport, SensitivityDesign,
    SensitivityError, SensitivityFactor, SensitivityManifest, SensitivityReference,
    SensitivityStudy, SensitivityTarget, plan_sensitivity, resolve_sensitivity_target_value,
};
pub use validate::{ValidationError, validate};

/// Increment this when the canonical IR or runtime trace contract changes.
pub const LANGUAGE_VERSION: &str = "0.28";
pub const RUNTIME_VERSION: &str = env!("CARGO_PKG_VERSION");
