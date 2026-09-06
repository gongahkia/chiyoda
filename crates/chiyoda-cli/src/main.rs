use anyhow::{Context, Result, bail};
use chiyoda_core::model::Point3;
use chiyoda_core::{
    BenchmarkManifest, BundleVerification, CanonicalScenario, CoordinationRoadmap, EvidenceCatalog,
    ExperimentAssumptionTarget, ExperimentManifest, ExperimentSensitivityStudy, GeographicPoint,
    InformationDeliveryMetrics, MovementMetrics, OnSurfaceClearanceMetrics,
    OpenStreetMapLayoutReport, OpenStreetMapLocalProjectionReport, OsmInspectionLimits,
    OsmScenarioAnchorManifest, OsmScenarioAnchorReport, QueueGridCoordinationPlan,
    QueueGridCoordinationRequest, QueueGridRollingCoordinationRequest, QueueGridRollingOutcome,
    QueueGridServiceAssumption, QueueGridTicketRequest, QueueGridUnresolvedReason, QueueMetrics,
    QueueResourceMetrics, RunBundle, RunOptions, SensitivityFactor, SensitivityManifest,
    SensitivityTarget, SweptOnSurfaceClearanceMetrics, TimedDiscSegment, TimedDiscTrajectory,
    anchor_osm_scenario, assess_queue_grid_rolling, calibrate_eindhoven_platform,
    estimate_queue_grid_departures, format_scenario, generator, inspect_openstreetmap_layout,
    parse, plan_sensitivity, project_openstreetmap_layout_report, reference_clearance_epsilon_m,
    resolve_sensitivity_target_value, run, summarize_crowd_queue_reference,
    summarize_vru_trajectory_reference, timed_disc_conflicts, validate, validate_catalog,
    validate_experiment_manifest, validate_manifest, validate_osm_scenario_anchor_manifest,
    verify_catalog_files, verify_openstreetmap_layout_catalog_contract,
    verify_openstreetmap_layout_report, verify_openstreetmap_local_projection_report,
    verify_osm_scenario_anchor_report, verify_run_bundle,
};
use chiyoda_core::{bundle::RunMetrics, experiment::ExperimentSourceAttestation};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Parser)]
#[command(
    name = "chiyoda",
    version,
    about = "Compile and run formal 3D pedestrian-flow experiments"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse and validate a Chiyoda source file without executing it.
    Check { source: PathBuf },
    /// Compile source to canonical, versioned JSON IR.
    Compile {
        source: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Render source in the canonical Chiyoda style.
    Format {
        source: PathBuf,
        /// Write formatted source to a distinct file instead of standard output.
        #[arg(short, long, conflicts_with = "check")]
        output: Option<PathBuf>,
        /// Exit non-zero when the source is not already canonical.
        #[arg(long)]
        check: bool,
    },
    /// Execute the deterministic reference runtime and create a run bundle.
    Run {
        source: PathBuf,
        #[arg(short, long, default_value = "out")]
        output: PathBuf,
        #[arg(long, default_value_t = 10)]
        trace_every: u32,
    },
    /// Produce a replayable, bounded coordination artifact for one authored queue grid.
    CoordinateQueueGrid {
        source: PathBuf,
        /// Authored agent-group identifier to coordinate.
        #[arg(long)]
        group: String,
        /// Authored queue-grid identifier to coordinate.
        #[arg(long)]
        queue_grid: String,
        /// Explicit uncalibrated time of the first service completion, in seconds.
        #[arg(long)]
        first_departure_at_s: f64,
        /// Explicit uncalibrated active-queue service headway, in seconds.
        #[arg(long)]
        headway_s: f64,
        /// Static lattice spacing for the planner, in metres.
        #[arg(long, default_value_t = 0.6)]
        roadmap_spacing_m: f64,
        /// Maximum statically clear roadmap nodes.
        #[arg(long, default_value_t = 3_000)]
        maximum_roadmap_nodes: usize,
        /// Time grid used only by the bounded planner, in seconds.
        #[arg(long, default_value_t = 0.5)]
        planning_timestep_s: f64,
        /// Maximum low-level search states per route stage.
        #[arg(long, default_value_t = 100_000)]
        maximum_low_level_expansions: u64,
        /// Maximum nodes in a per-cohort conflict tree.
        #[arg(long, default_value_t = 1_000)]
        maximum_conflict_tree_nodes: u64,
        /// Maximum tickets admitted to one bounded formation cohort.
        #[arg(long, default_value_t = 8)]
        maximum_tickets_per_cohort: usize,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Reconstruct a queue-grid coordination artifact and verify its exact conflict result.
    VerifyQueueGridCoordination { artifact: PathBuf },
    /// Generate a constraint-preserving scenario candidate from a seed.
    Generate {
        #[arg(long)]
        seed: u64,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Generate and execute a deterministic contiguous seed range.
    Sweep {
        /// First generated-scenario seed, included in the run.
        #[arg(long)]
        seed: u64,
        /// Number of generated scenarios to execute.
        #[arg(long)]
        count: u32,
        /// An empty directory that will receive one bundle per seed and a summary.
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, default_value_t = 10)]
        trace_every: u32,
    },
    /// Replicate one authored scenario over a contiguous seed range.
    Replicate {
        source: PathBuf,
        /// First runtime seed, included in the run.
        #[arg(long)]
        seed: u64,
        /// Number of deterministic replications to execute.
        #[arg(long)]
        count: u32,
        /// An empty directory that will receive one bundle per seed and a summary.
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, default_value_t = 10)]
        trace_every: u32,
    },
    /// Verify every run bundle and source recorded by a generated or authored sweep.
    VerifySweep { directory: PathBuf },
    /// Verify a sweep and emit exact descriptive aggregates; this is not a benchmark score.
    AnalyzeSweep {
        directory: PathBuf,
        /// Write the JSON report to a file instead of standard output.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Compare two compatible, verified authored replication sweeps by seed.
    CompareSweeps {
        /// Authored replication sweep representing the unchanged condition.
        baseline: PathBuf,
        /// Authored replication sweep representing the changed condition.
        candidate: PathBuf,
        /// Write the JSON report to a file instead of standard output.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Run a declared, uncalibrated sensitivity study over authored input alternatives.
    Sensitivity {
        /// JSON sensitivity manifest, with a baseline source relative to this file.
        manifest: PathBuf,
        /// An empty directory that will receive baseline, condition, and comparison artifacts.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Validate and resolve a sensitivity manifest without executing any runs.
    SensitivityPlan {
        /// JSON sensitivity manifest, with a baseline source relative to this file.
        manifest: PathBuf,
        /// Write the resolved plan as JSON instead of standard output.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Verify a complete sensitivity-study artifact against its manifest and sweeps.
    VerifySensitivity { directory: PathBuf },
    /// Create, plan, execute, or verify an uncalibrated authored experiment.
    Experiment {
        #[command(subcommand)]
        command: ExperimentCommand,
    },
    /// Verify an empirical benchmark round's evidence and seed-release contract.
    Benchmark {
        #[command(subcommand)]
        command: BenchmarkCommand,
    },
    /// Validate and content-lock open data; only evaluation catalogs can enter a round.
    Evidence {
        #[command(subcommand)]
        command: EvidenceCommand,
    },
    /// Inspect a content-locked public layout source without creating a scenario.
    Layout {
        #[command(subcommand)]
        command: LayoutCommand,
    },
    /// Produce an uncalibrated descriptive report from a content-locked reference source.
    Reference {
        #[command(subcommand)]
        command: ReferenceCommand,
    },
    /// Produce a descriptive, source-locked calibration intake report.
    Calibrate {
        #[command(subcommand)]
        command: CalibrateCommand,
    },
    /// Reconstruct and verify a run bundle before printing its summary.
    Replay {
        bundle: PathBuf,
        /// Permit a hash-only inspection of an incompatible legacy runtime artifact.
        #[arg(long)]
        allow_legacy_hash_only: bool,
    },
    /// Reconstruct a current run bundle and require zero reference-disc overlap audits.
    VerifyReferenceClearance { bundle: PathBuf },
}

const QUEUE_GRID_COORDINATION_ARTIFACT_SCHEMA: &str = "chiyoda.queue-grid-coordination.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct QueueGridCoordinationPolicy {
    first_departure_at_s: f64,
    headway_s: f64,
    roadmap_spacing_m: f64,
    maximum_roadmap_nodes: usize,
    planning_timestep_s: f64,
    maximum_low_level_expansions: u64,
    maximum_conflict_tree_nodes: u64,
    maximum_tickets_per_cohort: usize,
    clearance_epsilon_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct QueueGridCoordinationArtifact {
    schema: String,
    source: String,
    source_sha256: String,
    group: String,
    queue_grid: String,
    policy: QueueGridCoordinationPolicy,
    outcome: QueueGridCoordinationArtifactOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum QueueGridCoordinationArtifactOutcome {
    Planned {
        slot_windows: Vec<QueueGridSlotWindowArtifact>,
        trajectories: Vec<TimedDiscTrajectoryArtifact>,
        explored_conflict_tree_nodes: u64,
        low_level_explored_states: u64,
    },
    NoPlan {
        cohort_tickets: Vec<u64>,
    },
    Unresolved {
        cohort_tickets: Vec<u64>,
        reason: QueueGridUnresolvedReasonArtifact,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum QueueGridUnresolvedReasonArtifact {
    LowLevelSearchBoundExceeded {
        agent_id: String,
        target_index: usize,
        maximum_expansions: u64,
    },
    ConflictRepairBoundExceeded {
        maximum_conflict_tree_nodes: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct QueueGridSlotWindowArtifact {
    ticket: u64,
    starts_at_s: f64,
    ends_at_s: f64,
    slot_rank: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TimedDiscTrajectoryArtifact {
    agent_id: String,
    segments: Vec<TimedDiscSegmentArtifact>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TimedDiscSegmentArtifact {
    surface: String,
    starts_at_s: f64,
    ends_at_s: f64,
    start: [f64; 3],
    end: [f64; 3],
    radius_m: f64,
}

#[derive(Debug, Subcommand)]
enum BenchmarkCommand {
    Verify { manifest: PathBuf },
}

#[derive(Debug, Subcommand)]
enum EvidenceCommand {
    /// Validate source metadata and its declared empirical or source-only purpose.
    Verify { catalog: PathBuf },
    /// Validate a catalog and verify every acquired source's size and SHA-256.
    Lock {
        catalog: PathBuf,
        #[arg(long, default_value = "data/raw")]
        data_root: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum LayoutCommand {
    /// Preserve recognized OSM XML features as source observations, not DSL geometry.
    Osm {
        /// An uncalibrated-reference `ODbL` catalog with one acquired OSM XML file.
        catalog: PathBuf,
        #[arg(long, default_value = "data/raw")]
        data_root: PathBuf,
        /// Deliberately raise only for a reviewed extract larger than a station area.
        #[arg(long, default_value_t = 250_000)]
        max_nodes: usize,
        /// Deliberately raise only for a reviewed extract larger than a station area.
        #[arg(long, default_value_t = 50_000)]
        max_ways: usize,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Reconstruct and verify a source-observation report from its locked OSM XML.
    VerifyOsm {
        /// The `ODbL` source catalog used to generate the report.
        catalog: PathBuf,
        /// Existing source-observation JSON report to reconstruct and compare.
        report: PathBuf,
        #[arg(long, default_value = "data/raw")]
        data_root: PathBuf,
    },
    /// Derive an explicitly anchored local east/north reference from a verified OSM observation report.
    ProjectOsm {
        /// The `ODbL` source catalog used to generate the observation report.
        catalog: PathBuf,
        /// Existing source-observation JSON report to verify before projection.
        report: PathBuf,
        #[arg(long, default_value = "data/raw")]
        data_root: PathBuf,
        /// Reviewed WGS84 latitude for the local tangent-plane origin, in degrees.
        #[arg(long)]
        origin_latitude: f64,
        /// Reviewed WGS84 longitude for the local tangent-plane origin, in degrees.
        #[arg(long)]
        origin_longitude: f64,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Verify a local-coordinate reference against its verified OSM observation report.
    VerifyProjection {
        /// The `ODbL` source catalog used to generate the observation report.
        catalog: PathBuf,
        /// Existing source-observation JSON report that anchors the projection.
        report: PathBuf,
        /// Existing local-coordinate reference JSON report to reconstruct and compare.
        projection: PathBuf,
        #[arg(long, default_value = "data/raw")]
        data_root: PathBuf,
    },
    /// Prove selected scenario x/y points equal selected projected OSM points without importing geometry.
    AnchorOsm {
        /// The `ODbL` source catalog used to generate the observation report.
        catalog: PathBuf,
        /// Existing source-observation JSON report to verify before anchoring.
        observation: PathBuf,
        /// Existing local-coordinate reference JSON report to verify before anchoring.
        projection: PathBuf,
        /// JSON anchor manifest; its scenario path resolves relative to this file.
        manifest: PathBuf,
        #[arg(long, default_value = "data/raw")]
        data_root: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Reconstruct an OSM scenario-anchor report from its manifest, scenario, and verified local projection.
    VerifyAnchorOsm {
        /// The `ODbL` source catalog used to generate the observation report.
        catalog: PathBuf,
        /// Existing source-observation JSON report to verify before anchoring.
        observation: PathBuf,
        /// Existing local-coordinate reference JSON report to verify before anchoring.
        projection: PathBuf,
        /// JSON anchor manifest; its scenario path resolves relative to this file.
        manifest: PathBuf,
        /// Existing OSM scenario-anchor JSON report to reconstruct and compare.
        anchor_report: PathBuf,
        #[arg(long, default_value = "data/raw")]
        data_root: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum ExperimentCommand {
    /// Create a no-data, best-guess experiment draft from a deterministic generated scenario.
    Init {
        /// Human-readable experiment name for the initial manifest.
        #[arg(long)]
        name: String,
        /// Deterministic scenario-generator seed.
        #[arg(long)]
        seed: u64,
        /// An empty directory that will receive scenario.chy, experiment.json, and optional sensitivity.json.
        #[arg(short, long)]
        output: PathBuf,
        /// Persist every N runtime steps in a later experiment artifact.
        #[arg(long, default_value_t = 10)]
        trace_every: u32,
        /// Also create a one-at-a-time, no-data sensitivity manifest for the generated draft.
        #[arg(long)]
        with_sensitivity: bool,
        /// Replications per condition in the optional starter sensitivity study (default: 8).
        #[arg(long, requires = "with_sensitivity", value_name = "COUNT")]
        sensitivity_runs: Option<u32>,
    },
    /// Validate one experiment and its declared sources without executing the runtime.
    Plan {
        /// JSON experiment manifest; relative paths resolve from this file.
        manifest: PathBuf,
        /// Write the resolved plan as JSON instead of standard output.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Snapshot one scenario, its assumptions, and declared source reports with a run bundle.
    Run {
        /// JSON experiment manifest; relative paths resolve from this file.
        manifest: PathBuf,
        /// An empty directory that will receive the complete experiment artifact.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Reconstruct and verify every immutable part of an experiment artifact.
    Verify { directory: PathBuf },
}

#[derive(Debug, Subcommand)]
enum CalibrateCommand {
    /// Summarize the locked Eindhoven Centraal platform trajectories.
    EindhovenPlatform {
        catalog: PathBuf,
        #[arg(long, default_value = "data/raw")]
        data_root: PathBuf,
        /// Process one role only. Calibration is the safe default; held-out data
        /// should be inspected only after a protocol has frozen the model.
        #[arg(long, value_enum, default_value_t = EvidencePartition::Calibration)]
        partition: EvidencePartition,
        #[arg(short, long)]
        output: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum ReferenceCommand {
    /// Summarize the source-locked VRU pedestrian trajectories without calibrating the runtime.
    VruTrajectory {
        catalog: PathBuf,
        #[arg(long, default_value = "data/raw")]
        data_root: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Summarize the locked Wuppertal controlled-bottleneck trajectories without calibrating the runtime.
    CrowdQueue {
        catalog: PathBuf,
        #[arg(long, default_value = "data/raw")]
        data_root: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EvidencePartition {
    Calibration,
    HeldOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SweepSummary {
    schema_version: String,
    generator_version: String,
    #[serde(default, skip_serializing_if = "SweepSource::is_generated")]
    source: SweepSource,
    first_seed: u64,
    count: u32,
    trace_every_steps: u32,
    runs: Vec<SweepRun>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum SweepSource {
    #[default]
    Generated,
    Authored {
        template_scenario_hash: String,
    },
}

impl SweepSource {
    fn is_generated(&self) -> bool {
        matches!(self, Self::Generated)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SweepRun {
    seed: u64,
    scenario_name: String,
    bundle_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bundle_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    runtime_version: Option<String>,
    total_agents: u32,
    evacuated_agents: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    evacuated_by_exit: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    remaining_by_state: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    information_delivery: BTreeMap<String, InformationDeliveryMetrics>,
    /// Current sweeps retain whether an agent ever entered each modeled queue.
    /// Older summaries lack these fields and remain explicitly distinguishable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    queue_experience: Option<QueueExperience>,
    /// Full discrete queue telemetry for current bundles. Older bundles omit
    /// it rather than implying a physical or zero-valued queue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    queue_metrics: Option<QueueMetrics>,
    /// Local-clearance-resolver telemetry for current bundles. Its absence in
    /// historical summaries is preserved rather than inferred from traces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    movement_metrics: Option<MovementMetrics>,
    clearance_time_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_exit_time_s: Option<f64>,
}

#[derive(Debug, Serialize)]
struct SweepAnalysis {
    schema_version: String,
    input_sweep_schema_version: String,
    generator_version: String,
    source: SweepSource,
    first_seed: u64,
    run_count: u32,
    total_agents: u64,
    evacuated_agents: u64,
    un_evacuated_agents: u64,
    overall_evacuation_fraction: ExactRatio,
    runs_with_any_evacuation: u32,
    fully_evacuated_runs: u32,
    evacuated_by_exit: BTreeMap<String, u64>,
    unattributed_evacuations: u64,
    remaining_by_state: BTreeMap<String, u64>,
    unattributed_remaining_agents: u64,
    information_delivery: BTreeMap<String, AggregateInformationDelivery>,
    queue_experience: AggregateQueueExperience,
    queue_telemetry: AggregateQueueTelemetry,
    movement_telemetry: AggregateMovementTelemetry,
    clearance_time_s: Option<DescriptiveRange>,
    last_exit_time_s: Option<DescriptiveRange>,
    claim_boundary: String,
}

#[derive(Debug, Serialize)]
struct ExactRatio {
    numerator: u64,
    denominator: u64,
}

#[derive(Debug, Clone, Serialize)]
struct DescriptiveRange {
    measured_runs: u32,
    minimum_s: f64,
    mean_s: f64,
    maximum_s: f64,
}

#[derive(Debug, Serialize)]
struct AggregateInformationDelivery {
    received_agents: u64,
    accepted_agents: u64,
}

/// Counts agents that entered a modeled wait state at least once in one run.
/// It is not a peak queue length, a waiting-time distribution, or a physical
/// flow measurement.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)] // fields intentionally mirror persisted runtime metrics
struct QueueExperience {
    queued_for_lift_agents: u32,
    queued_for_connector_agents: u32,
    queued_for_gate_agents: u32,
    queued_for_exit_agents: u32,
}

#[derive(Debug, Clone, Serialize)]
struct AggregateQueueExperience {
    observed_runs: u32,
    unobserved_legacy_runs: u32,
    queued_for_lift_agents: u64,
    queued_for_connector_agents: u64,
    queued_for_gate_agents: u64,
    queued_for_exit_agents: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
struct AggregateQueueTelemetry {
    observed_runs: u32,
    unobserved_legacy_runs: u32,
    lift: AggregateQueueResourceTelemetry,
    connector: AggregateQueueResourceTelemetry,
    gate: AggregateQueueResourceTelemetry,
    exit: AggregateQueueResourceTelemetry,
    by_resource: AggregateQueueResourceAttribution,
}

#[derive(Debug, Clone, Default, Serialize)]
struct AggregateQueueResourceTelemetry {
    ever_queued_agents: u64,
    cumulative_wait_agent_seconds: f64,
    maximum_peak_waiting_agents: u32,
}

/// Aggregate coverage and totals for structural local-clearance adjustments.
/// These are runtime-algorithm observations, not physical crowd metrics.
#[derive(Debug, Clone, Default, Serialize)]
struct AggregateMovementTelemetry {
    observed_runs: u32,
    unobserved_legacy_runs: u32,
    agents_with_local_clearance_adjustments: u64,
    local_clearance_adjustment_steps: u64,
    /// Coverage for the 0.31 ORCA infeasibility counter is separate from the
    /// older local-motion telemetry contract.
    constraint_fallback_observed_runs: u32,
    constraint_fallback_unobserved_legacy_runs: u32,
    local_avoidance_constraint_fallback_steps: u64,
    /// Coverage for the 0.36 integration-boundary reference-disc audit is
    /// independent from older local-motion telemetry.
    on_surface_clearance_audit_observed_runs: u32,
    on_surface_clearance_audit_unobserved_legacy_runs: u32,
    /// Sum of per-run distinct-agent counts; this is not a cross-run unique
    /// population count.
    agents_with_on_surface_disc_overlaps: u64,
    on_surface_disc_overlap_pair_steps: u64,
    maximum_on_surface_disc_overlap_m: f64,
    /// Coverage for the 0.37 analytic same-surface interval audit is distinct
    /// from the 0.36 integration-boundary audit.
    swept_on_surface_clearance_audit_observed_runs: u32,
    swept_on_surface_clearance_audit_unobserved_legacy_runs: u32,
    /// Sum of per-run distinct-agent counts; this is not a cross-run unique
    /// population count.
    agents_with_swept_disc_overlaps: u64,
    swept_disc_overlap_pair_steps: u64,
    maximum_swept_disc_overlap_m: f64,
    cumulative_local_clearance_adjustment_m: f64,
    maximum_local_clearance_adjustment_m: f64,
}

/// Resource-attributed queue aggregates are separately coverage-labeled because
/// 0.22 detailed telemetry did not include the resource identity.
#[derive(Debug, Clone, Default, Serialize)]
struct AggregateQueueResourceAttribution {
    observed_runs: u32,
    unobserved_legacy_runs: u32,
    lifts: BTreeMap<String, AggregateQueueResourceTelemetry>,
    connectors: BTreeMap<String, AggregateQueueResourceTelemetry>,
    gates: BTreeMap<String, AggregateQueueResourceTelemetry>,
    exits: BTreeMap<String, AggregateQueueResourceTelemetry>,
}

#[derive(Debug, Clone, Serialize)]
#[allow(clippy::struct_field_names)] // fields intentionally mirror persisted runtime metrics
struct QueueExperienceDelta {
    queued_for_lift_agents: i64,
    queued_for_connector_agents: i64,
    queued_for_gate_agents: i64,
    queued_for_exit_agents: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
struct QueueTelemetryDelta {
    lift: QueueResourceTelemetryDelta,
    connector: QueueResourceTelemetryDelta,
    gate: QueueResourceTelemetryDelta,
    exit: QueueResourceTelemetryDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    by_resource: Option<QueueResourceTelemetryDeltaBreakdown>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct QueueResourceTelemetryDelta {
    ever_queued_agents: i64,
    cumulative_wait_agent_seconds: f64,
    maximum_peak_waiting_agents: i64,
}

/// Candidate-minus-baseline change in local-motion telemetry.
/// The maximum field compares the largest per-attempt adjustment in each arm;
/// the other fields sum per-run values across paired seed records.
#[derive(Debug, Clone, Default, Serialize)]
struct MovementTelemetryDelta {
    agents_with_local_clearance_adjustments: i64,
    local_clearance_adjustment_steps: i128,
    /// Present only when every paired run has the 0.31 fallback counter.
    #[serde(skip_serializing_if = "Option::is_none")]
    local_avoidance_constraint_fallback_steps: Option<i128>,
    /// Present only when every paired run has the 0.36 reference-disc audit.
    #[serde(skip_serializing_if = "Option::is_none")]
    on_surface_clearance_audit: Option<OnSurfaceClearanceAuditDelta>,
    /// Present only when every paired run has the 0.37 analytic interval audit.
    #[serde(skip_serializing_if = "Option::is_none")]
    swept_on_surface_clearance_audit: Option<SweptOnSurfaceClearanceAuditDelta>,
    cumulative_local_clearance_adjustment_m: f64,
    maximum_local_clearance_adjustment_m: f64,
}

/// Candidate-minus-baseline change in integration-boundary reference-disc
/// audit telemetry. Counts sum per-run values; the maximum compares each arm's
/// largest per-run overlap.
#[derive(Debug, Clone, Default, Serialize)]
struct OnSurfaceClearanceAuditDelta {
    agents_with_disc_overlaps: i64,
    disc_overlap_pair_steps: i128,
    maximum_disc_overlap_m: f64,
}

/// Candidate-minus-baseline change in analytic same-surface linear-interval
/// reference-disc audit telemetry. Counts sum per-run values; the maximum
/// compares each arm's largest per-run overlap.
#[derive(Debug, Clone, Default, Serialize)]
struct SweptOnSurfaceClearanceAuditDelta {
    agents_with_swept_disc_overlaps: i64,
    swept_disc_overlap_pair_steps: i128,
    maximum_swept_disc_overlap_m: f64,
}

/// Candidate-minus-baseline queue deltas for individual resource identifiers.
/// A resource introduced or removed by an authored intervention has an explicit
/// declaration flag instead of being presented as a shared resource with zero
/// activity.
#[derive(Debug, Clone, Default, Serialize)]
struct QueueResourceTelemetryDeltaBreakdown {
    lifts: BTreeMap<String, AttributedQueueResourceTelemetryDelta>,
    connectors: BTreeMap<String, AttributedQueueResourceTelemetryDelta>,
    gates: BTreeMap<String, AttributedQueueResourceTelemetryDelta>,
    exits: BTreeMap<String, AttributedQueueResourceTelemetryDelta>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct AttributedQueueResourceTelemetryDelta {
    baseline_resource_declared: bool,
    candidate_resource_declared: bool,
    ever_queued_agents: i64,
    cumulative_wait_agent_seconds: f64,
    maximum_peak_waiting_agents: i64,
}

#[derive(Debug, Serialize)]
struct SweepComparison {
    schema_version: String,
    pairing: SweepPairing,
    baseline: SweepAnalysis,
    candidate: SweepAnalysis,
    paired_runs: Vec<PairedRun>,
    aggregate: PairedAggregate,
    claim_boundary: String,
}

#[derive(Debug, Serialize)]
struct SensitivityReport {
    schema_version: String,
    study_name: String,
    description: String,
    manifest_snapshot: String,
    design: chiyoda_core::SensitivityDesign,
    baseline: SensitivityBaseline,
    first_seed: u64,
    run_count_per_condition: u32,
    trace_every_steps: u32,
    reference_report_snapshots: Vec<SensitivityReferenceReportSnapshot>,
    factors: Vec<SensitivityFactorReport>,
    conditions: Vec<SensitivityConditionReport>,
    one_at_a_time_responses: Option<Vec<SensitivityFactorResponse>>,
    author_claim_boundary: String,
    claim_boundary: String,
}

#[derive(Debug, Serialize)]
struct SensitivityPlanReport {
    schema_version: String,
    study_name: String,
    description: String,
    manifest: String,
    design: chiyoda_core::SensitivityDesign,
    baseline: SensitivityBaseline,
    first_seed: u64,
    run_count_per_condition: u32,
    execution: SensitivityPlanExecution,
    trace_every_steps: u32,
    reference_report_snapshots: Vec<SensitivityReferenceReportSnapshot>,
    factors: Vec<SensitivityFactorReport>,
    conditions: Vec<SensitivityPlanCondition>,
    author_claim_boundary: String,
    claim_boundary: String,
}

/// Exact deterministic work queued by a sensitivity study, not a runtime-time estimate.
#[derive(Debug, Serialize)]
struct SensitivityPlanExecution {
    baseline_runs: u64,
    condition_count: u64,
    condition_runs: u64,
    total_runs: u64,
    integration_steps_per_run: u64,
    stored_trace_frames_per_run: u64,
    total_integration_steps: u64,
    total_stored_trace_frames: u64,
}

#[derive(Debug, Serialize)]
struct SensitivityPlanCondition {
    id: String,
    factor_values: BTreeMap<String, f64>,
    template_scenario_hash: String,
}

#[derive(Debug, Clone, Serialize)]
struct SensitivityReferenceReportSnapshot {
    factor_id: String,
    reference_id: String,
    source_path: String,
    snapshot_path: String,
    sha256: String,
}

#[derive(Debug)]
struct CapturedSensitivityReferenceReport {
    snapshot: SensitivityReferenceReportSnapshot,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
struct ExperimentSourceReportSnapshot {
    source_id: String,
    source_path: String,
    snapshot_path: String,
    sha256: String,
}

#[derive(Debug)]
struct CapturedExperimentSourceReport {
    snapshot: ExperimentSourceReportSnapshot,
    bytes: Vec<u8>,
}

#[derive(Debug)]
enum CapturedExperimentOsmAttestation {
    LocalProjection {
        source_id: String,
        catalog_bytes: Vec<u8>,
        observation_bytes: Vec<u8>,
    },
    ScenarioAnchor {
        source_id: String,
        anchor_manifest_bytes: Vec<u8>,
    },
}

impl CapturedExperimentOsmAttestation {
    fn source_id(&self) -> &str {
        match self {
            Self::LocalProjection { source_id, .. } | Self::ScenarioAnchor { source_id, .. } => {
                source_id
            }
        }
    }
}

const EXPERIMENT_PLAN_SCHEMA_VERSION: &str = "0.3";
const EXPERIMENT_REPORT_SCHEMA_VERSION: &str = "0.4";

/// A reviewable, non-mutating preflight for one uncalibrated experiment.
#[derive(Debug, Serialize)]
struct ExperimentPlanReport {
    schema_version: String,
    experiment_name: String,
    description: String,
    manifest: String,
    scenario_source: String,
    scenario_hash: String,
    trace_every_steps: u32,
    execution: ExperimentPlanExecution,
    scenario: ExperimentScenarioInventory,
    assumptions: Vec<chiyoda_core::ExperimentAssumption>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    resolved_assumption_targets: Vec<ExperimentResolvedAssumptionTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sensitivity_coverage: Option<ExperimentSensitivityCoverage>,
    sources: Vec<chiyoda_core::SensitivityReference>,
    source_report_snapshots: Vec<ExperimentSourceReportSnapshot>,
    source_attestations: Vec<ExperimentSourceAttestation>,
    verified_osm_source_attestations: Vec<String>,
    author_claim_boundary: String,
    claim_boundary: String,
}

/// Exact reference-runtime work implied by the authored timing contract.
#[derive(Debug, Serialize)]
struct ExperimentPlanExecution {
    duration_s: f64,
    timestep_s: f64,
    integration_steps: u64,
    stored_trace_frames: u64,
}

fn stored_trace_frame_count(integration_steps: u64, trace_every_steps: u32) -> Result<u64> {
    if trace_every_steps == 0 {
        bail!("trace cadence must be greater than zero");
    }
    let cadence = u64::from(trace_every_steps);
    let periodic_trace_frames = integration_steps / cadence;
    let final_trace_frames = u64::from(!integration_steps.is_multiple_of(cadence));
    1_u64
        .checked_add(periodic_trace_frames)
        .and_then(|frames| frames.checked_add(final_trace_frames))
        .context("trace-frame count overflowed")
}

/// Counts only declared scenario structure; it does not estimate a facility,
/// population, or outcome before the runtime is invoked.
#[derive(Debug, Serialize)]
struct ExperimentScenarioInventory {
    surfaces: usize,
    obstacles: usize,
    waypoints: usize,
    exits: usize,
    connectors: usize,
    gates: usize,
    agent_groups: usize,
    declared_agents: u64,
    connector_state_changes: usize,
    exit_state_changes: usize,
    connector_capacity_changes: usize,
    exit_capacity_changes: usize,
    gate_state_changes: usize,
    gate_capacity_changes: usize,
    messages: usize,
    countermeasures: usize,
}

/// A resolved, exact baseline value for one optional typed assumption target.
/// This makes an uncalibrated choice inspectable without assigning it a
/// probability or an empirical interpretation.
#[derive(Debug, Clone, Serialize)]
struct ExperimentResolvedAssumptionTarget {
    assumption_id: String,
    target: SensitivityTarget,
    subject: String,
    baseline_value: f64,
    unit: String,
}

/// A factor declared by a linked sensitivity-study manifest, resolved against
/// the same canonical experiment scenario. Its values are authored
/// alternatives, not a probability distribution or completed-study outcomes.
#[derive(Debug, Clone, Serialize)]
struct ExperimentSensitivityFactorCoverage {
    factor_id: String,
    target: SensitivityTarget,
    subject: String,
    baseline_value: f64,
    unit: String,
    values: Vec<f64>,
}

/// The exact factor that examines one typed experiment assumption target.
#[derive(Debug, Clone, Serialize)]
struct ExperimentSensitivityFactorLink {
    study_id: String,
    factor_id: String,
}

/// Coverage status for one typed assumption. An empty `sensitivity_factors`
/// deliberately records a disclosed input that a linked study does not vary.
#[derive(Debug, Clone, Serialize)]
struct ExperimentAssumptionSensitivityCoverage {
    assumption_id: String,
    target: SensitivityTarget,
    subject: String,
    baseline_value: f64,
    unit: String,
    sensitivity_factors: Vec<ExperimentSensitivityFactorLink>,
}

/// One sensitivity-study contract captured beside an experiment artifact.
#[derive(Debug, Clone, Serialize)]
struct ExperimentSensitivityStudyCoverage {
    study_id: String,
    declared_manifest_path: String,
    manifest_snapshot: String,
    manifest_sha256: String,
    baseline_source_snapshot: String,
    baseline_source_sha256: String,
    baseline_scenario_hash: String,
    design: chiyoda_core::SensitivityDesign,
    condition_count: usize,
    factors: Vec<ExperimentSensitivityFactorCoverage>,
}

/// A reviewable crosswalk between best-guess experiment inputs and declared
/// sensitivity factors. It reports coverage only; it does not assert that a
/// study was executed or establish uncertainty quantification.
#[derive(Debug, Clone, Serialize)]
struct ExperimentSensitivityCoverage {
    studies: Vec<ExperimentSensitivityStudyCoverage>,
    assumption_targets: Vec<ExperimentAssumptionSensitivityCoverage>,
}

#[derive(Debug)]
struct CapturedExperimentSensitivityStudy {
    coverage: ExperimentSensitivityStudyCoverage,
    manifest_bytes: Vec<u8>,
    baseline_source_bytes: Vec<u8>,
}

#[derive(Debug, Serialize)]
struct ExperimentReportBase {
    schema_version: String,
    experiment_name: String,
    description: String,
    manifest_snapshot: String,
    manifest_sha256: String,
    scenario_snapshot: String,
    scenario_source_sha256: String,
    scenario_hash: String,
    run_bundle: String,
    bundle_hash: String,
    trace_every_steps: u32,
    source_report_snapshots: Vec<ExperimentSourceReportSnapshot>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    resolved_assumption_targets: Vec<ExperimentResolvedAssumptionTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sensitivity_coverage: Option<ExperimentSensitivityCoverage>,
    author_claim_boundary: String,
    claim_boundary: String,
}

/// The current report gives readers exact deterministic outcomes without
/// requiring them to independently parse `run.json`. Verification rebuilds it
/// from that bundle, so it cannot drift from the authoritative artifact.
#[derive(Debug, Serialize)]
struct ExperimentReport {
    #[serde(flatten)]
    base: ExperimentReportBase,
    runtime_metrics: RunMetrics,
}

/// Version 0.1 reports predate the human-readable metrics mirror. Keep their
/// reconstruction path so existing artifacts remain independently verifiable.
#[derive(Debug, Serialize)]
struct LegacyExperimentReport {
    #[serde(flatten)]
    base: ExperimentReportBase,
}

#[derive(Debug, Serialize)]
struct SensitivityBaseline {
    source: String,
    template_scenario_hash: String,
    sweep_directory: String,
}

#[derive(Debug, Serialize)]
struct SensitivityFactorReport {
    factor: SensitivityFactor,
    baseline_value: f64,
    unit: String,
}

#[derive(Debug, Serialize)]
struct SensitivityConditionReport {
    id: String,
    factor_values: BTreeMap<String, f64>,
    template_scenario_hash: String,
    sweep_directory: String,
    comparison_path: String,
    outcome: SensitivityOutcome,
}

#[derive(Debug, Serialize)]
struct SensitivityFactorResponse {
    factor_id: String,
    baseline_value: f64,
    unit: String,
    alternatives: Vec<SensitivityResponseObservation>,
}

#[derive(Debug, Serialize)]
struct SensitivityResponseObservation {
    value: f64,
    outcome: SensitivityOutcome,
}

#[derive(Debug, Clone, Serialize)]
struct SensitivityOutcome {
    evacuated_agents_delta: i64,
    un_evacuated_agents_delta: i64,
    baseline_fully_evacuated_runs: u32,
    candidate_fully_evacuated_runs: u32,
    queue_experience_delta: Option<QueueExperienceDelta>,
    queue_telemetry_delta: Option<QueueTelemetryDelta>,
    movement_telemetry_delta: Option<MovementTelemetryDelta>,
    clearance_time_s: SensitivityTiming,
    last_exit_time_s: SensitivityTiming,
}

#[derive(Debug, Clone, Serialize)]
struct SensitivityTiming {
    both_recorded_runs: u32,
    baseline_only_recorded_runs: u32,
    candidate_only_recorded_runs: u32,
    neither_recorded_runs: u32,
    candidate_earlier_runs: u32,
    candidate_later_runs: u32,
    unchanged_runs: u32,
    candidate_minus_baseline_s: Option<DescriptiveRange>,
}

#[derive(Debug, Serialize)]
struct SweepPairing {
    first_seed: u64,
    run_count: u32,
    execution_contract: ExecutionContract,
    baseline_template_scenario_hash: String,
    candidate_template_scenario_hash: String,
    /// Whether all authored agent declarations match. The ordinary comparison
    /// command requires this; sensitivity comparisons may vary one declared
    /// agent input and record the two denominators per seed instead.
    agent_declarations_matched: bool,
    changed_scenario_sections: Vec<String>,
    information_sampling: InformationSamplingAlignment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ExecutionContract {
    bundle_version: String,
    runtime_version: String,
}

#[derive(Debug, Serialize)]
struct InformationSamplingAlignment {
    shared: BTreeMap<String, SamplingPair>,
    baseline_only: BTreeMap<String, SamplingDeclaration>,
    candidate_only: BTreeMap<String, SamplingDeclaration>,
}

#[derive(Debug, Serialize)]
struct SamplingPair {
    baseline: SamplingDeclaration,
    candidate: SamplingDeclaration,
}

#[derive(Debug, Serialize)]
struct SamplingDeclaration {
    intervention: String,
    kind: String,
}

#[derive(Debug, Serialize)]
struct PairedRun {
    seed: u64,
    baseline_total_agents: u32,
    candidate_total_agents: u32,
    baseline: PairedRunArm,
    candidate: PairedRunArm,
    candidate_minus_baseline: PairedRunDelta,
}

#[derive(Debug, Serialize)]
struct PairedRunArm {
    bundle_hash: String,
    evacuated_agents: u32,
    evacuated_by_exit: BTreeMap<String, u32>,
    remaining_by_state: BTreeMap<String, u32>,
    information_delivery: BTreeMap<String, InformationDeliveryMetrics>,
    queue_metrics: Option<QueueMetrics>,
    movement_metrics: Option<MovementMetrics>,
    clearance_time_s: Option<f64>,
    last_exit_time_s: Option<f64>,
}

#[derive(Debug, Serialize)]
struct PairedRunDelta {
    evacuated_agents: i64,
    un_evacuated_agents: i64,
    clearance_time_s: Option<f64>,
    last_exit_time_s: Option<f64>,
}

#[derive(Debug, Serialize)]
struct PairedAggregate {
    candidate_minus_baseline: AggregateDelta,
    runs_with_more_candidate_evacuations: u32,
    runs_with_fewer_candidate_evacuations: u32,
    runs_with_unchanged_evacuations: u32,
    clearance_time_s: PairedTime,
    last_exit_time_s: PairedTime,
}

#[derive(Debug, Serialize)]
struct AggregateDelta {
    evacuated_agents: i64,
    un_evacuated_agents: i64,
    evacuated_by_exit: BTreeMap<String, i64>,
    remaining_by_state: BTreeMap<String, i64>,
    information_delivery: BTreeMap<String, InformationDeliveryDelta>,
    queue_experience: Option<QueueExperienceDelta>,
    queue_telemetry: Option<QueueTelemetryDelta>,
    movement_telemetry: Option<MovementTelemetryDelta>,
}

#[derive(Debug, Serialize)]
struct InformationDeliveryDelta {
    received_agents: i64,
    accepted_agents: i64,
}

#[derive(Debug, Serialize)]
struct PairedTime {
    both_recorded_runs: u32,
    baseline_only_recorded_runs: u32,
    candidate_only_recorded_runs: u32,
    neither_recorded_runs: u32,
    candidate_earlier_runs: u32,
    candidate_later_runs: u32,
    unchanged_runs: u32,
    candidate_minus_baseline_s: Option<DescriptiveRange>,
}

#[derive(Default)]
struct PairedTimeAccumulator {
    both_recorded_runs: u32,
    baseline_only_recorded_runs: u32,
    candidate_only_recorded_runs: u32,
    neither_recorded_runs: u32,
    candidate_earlier_runs: u32,
    candidate_later_runs: u32,
    unchanged_runs: u32,
    deltas: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComparisonPolicy {
    RequireMatchingAgentDeclarations,
    AllowSensitivityAgentDeclarationChanges,
}

impl ComparisonPolicy {
    fn allows_agent_declaration_changes(self) -> bool {
        self == Self::AllowSensitivityAgentDeclarationChanges
    }
}

#[allow(clippy::too_many_lines)] // top-level command dispatch is intentionally visible in one place
fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { source } => {
            let scenario = read_scenario(&source)?;
            println!("valid: {}", scenario.name);
        }
        Command::Compile { source, output } => {
            let scenario = read_scenario(&source)?;
            write_json(&output, &CanonicalScenario::from(scenario))?;
            println!("compiled: {}", output.display());
        }
        Command::Format {
            source,
            output,
            check,
        } => {
            let scenario = read_scenario(&source)?;
            let formatted = format_scenario(&scenario);
            if check {
                let original = read_text(&source)?;
                if original != formatted {
                    bail!(
                        "{} is not canonically formatted; run `chiyoda format {}`",
                        source.display(),
                        source.display()
                    );
                }
                println!("formatted: {}", source.display());
            } else if let Some(output) = output {
                fs::write(&output, formatted)
                    .with_context(|| format!("writing formatted source {}", output.display()))?;
                println!("formatted: {}", output.display());
            } else {
                print!("{formatted}");
            }
        }
        Command::Run {
            source,
            output,
            trace_every,
        } => {
            let text = read_text(&source)?;
            let scenario = parse(&text).map_err(|error| anyhow::anyhow!(error))?;
            validate(&scenario).map_err(|errors| validation_error(&errors))?;
            let bundle = run(
                &scenario,
                RunOptions {
                    trace_every_steps: trace_every,
                },
            )?;
            fs::create_dir_all(&output)
                .with_context(|| format!("creating output directory {}", output.display()))?;
            fs::write(output.join("scenario.chy"), text)
                .with_context(|| format!("writing source into {}", output.display()))?;
            write_json(&output.join("run.json"), &bundle)?;
            println!("run: {}", output.join("run.json").display());
            println!("bundle hash: {}", bundle.bundle_hash);
            println!(
                "evacuated: {}/{}",
                bundle.metrics.evacuated_agents, bundle.metrics.total_agents
            );
            print_local_clearance_summary(&bundle.metrics);
        }
        Command::CoordinateQueueGrid {
            source,
            group,
            queue_grid,
            first_departure_at_s,
            headway_s,
            roadmap_spacing_m,
            maximum_roadmap_nodes,
            planning_timestep_s,
            maximum_low_level_expansions,
            maximum_conflict_tree_nodes,
            maximum_tickets_per_cohort,
            output,
        } => {
            let source_text = read_text(&source)?;
            let policy = QueueGridCoordinationPolicy {
                first_departure_at_s,
                headway_s,
                roadmap_spacing_m,
                maximum_roadmap_nodes,
                planning_timestep_s,
                maximum_low_level_expansions,
                maximum_conflict_tree_nodes,
                maximum_tickets_per_cohort,
                clearance_epsilon_m: reference_clearance_epsilon_m(),
            };
            let artifact =
                build_queue_grid_coordination_artifact(source_text, group, queue_grid, policy)?;
            write_json(&output, &artifact)?;
            print_queue_grid_coordination_outcome(&artifact.outcome);
            println!("coordination artifact: {}", output.display());
        }
        Command::VerifyQueueGridCoordination { artifact } => {
            let artifact: QueueGridCoordinationArtifact = read_json(&artifact)?;
            verify_queue_grid_coordination_artifact(&artifact)?;
            print_queue_grid_coordination_outcome(&artifact.outcome);
            println!("verified coordination artifact: {}", artifact.schema);
        }
        Command::Generate { seed, output } => {
            let source = generator::source(seed);
            let scenario = generator::scenario(seed)?;
            fs::write(&output, source)
                .with_context(|| format!("writing generated scenario {}", output.display()))?;
            println!("generated: {} ({})", output.display(), scenario.name);
        }
        Command::Sweep {
            seed,
            count,
            output,
            trace_every,
        } => run_sweep(seed, count, &output, trace_every)?,
        Command::Replicate {
            source,
            seed,
            count,
            output,
            trace_every,
        } => run_replicates(&source, seed, count, &output, trace_every)?,
        Command::VerifySweep { directory } => verify_sweep(&directory)?,
        Command::AnalyzeSweep { directory, output } => {
            analyze_sweep(&directory, output.as_deref())?;
        }
        Command::CompareSweeps {
            baseline,
            candidate,
            output,
        } => compare_sweeps(&baseline, &candidate, output.as_deref())?,
        Command::Sensitivity { manifest, output } => run_sensitivity(&manifest, &output)?,
        Command::SensitivityPlan { manifest, output } => {
            sensitivity_plan(&manifest, output.as_deref())?;
        }
        Command::VerifySensitivity { directory } => verify_sensitivity(&directory)?,
        Command::Experiment { command } => handle_experiment(command)?,
        Command::Benchmark { command } => match command {
            BenchmarkCommand::Verify { manifest } => verify_benchmark(&manifest)?,
        },
        Command::Evidence { command } => handle_evidence(command)?,
        Command::Layout { command } => handle_layout(command)?,
        Command::Reference { command } => handle_reference(command)?,
        Command::Calibrate { command } => handle_calibration(command)?,
        Command::Replay {
            bundle: bundle_path,
            allow_legacy_hash_only,
        } => {
            let bundle: RunBundle = read_json(&bundle_path)?;
            match verify_run_bundle(&bundle)? {
                BundleVerification::Reconstructed => {}
                BundleVerification::HashOnlyLegacy if allow_legacy_hash_only => {
                    eprintln!(
                        "warning: bundle uses an incompatible runtime contract; only its hash was verified"
                    );
                }
                BundleVerification::HashOnlyLegacy => {
                    bail!(
                        "bundle uses an incompatible runtime contract and cannot be reconstructed; pass --allow-legacy-hash-only only to inspect its hash"
                    );
                }
            }
            println!("verified reconstruction: {}", bundle.bundle_hash);
            println!("scenario: {}", bundle.scenario.scenario.name);
            println!("frames: {}", bundle.trace.len());
            println!(
                "evacuated: {}/{}",
                bundle.metrics.evacuated_agents, bundle.metrics.total_agents
            );
            print_local_clearance_summary(&bundle.metrics);
            println!("open with: chiyoda-replay {}", bundle_path.display());
        }
        Command::VerifyReferenceClearance {
            bundle: bundle_path,
        } => {
            let bundle: RunBundle = read_json(&bundle_path)?;
            if verify_run_bundle(&bundle)? != BundleVerification::Reconstructed {
                bail!(
                    "reference-clearance acceptance requires a bundle compatible with this runtime"
                );
            }
            require_zero_reference_clearance(&bundle)?;
            println!("reference-clearance audits pass: {}", bundle.bundle_hash);
        }
    }
    Ok(())
}

/// Require the runtime's two reference-disc audits to contain no positive
/// overlap. This is a deterministic acceptance boundary for a reconstructed
/// reference run, not a contact model or a physical-safety certification.
fn require_zero_reference_clearance(bundle: &RunBundle) -> Result<()> {
    let movement = bundle
        .metrics
        .movement_metrics
        .as_ref()
        .context("bundle has no local-motion telemetry")?;
    let boundary = movement
        .on_surface_clearance_audit
        .as_ref()
        .context("bundle has no integration-boundary reference-disc audit")?;
    let swept = movement
        .swept_on_surface_clearance_audit
        .as_ref()
        .context("bundle has no swept reference-disc audit")?;
    if boundary.disc_overlap_pair_steps != 0
        || boundary.maximum_disc_overlap_m != 0.0
        || swept.swept_disc_overlap_pair_steps != 0
        || swept.maximum_swept_disc_overlap_m != 0.0
    {
        bail!(
            "reference-clearance audits are nonzero: boundary {} pair-steps, {}m maximum; swept {} pair-steps, {}m maximum",
            boundary.disc_overlap_pair_steps,
            boundary.maximum_disc_overlap_m,
            swept.swept_disc_overlap_pair_steps,
            swept.maximum_swept_disc_overlap_m,
        );
    }
    Ok(())
}

fn build_queue_grid_coordination_artifact(
    source: String,
    group: String,
    queue_grid: String,
    policy: QueueGridCoordinationPolicy,
) -> Result<QueueGridCoordinationArtifact> {
    let outcome = coordinate_queue_grid(&source, &group, &queue_grid, &policy)?;
    Ok(QueueGridCoordinationArtifact {
        schema: QUEUE_GRID_COORDINATION_ARTIFACT_SCHEMA.to_owned(),
        source_sha256: sha256_hex(source.as_bytes()),
        source,
        group,
        queue_grid,
        policy,
        outcome,
    })
}

fn coordinate_queue_grid(
    source: &str,
    group_id: &str,
    queue_grid_id: &str,
    policy: &QueueGridCoordinationPolicy,
) -> Result<QueueGridCoordinationArtifactOutcome> {
    if policy.clearance_epsilon_m != reference_clearance_epsilon_m() {
        bail!(
            "queue-grid coordination artifacts must use the reference clearance epsilon {}m",
            reference_clearance_epsilon_m()
        );
    }
    let scenario = parse(source).map_err(|error| anyhow::anyhow!(error))?;
    validate(&scenario).map_err(|errors| validation_error(&errors))?;
    let group = scenario
        .agents
        .iter()
        .find(|group| group.id == group_id)
        .with_context(|| format!("unknown agent group `{group_id}`"))?;
    let footprint = scenario
        .queue_footprints
        .iter()
        .find(|footprint| footprint.id == queue_grid_id)
        .with_context(|| format!("unknown queue grid `{queue_grid_id}`"))?;
    let surface = scenario
        .surfaces
        .iter()
        .find(|surface| surface.id == footprint.surface)
        .with_context(|| {
            format!(
                "queue grid `{queue_grid_id}` references missing surface `{}`",
                footprint.surface
            )
        })?;
    let starts = group.spawn_positions().collect::<Vec<_>>();
    let slots = (0..footprint.slots)
        .map(|rank| footprint.position(rank))
        .collect::<Vec<_>>();
    let anchors = starts.iter().chain(&slots).copied().collect::<Vec<_>>();
    let lattice = CoordinationRoadmap::lattice(
        surface,
        &scenario.obstacles,
        group.radius_m,
        policy.roadmap_spacing_m,
        policy.maximum_roadmap_nodes,
        &anchors,
    )?;
    let tickets = starts
        .iter()
        .enumerate()
        .map(|(index, _)| {
            Ok(QueueGridTicketRequest {
                ticket: u64::try_from(index + 1).context("agent ordinal exceeds u64")?,
                agent_id: format!("{group_id}:{index}"),
                start_node: lattice.anchor_nodes[index],
                radius_m: group.radius_m,
                activation_at_s: group
                    .release_time_for(u32::try_from(index).context("agent ordinal exceeds u32")?),
                speed_mps: group.speed_mps,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let departures = estimate_queue_grid_departures(
        &tickets,
        QueueGridServiceAssumption {
            first_departure_at_s: policy.first_departure_at_s,
            headway_s: policy.headway_s,
        },
        scenario.duration_s,
    )?;
    let outcome = assess_queue_grid_rolling(&QueueGridRollingCoordinationRequest {
        queue: QueueGridCoordinationRequest {
            slot_nodes: &lattice.anchor_nodes[starts.len()..],
            tickets,
            departures,
            horizon_s: scenario.duration_s,
            occupied_trajectories: &[],
            timestep_s: policy.planning_timestep_s,
            maximum_low_level_expansions: policy.maximum_low_level_expansions,
            maximum_conflict_tree_nodes: policy.maximum_conflict_tree_nodes,
            clearance_epsilon_m: policy.clearance_epsilon_m,
            roadmap: &lattice.roadmap,
        },
        maximum_tickets_per_cohort: policy.maximum_tickets_per_cohort,
    })?;
    Ok(outcome.into())
}

fn verify_queue_grid_coordination_artifact(artifact: &QueueGridCoordinationArtifact) -> Result<()> {
    if artifact.schema != QUEUE_GRID_COORDINATION_ARTIFACT_SCHEMA {
        bail!(
            "unsupported queue-grid coordination artifact schema `{}`",
            artifact.schema
        );
    }
    let actual_source_sha256 = sha256_hex(artifact.source.as_bytes());
    if actual_source_sha256 != artifact.source_sha256 {
        bail!(
            "queue-grid coordination artifact source hash mismatch: expected {}, found {}",
            artifact.source_sha256,
            actual_source_sha256
        );
    }
    let reconstructed = coordinate_queue_grid(
        &artifact.source,
        &artifact.group,
        &artifact.queue_grid,
        &artifact.policy,
    )?;
    if reconstructed != artifact.outcome {
        bail!("queue-grid coordination artifact does not reproduce its recorded outcome");
    }
    if let QueueGridCoordinationArtifactOutcome::Planned { trajectories, .. } = &artifact.outcome {
        let trajectories = trajectories
            .iter()
            .cloned()
            .map(TimedDiscTrajectory::from)
            .collect::<Vec<_>>();
        let conflicts = timed_disc_conflicts(&trajectories, artifact.policy.clearance_epsilon_m)?;
        if !conflicts.is_empty() {
            bail!(
                "queue-grid coordination artifact contains {} exact timed-disc conflicts",
                conflicts.len()
            );
        }
    }
    Ok(())
}

fn print_queue_grid_coordination_outcome(outcome: &QueueGridCoordinationArtifactOutcome) {
    match outcome {
        QueueGridCoordinationArtifactOutcome::Planned {
            trajectories,
            explored_conflict_tree_nodes,
            low_level_explored_states,
            ..
        } => println!(
            "queue-grid coordination: planned {} trajectories; {} conflict-tree nodes; {} low-level states",
            trajectories.len(),
            explored_conflict_tree_nodes,
            low_level_explored_states
        ),
        QueueGridCoordinationArtifactOutcome::NoPlan { cohort_tickets } => println!(
            "queue-grid coordination: no plan in the declared rolling policy for cohort {:?}",
            cohort_tickets
        ),
        QueueGridCoordinationArtifactOutcome::Unresolved {
            cohort_tickets,
            reason,
        } => println!(
            "queue-grid coordination: unresolved cohort {:?}: {reason:?}",
            cohort_tickets
        ),
    }
}

impl From<QueueGridRollingOutcome> for QueueGridCoordinationArtifactOutcome {
    fn from(outcome: QueueGridRollingOutcome) -> Self {
        match outcome {
            QueueGridRollingOutcome::Planned(plan) => Self::from(plan),
            QueueGridRollingOutcome::NoPlan { cohort_tickets } => Self::NoPlan { cohort_tickets },
            QueueGridRollingOutcome::Unresolved {
                cohort_tickets,
                reason,
            } => Self::Unresolved {
                cohort_tickets,
                reason: QueueGridUnresolvedReasonArtifact::from(reason),
            },
        }
    }
}

impl From<QueueGridCoordinationPlan> for QueueGridCoordinationArtifactOutcome {
    fn from(plan: QueueGridCoordinationPlan) -> Self {
        Self::Planned {
            slot_windows: plan
                .slot_windows
                .into_iter()
                .map(|window| QueueGridSlotWindowArtifact {
                    ticket: window.ticket,
                    starts_at_s: window.starts_at_s,
                    ends_at_s: window.ends_at_s,
                    slot_rank: window.slot_rank,
                })
                .collect(),
            trajectories: plan
                .repair_plan
                .trajectories
                .into_iter()
                .map(TimedDiscTrajectoryArtifact::from)
                .collect(),
            explored_conflict_tree_nodes: plan.repair_plan.explored_conflict_tree_nodes,
            low_level_explored_states: plan.repair_plan.low_level_explored_states,
        }
    }
}

impl From<QueueGridUnresolvedReason> for QueueGridUnresolvedReasonArtifact {
    fn from(reason: QueueGridUnresolvedReason) -> Self {
        match reason {
            QueueGridUnresolvedReason::LowLevelSearchBoundExceeded {
                agent_id,
                target_index,
                maximum_expansions,
            } => Self::LowLevelSearchBoundExceeded {
                agent_id,
                target_index,
                maximum_expansions,
            },
            QueueGridUnresolvedReason::ConflictRepairBoundExceeded {
                maximum_conflict_tree_nodes,
            } => Self::ConflictRepairBoundExceeded {
                maximum_conflict_tree_nodes,
            },
        }
    }
}

impl From<TimedDiscTrajectory> for TimedDiscTrajectoryArtifact {
    fn from(trajectory: TimedDiscTrajectory) -> Self {
        Self {
            agent_id: trajectory.agent_id,
            segments: trajectory
                .segments
                .into_iter()
                .map(TimedDiscSegmentArtifact::from)
                .collect(),
        }
    }
}

impl From<TimedDiscTrajectoryArtifact> for TimedDiscTrajectory {
    fn from(trajectory: TimedDiscTrajectoryArtifact) -> Self {
        Self {
            agent_id: trajectory.agent_id,
            segments: trajectory
                .segments
                .into_iter()
                .map(TimedDiscSegment::from)
                .collect(),
        }
    }
}

impl From<TimedDiscSegment> for TimedDiscSegmentArtifact {
    fn from(segment: TimedDiscSegment) -> Self {
        Self {
            surface: segment.surface,
            starts_at_s: segment.starts_at_s,
            ends_at_s: segment.ends_at_s,
            start: [segment.start.x_m, segment.start.y_m, segment.start.z_m],
            end: [segment.end.x_m, segment.end.y_m, segment.end.z_m],
            radius_m: segment.radius_m,
        }
    }
}

impl From<TimedDiscSegmentArtifact> for TimedDiscSegment {
    fn from(segment: TimedDiscSegmentArtifact) -> Self {
        Self {
            surface: segment.surface,
            starts_at_s: segment.starts_at_s,
            ends_at_s: segment.ends_at_s,
            start: Point3 {
                x_m: segment.start[0],
                y_m: segment.start[1],
                z_m: segment.start[2],
            },
            end: Point3 {
                x_m: segment.end[0],
                y_m: segment.end[1],
                z_m: segment.end[2],
            },
            radius_m: segment.radius_m,
        }
    }
}

fn print_local_clearance_summary(metrics: &RunMetrics) {
    let Some(movement) = &metrics.movement_metrics else {
        println!("local-motion telemetry: unavailable in this legacy bundle");
        return;
    };
    println!(
        "local-motion adjustments: {} agents, {} attempts, {}m cumulative, {}m maximum",
        movement.agents_with_local_clearance_adjustments,
        movement.local_clearance_adjustment_steps,
        movement.cumulative_local_clearance_adjustment_m,
        movement.maximum_local_clearance_adjustment_m,
    );
    match movement.local_avoidance_constraint_fallback_steps {
        Some(steps) => println!("local-motion ORCA constraint fallbacks: {steps} steps"),
        None => {
            println!("local-motion ORCA constraint fallback telemetry: unavailable before 0.31");
        }
    }
    match &movement.on_surface_clearance_audit {
        Some(audit) => println!(
            "on-surface reference-disc overlaps: {} agents, {} pair-steps, {}m maximum",
            audit.agents_with_disc_overlaps,
            audit.disc_overlap_pair_steps,
            audit.maximum_disc_overlap_m,
        ),
        None => println!("on-surface reference-disc audit: unavailable before 0.36"),
    }
    match &movement.swept_on_surface_clearance_audit {
        Some(audit) => println!(
            "swept on-surface reference-disc overlaps: {} agents, {} pair-steps, {}m maximum",
            audit.agents_with_swept_disc_overlaps,
            audit.swept_disc_overlap_pair_steps,
            audit.maximum_swept_disc_overlap_m,
        ),
        None => println!("swept on-surface reference-disc audit: unavailable before 0.37"),
    }
}

fn verify_benchmark(manifest: &Path) -> Result<()> {
    let manifest: BenchmarkManifest = read_json(manifest)?;
    validate_manifest(&manifest).map_err(|errors| benchmark_error(&errors))?;
    println!("valid empirical benchmark round: {}", manifest.round_id);
    Ok(())
}

fn handle_evidence(command: EvidenceCommand) -> Result<()> {
    match command {
        EvidenceCommand::Verify { catalog } => {
            let catalog: EvidenceCatalog = read_json(&catalog)?;
            validate_catalog(&catalog).map_err(|errors| evidence_error(&errors))?;
            println!("valid evidence catalog: {}", catalog.dataset_id);
        }
        EvidenceCommand::Lock { catalog, data_root } => {
            let catalog: EvidenceCatalog = read_json(&catalog)?;
            verify_catalog_files(&catalog, &data_root)?;
            println!("content-locked: {}", catalog.dataset_id);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // one exhaustive layout-command dispatch keeps CLI routing inspectable
fn handle_layout(command: LayoutCommand) -> Result<()> {
    match command {
        LayoutCommand::Osm {
            catalog,
            data_root,
            max_nodes,
            max_ways,
            output,
        } => {
            let catalog: EvidenceCatalog = read_json(&catalog)?;
            let limits = OsmInspectionLimits {
                max_nodes,
                max_ways,
                ..OsmInspectionLimits::default()
            };
            let report = inspect_openstreetmap_layout(&catalog, &data_root, limits)?;
            write_json(&output, &report)?;
            println!("layout observation report: {}", output.display());
            println!("status: {}", report.status);
        }
        LayoutCommand::VerifyOsm {
            catalog,
            report,
            data_root,
        } => {
            let catalog: EvidenceCatalog = read_json(&catalog)?;
            let report: OpenStreetMapLayoutReport = read_json(&report)?;
            verify_openstreetmap_layout_report(&catalog, &data_root, &report)?;
            println!(
                "verified layout observation report: {}",
                report.source.dataset_id
            );
        }
        LayoutCommand::ProjectOsm {
            catalog,
            report,
            data_root,
            origin_latitude,
            origin_longitude,
            output,
        } => {
            let catalog: EvidenceCatalog = read_json(&catalog)?;
            let report: OpenStreetMapLayoutReport = read_json(&report)?;
            verify_openstreetmap_layout_report(&catalog, &data_root, &report)?;
            let projection = project_openstreetmap_layout_report(
                &report,
                GeographicPoint {
                    latitude: origin_latitude,
                    longitude: origin_longitude,
                },
            )?;
            write_json(&output, &projection)?;
            println!("local-coordinate reference report: {}", output.display());
            println!("status: {}", projection.status);
        }
        LayoutCommand::VerifyProjection {
            catalog,
            report,
            projection,
            data_root,
        } => {
            let catalog: EvidenceCatalog = read_json(&catalog)?;
            let report: OpenStreetMapLayoutReport = read_json(&report)?;
            let projection: OpenStreetMapLocalProjectionReport = read_json(&projection)?;
            verify_openstreetmap_layout_report(&catalog, &data_root, &report)?;
            verify_openstreetmap_local_projection_report(&report, &projection)?;
            println!(
                "verified local-coordinate reference report: {}",
                projection.source.dataset_id
            );
        }
        LayoutCommand::AnchorOsm {
            catalog,
            observation,
            projection,
            manifest,
            data_root,
            output,
        } => anchor_osm_scenario_command(
            &catalog,
            &observation,
            &projection,
            &manifest,
            &data_root,
            &output,
        )?,
        LayoutCommand::VerifyAnchorOsm {
            catalog,
            observation,
            projection,
            manifest,
            anchor_report,
            data_root,
        } => verify_osm_scenario_anchor_command(
            &catalog,
            &observation,
            &projection,
            &manifest,
            &anchor_report,
            &data_root,
        )?,
    }
    Ok(())
}

fn anchor_osm_scenario_command(
    catalog_path: &Path,
    observation_path: &Path,
    projection_path: &Path,
    manifest_path: &Path,
    data_root: &Path,
    output: &Path,
) -> Result<()> {
    let catalog: EvidenceCatalog = read_json(catalog_path)?;
    let observation: OpenStreetMapLayoutReport = read_json(observation_path)?;
    let projection_bytes = fs::read(projection_path)
        .with_context(|| format!("reading {}", projection_path.display()))?;
    let projection: OpenStreetMapLocalProjectionReport = serde_json::from_slice(&projection_bytes)
        .with_context(|| format!("parsing {}", projection_path.display()))?;
    verify_openstreetmap_layout_report(&catalog, data_root, &observation)?;
    verify_openstreetmap_local_projection_report(&observation, &projection)?;
    let (manifest, manifest_bytes, scenario_source, scenario) =
        load_osm_scenario_anchor(manifest_path)?;
    let anchored = anchor_osm_scenario(
        &manifest,
        &sha256_hex(&manifest_bytes),
        &scenario,
        &sha256_hex(scenario_source.as_bytes()),
        &projection,
        &sha256_hex(&projection_bytes),
    )?;
    write_json(output, &anchored)?;
    println!("OSM scenario-anchor report: {}", output.display());
    println!("status: {}", anchored.status);
    Ok(())
}

fn verify_osm_scenario_anchor_command(
    catalog_path: &Path,
    observation_path: &Path,
    projection_path: &Path,
    manifest_path: &Path,
    anchor_report_path: &Path,
    data_root: &Path,
) -> Result<()> {
    let catalog: EvidenceCatalog = read_json(catalog_path)?;
    let observation: OpenStreetMapLayoutReport = read_json(observation_path)?;
    let projection_bytes = fs::read(projection_path)
        .with_context(|| format!("reading {}", projection_path.display()))?;
    let projection: OpenStreetMapLocalProjectionReport = serde_json::from_slice(&projection_bytes)
        .with_context(|| format!("parsing {}", projection_path.display()))?;
    verify_openstreetmap_layout_report(&catalog, data_root, &observation)?;
    verify_openstreetmap_local_projection_report(&observation, &projection)?;
    let (manifest, manifest_bytes, scenario_source, scenario) =
        load_osm_scenario_anchor(manifest_path)?;
    let anchored: OsmScenarioAnchorReport = read_json(anchor_report_path)?;
    verify_osm_scenario_anchor_report(
        &manifest,
        &sha256_hex(&manifest_bytes),
        &scenario,
        &sha256_hex(scenario_source.as_bytes()),
        &projection,
        &sha256_hex(&projection_bytes),
        &anchored,
    )?;
    println!(
        "verified OSM scenario-anchor report: {}",
        anchored.source.dataset_id
    );
    Ok(())
}

fn handle_experiment(command: ExperimentCommand) -> Result<()> {
    match command {
        ExperimentCommand::Init {
            name,
            seed,
            output,
            trace_every,
            with_sensitivity,
            sensitivity_runs,
        } => initialize_experiment(
            &name,
            seed,
            &output,
            trace_every,
            with_sensitivity,
            sensitivity_runs,
        )?,
        ExperimentCommand::Plan { manifest, output } => {
            plan_experiment(&manifest, output.as_deref())?;
        }
        ExperimentCommand::Run { manifest, output } => run_experiment(&manifest, &output)?,
        ExperimentCommand::Verify { directory } => verify_experiment(&directory)?,
    }
    Ok(())
}

fn initialize_experiment(
    name: &str,
    seed: u64,
    output: &Path,
    trace_every_steps: u32,
    with_sensitivity: bool,
    sensitivity_runs: Option<u32>,
) -> Result<()> {
    let scenario_source = generator::source(seed);
    let scenario = generator::scenario(seed)?;
    let manifest =
        starter_experiment_manifest(name, seed, trace_every_steps, &scenario, with_sensitivity)?;
    validate_experiment_manifest(&manifest).map_err(|errors| experiment_error(&errors))?;
    let sensitivity_manifest = with_sensitivity
        .then(|| {
            starter_sensitivity_manifest(
                name,
                seed,
                trace_every_steps,
                sensitivity_runs.unwrap_or(8),
                &scenario,
            )
        })
        .transpose()?;
    ensure_empty_directory(output)?;
    fs::write(output.join("scenario.chy"), scenario_source)
        .with_context(|| format!("writing starter scenario into {}", output.display()))?;
    write_json(&output.join("experiment.json"), &manifest)?;
    if let Some(sensitivity_manifest) = sensitivity_manifest {
        write_json(&output.join("sensitivity.json"), &sensitivity_manifest)?;
    }
    println!(
        "uncalibrated experiment starter: {} ({})",
        output.join("experiment.json").display(),
        scenario.name
    );
    println!(
        "next: chiyoda experiment plan {}",
        output.join("experiment.json").display()
    );
    if with_sensitivity {
        let sensitivity_manifest_path = output.join("sensitivity.json");
        let sensitivity_output = output.join("sensitivity-study");
        println!(
            "review sensitivity: chiyoda sensitivity-plan {}",
            sensitivity_manifest_path.display()
        );
        println!(
            "after review: chiyoda sensitivity {} -o {}",
            sensitivity_manifest_path.display(),
            sensitivity_output.display()
        );
        println!(
            "then verify: chiyoda verify-sensitivity {}",
            sensitivity_output.display()
        );
    }
    Ok(())
}

fn starter_experiment_manifest(
    name: &str,
    seed: u64,
    trace_every_steps: u32,
    scenario: &chiyoda_core::Scenario,
    with_sensitivity: bool,
) -> Result<ExperimentManifest> {
    let passengers = scenario
        .agents
        .first()
        .context("generated starter scenario has no agent group")?;
    let gate = scenario
        .gates
        .first()
        .context("generated starter scenario has no gate")?;
    let misinformation = scenario
        .messages
        .first()
        .context("generated starter scenario has no message")?;
    let correction = scenario
        .countermeasures
        .first()
        .context("generated starter scenario has no countermeasure")?;
    Ok(ExperimentManifest {
        schema_version: "0.4".to_owned(),
        name: name.to_owned(),
        description: format!(
            "an initial uncalibrated structural draft generated from deterministic seed {seed}; review every stated input before interpreting a run"
        ),
        scenario_source: "scenario.chy".to_owned(),
        trace_every_steps,
        assumptions: starter_experiment_assumptions(seed, passengers, gate, misinformation, correction),
        sources: Vec::new(),
        source_attestations: Vec::new(),
        sensitivity_studies: if with_sensitivity {
            vec![ExperimentSensitivityStudy {
                id: "generated_best_guess_stress_test".to_owned(),
                manifest_path: "sensitivity.json".to_owned(),
            }]
        } else {
            Vec::new()
        },
        claim_boundary: "This is an uncalibrated deterministic structural draft. It does not predict a real facility, population, evacuation outcome, operational response, or safety result.".to_owned(),
    })
}

fn current_experiment_report_schema(manifest: &ExperimentManifest) -> &'static str {
    if manifest.schema_version == "0.4" {
        EXPERIMENT_REPORT_SCHEMA_VERSION
    } else {
        "0.3"
    }
}

fn starter_experiment_assumptions(
    seed: u64,
    passengers: &chiyoda_core::model::AgentGroup,
    gate: &chiyoda_core::model::Gate,
    misinformation: &chiyoda_core::model::Message,
    correction: &chiyoda_core::model::Countermeasure,
) -> Vec<chiyoda_core::ExperimentAssumption> {
    vec![
        chiyoda_core::ExperimentAssumption {
            id: "generated_topology".to_owned(),
            subject: "scenario seed, topology, routing alternatives, and scheduled changes"
                .to_owned(),
            basis: chiyoda_core::AssumptionBasis::StructuralAssumption,
            rationale: format!(
                "the deterministic generator produced this structural draft from seed {seed}; it is not a representation of a real facility"
            ),
            source_ids: Vec::new(),
            targets: Vec::new(),
        },
        passenger_demand_and_motion_assumption(passengers),
        service_and_information_conditions_assumption(gate, misinformation, correction),
    ]
}

fn passenger_demand_and_motion_assumption(
    passengers: &chiyoda_core::model::AgentGroup,
) -> chiyoda_core::ExperimentAssumption {
    chiyoda_core::ExperimentAssumption {
        id: "passenger_demand_and_motion".to_owned(),
        subject: "passengers count, release schedule, speed, radius, and height".to_owned(),
        basis: chiyoda_core::AssumptionBasis::BestGuess,
        rationale: "the generated demand and body/motion values are explicit starting inputs, not a population estimate".to_owned(),
        source_ids: Vec::new(),
        targets: vec![
            experiment_assumption_target(SensitivityTarget::AgentCount, &passengers.id),
            experiment_assumption_target(SensitivityTarget::AgentReleaseAtS, &passengers.id),
            experiment_assumption_target(SensitivityTarget::AgentSpeedMps, &passengers.id),
            experiment_assumption_target(SensitivityTarget::AgentRadiusM, &passengers.id),
            experiment_assumption_target(SensitivityTarget::AgentHeightM, &passengers.id),
        ],
    }
}

fn service_and_information_conditions_assumption(
    gate: &chiyoda_core::model::Gate,
    misinformation: &chiyoda_core::model::Message,
    correction: &chiyoda_core::model::Countermeasure,
) -> chiyoda_core::ExperimentAssumption {
    chiyoda_core::ExperimentAssumption {
        id: "service_and_information_conditions".to_owned(),
        subject: "gate service capacity, closure schedule, message reach, and trust".to_owned(),
        basis: chiyoda_core::AssumptionBasis::StructuralAssumption,
        rationale: "the generated constraints and interventions are stress conditions for reference-runtime exploration, not observed operations or behavior".to_owned(),
        source_ids: Vec::new(),
        targets: vec![
            experiment_assumption_target(SensitivityTarget::GateServiceRatePerS, &gate.id),
            experiment_assumption_target(SensitivityTarget::MessageAtS, &misinformation.id),
            experiment_assumption_target(SensitivityTarget::MessageReachM, &misinformation.id),
            experiment_assumption_target(SensitivityTarget::MessageTrust, &misinformation.id),
            experiment_assumption_target(SensitivityTarget::CountermeasureAtS, &correction.id),
            experiment_assumption_target(SensitivityTarget::CountermeasureReachM, &correction.id),
            experiment_assumption_target(SensitivityTarget::CountermeasureTrust, &correction.id),
        ],
    }
}

fn experiment_assumption_target(
    target: SensitivityTarget,
    subject: &str,
) -> ExperimentAssumptionTarget {
    ExperimentAssumptionTarget {
        target,
        subject: subject.to_owned(),
    }
}

fn starter_sensitivity_manifest(
    name: &str,
    seed: u64,
    trace_every_steps: u32,
    run_count: u32,
    scenario: &chiyoda_core::Scenario,
) -> Result<SensitivityManifest> {
    let passengers = scenario
        .agents
        .first()
        .context("generated starter scenario has no agent group")?;
    let gate = scenario
        .gates
        .first()
        .context("generated starter scenario has no gate")?;
    let misinformation = scenario
        .messages
        .first()
        .context("generated starter scenario has no message")?;
    let correction = scenario
        .countermeasures
        .first()
        .context("generated starter scenario has no countermeasure")?;
    let manifest = SensitivityManifest {
        schema_version: "0.1".to_owned(),
        name: format!("{name} sensitivity alternatives"),
        description: "a no-data, one-at-a-time exploration of six explicitly generated best-guess inputs".to_owned(),
        baseline_source: "scenario.chy".to_owned(),
        first_seed: seed,
        count: run_count,
        trace_every_steps,
        design: chiyoda_core::SensitivityDesign::OneAtATime,
        max_conditions: 12,
        factors: vec![
            SensitivityFactor {
                id: "passenger_demand".to_owned(),
                target: chiyoda_core::SensitivityTarget::AgentCount,
                subject: passengers.id.clone(),
                values: vec![
                    f64::from(passengers.count.saturating_mul(3) / 4),
                    f64::from(passengers.count),
                    f64::from(passengers.count.saturating_mul(5).div_ceil(4)),
                ],
                basis: chiyoda_core::AssumptionBasis::BestGuess,
                rationale: "these alternatives bracket the generated passenger count without treating it as a demand estimate or population profile".to_owned(),
                references: Vec::new(),
            },
            SensitivityFactor {
                id: "passenger_speed".to_owned(),
                target: chiyoda_core::SensitivityTarget::AgentSpeedMps,
                subject: passengers.id.clone(),
                values: vec![
                    passengers.speed_mps * 0.8,
                    passengers.speed_mps,
                    passengers.speed_mps * 1.2,
                ],
                basis: chiyoda_core::AssumptionBasis::BestGuess,
                rationale: "these alternatives bracket the generated walking-speed input without claiming a population distribution or measured travel speed".to_owned(),
                references: Vec::new(),
            },
            SensitivityFactor {
                id: "gate_service_rate".to_owned(),
                target: chiyoda_core::SensitivityTarget::GateServiceRatePerS,
                subject: gate.id.clone(),
                values: vec![
                    gate.service_rate_per_s * 0.5,
                    gate.service_rate_per_s,
                    gate.service_rate_per_s * 1.5,
                ],
                basis: chiyoda_core::AssumptionBasis::BestGuess,
                rationale: "these alternatives expose the structural consequence of the generated gate service limit without treating it as observed throughput".to_owned(),
                references: Vec::new(),
            },
            SensitivityFactor {
                id: "misinformation_trust".to_owned(),
                target: chiyoda_core::SensitivityTarget::MessageTrust,
                subject: misinformation.id.clone(),
                values: vec![
                    misinformation.trust * 0.5,
                    misinformation.trust,
                    misinformation.trust.midpoint(1.0),
                ],
                basis: chiyoda_core::AssumptionBasis::BestGuess,
                rationale: "these alternatives expose the effect of the generated misinformation acceptance input without estimating human trust or message uptake".to_owned(),
                references: Vec::new(),
            },
            starter_correction_trust_factor(&correction.id, correction.trust),
            starter_correction_timing_factor(
                &correction.id,
                misinformation.at_s,
                correction.at_s,
                scenario.duration_s,
            ),
        ],
        claim_boundary: "This is an uncalibrated sensitivity exploration of generated best guesses. It does not estimate a real population, facility, service rate, evacuation outcome, operational response, or safety result.".to_owned(),
    };
    plan_sensitivity(&manifest, scenario)
        .context("generated starter sensitivity manifest must be valid")?;
    Ok(manifest)
}

fn starter_correction_trust_factor(correction_id: &str, trust: f64) -> SensitivityFactor {
    SensitivityFactor {
        id: "correction_trust".to_owned(),
        target: chiyoda_core::SensitivityTarget::CountermeasureTrust,
        subject: correction_id.to_owned(),
        values: vec![trust * 0.5, trust, trust.midpoint(1.0)],
        basis: chiyoda_core::AssumptionBasis::BestGuess,
        rationale: "these alternatives expose the effect of the generated corrective-message acceptance input without estimating staff effectiveness or human trust".to_owned(),
        references: Vec::new(),
    }
}

fn starter_correction_timing_factor(
    correction_id: &str,
    misinformation_at_s: f64,
    correction_at_s: f64,
    duration_s: f64,
) -> SensitivityFactor {
    SensitivityFactor {
        id: "correction_time".to_owned(),
        target: chiyoda_core::SensitivityTarget::CountermeasureAtS,
        subject: correction_id.to_owned(),
        values: vec![
            misinformation_at_s.midpoint(correction_at_s),
            correction_at_s,
            correction_at_s.midpoint(duration_s),
        ],
        basis: chiyoda_core::AssumptionBasis::BestGuess,
        rationale: "these alternatives expose the structural consequence of an earlier or later corrective message while preserving the generated false-message ordering; they do not estimate detection, staffing, or communication-response time".to_owned(),
        references: Vec::new(),
    }
}

fn resolve_experiment_assumption_targets(
    manifest: &ExperimentManifest,
    scenario: &chiyoda_core::Scenario,
) -> Result<Vec<ExperimentResolvedAssumptionTarget>> {
    manifest
        .assumptions
        .iter()
        .flat_map(|assumption| {
            assumption
                .targets
                .iter()
                .map(move |target| (assumption.id.as_str(), target))
        })
        .map(|(assumption_id, target)| {
            let baseline_value =
                resolve_sensitivity_target_value(scenario, target.target, &target.subject)
                    .with_context(|| {
                        format!(
                            "resolving assumption `{assumption_id}` target `{:?}` for subject `{}`",
                            target.target, target.subject
                        )
                    })?;
            Ok(ExperimentResolvedAssumptionTarget {
                assumption_id: assumption_id.to_owned(),
                target: target.target,
                subject: target.subject.clone(),
                baseline_value,
                unit: target.target.unit().to_owned(),
            })
        })
        .collect()
}

fn capture_experiment_sensitivity_coverage(
    manifest: &ExperimentManifest,
    manifest_path: &Path,
    scenario: &chiyoda_core::Scenario,
    resolved_assumption_targets: &[ExperimentResolvedAssumptionTarget],
) -> Result<(
    Option<ExperimentSensitivityCoverage>,
    Vec<CapturedExperimentSensitivityStudy>,
)> {
    if manifest.sensitivity_studies.is_empty() {
        return Ok((None, Vec::new()));
    }
    let scenario_hash =
        chiyoda_core::bundle::canonical_hash(&CanonicalScenario::from(scenario.clone()));
    let manifest_directory = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut captured = Vec::with_capacity(manifest.sensitivity_studies.len());
    for study_link in &manifest.sensitivity_studies {
        let path = manifest_directory.join(&study_link.manifest_path);
        let manifest_bytes = fs::read(&path)
            .with_context(|| format!("reading sensitivity-study manifest {}", path.display()))?;
        let sensitivity_manifest: SensitivityManifest = serde_json::from_slice(&manifest_bytes)
            .with_context(|| format!("parsing sensitivity-study manifest {}", path.display()))?;
        let baseline_path = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&sensitivity_manifest.baseline_source);
        let baseline_source = read_text(&baseline_path).with_context(|| {
            format!(
                "reading baseline scenario for sensitivity study `{}`",
                study_link.id
            )
        })?;
        let baseline = parse(&baseline_source).map_err(|error| anyhow::anyhow!(error))?;
        validate(&baseline).map_err(|errors| validation_error(&errors))?;
        let baseline_hash =
            chiyoda_core::bundle::canonical_hash(&CanonicalScenario::from(baseline.clone()));
        if baseline_hash != scenario_hash {
            bail!(
                "sensitivity study `{}` baseline scenario does not match the experiment scenario",
                study_link.id
            );
        }
        let coverage = resolve_experiment_sensitivity_study_coverage(
            study_link,
            &manifest_bytes,
            baseline_source.as_bytes(),
            &sensitivity_manifest,
            scenario,
            &scenario_hash,
            resolved_assumption_targets,
        )?;
        captured.push(CapturedExperimentSensitivityStudy {
            coverage,
            manifest_bytes,
            baseline_source_bytes: baseline_source.into_bytes(),
        });
    }
    let coverage = build_experiment_sensitivity_coverage(
        captured
            .iter()
            .map(|study| study.coverage.clone())
            .collect(),
        resolved_assumption_targets,
    );
    Ok((Some(coverage), captured))
}

fn resolve_experiment_sensitivity_study_coverage(
    study_link: &ExperimentSensitivityStudy,
    manifest_bytes: &[u8],
    baseline_source_bytes: &[u8],
    sensitivity_manifest: &SensitivityManifest,
    scenario: &chiyoda_core::Scenario,
    scenario_hash: &str,
    resolved_assumption_targets: &[ExperimentResolvedAssumptionTarget],
) -> Result<ExperimentSensitivityStudyCoverage> {
    let study = plan_sensitivity(sensitivity_manifest, scenario)
        .map_err(|error| anyhow::anyhow!(error))
        .with_context(|| format!("planning sensitivity study `{}`", study_link.id))?;
    let mut factors = Vec::with_capacity(sensitivity_manifest.factors.len());
    for factor in &sensitivity_manifest.factors {
        let resolved = resolved_assumption_targets
            .iter()
            .find(|assumption| {
                assumption.target == factor.target && assumption.subject == factor.subject
            })
            .with_context(|| {
                format!(
                    "sensitivity study `{}` factor `{}` targets an input that is not declared by an experiment assumption",
                    study_link.id, factor.id
                )
            })?;
        let baseline_value = *study
            .baseline_values
            .get(&factor.id)
            .context("validated sensitivity study has no factor baseline")?;
        if baseline_value.total_cmp(&resolved.baseline_value).is_ne() {
            bail!(
                "sensitivity study `{}` factor `{}` baseline disagrees with its experiment assumption target",
                study_link.id,
                factor.id
            );
        }
        factors.push(ExperimentSensitivityFactorCoverage {
            factor_id: factor.id.clone(),
            target: factor.target,
            subject: factor.subject.clone(),
            baseline_value,
            unit: factor.target.unit().to_owned(),
            values: factor.values.clone(),
        });
    }
    Ok(ExperimentSensitivityStudyCoverage {
        study_id: study_link.id.clone(),
        declared_manifest_path: study_link.manifest_path.clone(),
        manifest_snapshot: format!("sensitivity-studies/{}/manifest.json", study_link.id),
        manifest_sha256: sha256_hex(manifest_bytes),
        baseline_source_snapshot: format!("sensitivity-studies/{}/baseline.chy", study_link.id),
        baseline_source_sha256: sha256_hex(baseline_source_bytes),
        baseline_scenario_hash: scenario_hash.to_owned(),
        design: sensitivity_manifest.design,
        condition_count: study.conditions.len(),
        factors,
    })
}

fn build_experiment_sensitivity_coverage(
    studies: Vec<ExperimentSensitivityStudyCoverage>,
    resolved_assumption_targets: &[ExperimentResolvedAssumptionTarget],
) -> ExperimentSensitivityCoverage {
    let mut factors_by_target =
        BTreeMap::<(SensitivityTarget, String), Vec<ExperimentSensitivityFactorLink>>::new();
    for study in &studies {
        for factor in &study.factors {
            factors_by_target
                .entry((factor.target, factor.subject.clone()))
                .or_default()
                .push(ExperimentSensitivityFactorLink {
                    study_id: study.study_id.clone(),
                    factor_id: factor.factor_id.clone(),
                });
        }
    }
    let assumption_targets = resolved_assumption_targets
        .iter()
        .map(|assumption| ExperimentAssumptionSensitivityCoverage {
            assumption_id: assumption.assumption_id.clone(),
            target: assumption.target,
            subject: assumption.subject.clone(),
            baseline_value: assumption.baseline_value,
            unit: assumption.unit.clone(),
            sensitivity_factors: factors_by_target
                .remove(&(assumption.target, assumption.subject.clone()))
                .unwrap_or_default(),
        })
        .collect();
    ExperimentSensitivityCoverage {
        studies,
        assumption_targets,
    }
}

fn write_experiment_sensitivity_studies(
    output: &Path,
    studies: &[CapturedExperimentSensitivityStudy],
) -> Result<()> {
    for study in studies {
        let path = output.join(&study.coverage.manifest_snapshot);
        let parent = path
            .parent()
            .context("sensitivity-study manifest snapshot has no parent")?;
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        fs::write(&path, &study.manifest_bytes).with_context(|| {
            format!(
                "writing sensitivity-study manifest snapshot {}",
                path.display()
            )
        })?;
        let baseline_path = output.join(&study.coverage.baseline_source_snapshot);
        fs::write(&baseline_path, &study.baseline_source_bytes).with_context(|| {
            format!(
                "writing sensitivity-study baseline snapshot {}",
                baseline_path.display()
            )
        })?;
    }
    Ok(())
}

fn reconstruct_experiment_sensitivity_coverage(
    directory: &Path,
    manifest: &ExperimentManifest,
    scenario: &chiyoda_core::Scenario,
    resolved_assumption_targets: &[ExperimentResolvedAssumptionTarget],
) -> Result<Option<ExperimentSensitivityCoverage>> {
    if manifest.sensitivity_studies.is_empty() {
        let root = directory.join("sensitivity-studies");
        if root.exists() {
            bail!("experiment has undeclared sensitivity-study snapshots");
        }
        return Ok(None);
    }
    verify_experiment_sensitivity_study_layout(directory, manifest)?;
    let scenario_hash =
        chiyoda_core::bundle::canonical_hash(&CanonicalScenario::from(scenario.clone()));
    let mut studies = Vec::with_capacity(manifest.sensitivity_studies.len());
    for study_link in &manifest.sensitivity_studies {
        let path = directory
            .join("sensitivity-studies")
            .join(&study_link.id)
            .join("manifest.json");
        let manifest_bytes = fs::read(&path).with_context(|| {
            format!(
                "reading sensitivity-study manifest snapshot {}",
                path.display()
            )
        })?;
        let sensitivity_manifest: SensitivityManifest = serde_json::from_slice(&manifest_bytes)
            .with_context(|| {
                format!(
                    "parsing sensitivity-study manifest snapshot {}",
                    path.display()
                )
            })?;
        let baseline_path = directory
            .join("sensitivity-studies")
            .join(&study_link.id)
            .join("baseline.chy");
        let baseline_source = read_text(&baseline_path).with_context(|| {
            format!(
                "reading sensitivity-study baseline snapshot {}",
                baseline_path.display()
            )
        })?;
        let baseline = parse(&baseline_source).map_err(|error| anyhow::anyhow!(error))?;
        validate(&baseline).map_err(|errors| validation_error(&errors))?;
        let baseline_hash =
            chiyoda_core::bundle::canonical_hash(&CanonicalScenario::from(baseline));
        if baseline_hash != scenario_hash {
            bail!(
                "sensitivity-study baseline snapshot for `{}` does not match the experiment scenario",
                study_link.id
            );
        }
        studies.push(resolve_experiment_sensitivity_study_coverage(
            study_link,
            &manifest_bytes,
            baseline_source.as_bytes(),
            &sensitivity_manifest,
            scenario,
            &scenario_hash,
            resolved_assumption_targets,
        )?);
    }
    Ok(Some(build_experiment_sensitivity_coverage(
        studies,
        resolved_assumption_targets,
    )))
}

fn verify_experiment_sensitivity_study_layout(
    directory: &Path,
    manifest: &ExperimentManifest,
) -> Result<()> {
    let root = directory.join("sensitivity-studies");
    let expected_ids = manifest
        .sensitivity_studies
        .iter()
        .map(|study| study.id.clone())
        .collect::<BTreeSet<_>>();
    let actual_ids = fs::read_dir(&root)
        .with_context(|| format!("reading {}", root.display()))?
        .map(|entry| {
            let entry = entry.with_context(|| format!("reading {}", root.display()))?;
            if !entry
                .file_type()
                .with_context(|| format!("reading {}", entry.path().display()))?
                .is_dir()
            {
                bail!(
                    "sensitivity-study snapshot is not a directory: {}",
                    entry.path().display()
                );
            }
            entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("sensitivity-study identifier is not UTF-8"))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    if actual_ids != expected_ids {
        bail!("sensitivity-study snapshot directories do not match manifest declarations");
    }
    for id in expected_ids {
        let study_directory = root.join(&id);
        let files = fs::read_dir(&study_directory)
            .with_context(|| format!("reading {}", study_directory.display()))?
            .map(|entry| {
                let entry =
                    entry.with_context(|| format!("reading {}", study_directory.display()))?;
                if !entry
                    .file_type()
                    .with_context(|| format!("reading {}", entry.path().display()))?
                    .is_file()
                {
                    bail!(
                        "sensitivity-study snapshot entry is not a file: {}",
                        entry.path().display()
                    );
                }
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("sensitivity-study snapshot name is not UTF-8"))
            })
            .collect::<Result<BTreeSet<_>>>()?;
        if files != BTreeSet::from(["baseline.chy".to_owned(), "manifest.json".to_owned()]) {
            bail!("sensitivity-study snapshot files do not match the artifact contract");
        }
    }
    Ok(())
}

fn plan_experiment(manifest_path: &Path, output: Option<&Path>) -> Result<()> {
    let (manifest, scenario_source, scenario) = load_experiment(manifest_path)?;
    let resolved_assumption_targets = resolve_experiment_assumption_targets(&manifest, &scenario)?;
    let (sensitivity_coverage, _) = capture_experiment_sensitivity_coverage(
        &manifest,
        manifest_path,
        &scenario,
        &resolved_assumption_targets,
    )?;
    let source_reports = capture_experiment_source_reports(&manifest, manifest_path)?;
    let source_attestations = capture_experiment_osm_attestations(
        &manifest,
        manifest_path,
        &source_reports,
        &scenario_source,
        &scenario,
    )?;
    let integration_steps =
        chiyoda_core::integration_step_count(scenario.duration_s, scenario.timestep_s);
    let stored_trace_frames =
        stored_trace_frame_count(integration_steps, manifest.trace_every_steps)
            .context("experiment plan trace-frame count overflowed")?;
    let report = ExperimentPlanReport {
        schema_version: EXPERIMENT_PLAN_SCHEMA_VERSION.to_owned(),
        experiment_name: manifest.name.clone(),
        description: manifest.description.clone(),
        manifest: manifest_path.display().to_string(),
        scenario_source: manifest.scenario_source.clone(),
        scenario_hash: chiyoda_core::bundle::canonical_hash(&CanonicalScenario::from(scenario.clone())),
        trace_every_steps: manifest.trace_every_steps,
        execution: ExperimentPlanExecution {
            duration_s: scenario.duration_s,
            timestep_s: scenario.timestep_s,
            integration_steps,
            stored_trace_frames,
        },
        scenario: ExperimentScenarioInventory {
            surfaces: scenario.surfaces.len(),
            obstacles: scenario.obstacles.len(),
            waypoints: scenario.waypoints.len(),
            exits: scenario.exits.len(),
            connectors: scenario.connectors.len(),
            gates: scenario.gates.len(),
            agent_groups: scenario.agents.len(),
            declared_agents: scenario.agents.iter().map(|group| u64::from(group.count)).sum(),
            connector_state_changes: scenario.connector_states.len(),
            exit_state_changes: scenario.exit_states.len(),
            connector_capacity_changes: scenario.connector_capacity_states.len(),
            exit_capacity_changes: scenario.exit_capacity_states.len(),
            gate_state_changes: scenario.gate_states.len(),
            gate_capacity_changes: scenario.gate_capacity_states.len(),
            messages: scenario.messages.len(),
            countermeasures: scenario.countermeasures.len(),
        },
        assumptions: manifest.assumptions.clone(),
        resolved_assumption_targets,
        sensitivity_coverage,
        sources: manifest.sources.clone(),
        source_report_snapshots: source_reports
            .iter()
            .map(|report| report.snapshot.clone())
            .collect(),
        source_attestations: manifest.source_attestations.clone(),
        verified_osm_source_attestations: source_attestations
            .into_iter()
            .map(|attestation| attestation.source_id().to_owned())
            .collect(),
        author_claim_boundary: manifest.claim_boundary,
        claim_boundary: "This plan validates one authored, uncalibrated scenario and checks its declared source reports and optional OSM source attestations at planning time. It does not execute the runtime, produce outcomes, estimate likelihoods, validate a facility, or support predictive, operational, or safety claims.".to_owned(),
    };
    if let Some(output) = output {
        write_json(output, &report)?;
        println!("uncalibrated experiment plan: {}", output.display());
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).context("serializing experiment plan")?
        );
    }
    Ok(())
}

fn run_experiment(manifest_path: &Path, output: &Path) -> Result<()> {
    let (manifest, scenario_source, scenario) = load_experiment(manifest_path)?;
    let resolved_assumption_targets = resolve_experiment_assumption_targets(&manifest, &scenario)?;
    let (sensitivity_coverage, sensitivity_studies) = capture_experiment_sensitivity_coverage(
        &manifest,
        manifest_path,
        &scenario,
        &resolved_assumption_targets,
    )?;
    let source_reports = capture_experiment_source_reports(&manifest, manifest_path)?;
    let source_attestations = capture_experiment_osm_attestations(
        &manifest,
        manifest_path,
        &source_reports,
        &scenario_source,
        &scenario,
    )?;
    ensure_empty_directory(output)?;

    write_json(&output.join("manifest.json"), &manifest)?;
    fs::write(output.join("scenario.chy"), &scenario_source)
        .with_context(|| format!("writing scenario snapshot into {}", output.display()))?;
    write_experiment_source_reports(output, &source_reports)?;
    write_experiment_osm_attestations(output, &source_attestations)?;
    write_experiment_sensitivity_studies(output, &sensitivity_studies)?;
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: manifest.trace_every_steps,
        },
    )?;
    write_json(&output.join("run.json"), &bundle)?;
    let report = experiment_report(
        &manifest,
        &fs::read(output.join("manifest.json")).context("reading manifest snapshot")?,
        scenario_source.as_bytes(),
        &bundle,
        source_reports
            .iter()
            .map(|report| report.snapshot.clone())
            .collect(),
        resolved_assumption_targets,
        sensitivity_coverage,
        current_experiment_report_schema(&manifest),
    );
    write_json(&output.join("report.json"), &report)?;
    println!(
        "uncalibrated experiment: {}",
        output.join("report.json").display()
    );
    println!("bundle hash: {}", bundle.bundle_hash);
    Ok(())
}

#[allow(clippy::too_many_lines)] // one verifier binds every independently persisted experiment artifact boundary
fn verify_experiment(directory: &Path) -> Result<()> {
    let manifest: ExperimentManifest = read_json(&directory.join("manifest.json"))?;
    validate_experiment_manifest(&manifest).map_err(|errors| experiment_error(&errors))?;
    let source_reports = verify_experiment_source_reports(directory, &manifest)?;
    verify_experiment_layout(
        directory,
        !source_reports.is_empty(),
        !manifest.source_attestations.is_empty(),
        !manifest.sensitivity_studies.is_empty(),
    )?;
    let scenario_source = read_text(&directory.join("scenario.chy"))?;
    let scenario = parse(&scenario_source).map_err(|error| anyhow::anyhow!(error))?;
    validate(&scenario).map_err(|errors| validation_error(&errors))?;
    let resolved_assumption_targets = resolve_experiment_assumption_targets(&manifest, &scenario)?;
    let sensitivity_coverage = reconstruct_experiment_sensitivity_coverage(
        directory,
        &manifest,
        &scenario,
        &resolved_assumption_targets,
    )?;
    verify_experiment_osm_attestations(
        directory,
        &manifest,
        &source_reports,
        &scenario_source,
        &scenario,
    )?;
    let bundle: RunBundle = read_json(&directory.join("run.json"))?;
    let canonical = CanonicalScenario::from(scenario.clone());
    if bundle.scenario != canonical
        || bundle.scenario_hash != chiyoda_core::bundle::canonical_hash(&bundle.scenario)
    {
        bail!("experiment scenario snapshot does not match its run bundle");
    }
    if bundle.options.get("trace_every_steps") != Some(&manifest.trace_every_steps.to_string()) {
        bail!("experiment run bundle does not use the manifest trace_every_steps");
    }
    if verify_run_bundle(&bundle)? != BundleVerification::Reconstructed {
        bail!("experiment run bundle uses an incompatible runtime contract");
    }
    let persisted_report: serde_json::Value = read_json(&directory.join("report.json"))?;
    let manifest_bytes =
        fs::read(directory.join("manifest.json")).context("reading manifest snapshot")?;
    let report_schema = persisted_report
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        .context("experiment report has no string schema_version")?;
    if matches!(manifest.schema_version.as_str(), "0.3" | "0.4")
        && report_schema != current_experiment_report_schema(&manifest)
    {
        bail!(
            "experiment manifest schema {} requires report schema {}",
            manifest.schema_version,
            current_experiment_report_schema(&manifest)
        );
    }
    let expected_report = match report_schema {
        "0.1" => serde_json::to_value(legacy_experiment_report(
            &manifest,
            &manifest_bytes,
            scenario_source.as_bytes(),
            &bundle,
            source_reports,
        )),
        "0.2" => serde_json::to_value(experiment_report(
            &manifest,
            &manifest_bytes,
            scenario_source.as_bytes(),
            &bundle,
            source_reports,
            Vec::new(),
            None,
            "0.2",
        )),
        "0.3" => serde_json::to_value(experiment_report(
            &manifest,
            &manifest_bytes,
            scenario_source.as_bytes(),
            &bundle,
            source_reports,
            resolved_assumption_targets,
            None,
            "0.3",
        )),
        EXPERIMENT_REPORT_SCHEMA_VERSION => serde_json::to_value(experiment_report(
            &manifest,
            &manifest_bytes,
            scenario_source.as_bytes(),
            &bundle,
            source_reports,
            resolved_assumption_targets,
            sensitivity_coverage,
            EXPERIMENT_REPORT_SCHEMA_VERSION,
        )),
        version => bail!("unsupported experiment report schema `{version}`"),
    }
    .context("serializing reconstructed experiment report")?;
    if persisted_report != expected_report {
        bail!("persisted experiment report does not match reconstruction");
    }
    println!("verified uncalibrated experiment: {}", directory.display());
    Ok(())
}

fn load_experiment(
    manifest_path: &Path,
) -> Result<(ExperimentManifest, String, chiyoda_core::Scenario)> {
    let manifest: ExperimentManifest = read_json(manifest_path)?;
    validate_experiment_manifest(&manifest).map_err(|errors| experiment_error(&errors))?;
    let parent = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let source_path = parent.join(&manifest.scenario_source);
    let source = read_text(&source_path)?;
    let scenario = parse(&source).map_err(|error| anyhow::anyhow!(error))?;
    validate(&scenario).map_err(|errors| validation_error(&errors))?;
    Ok((manifest, source, scenario))
}

fn load_osm_scenario_anchor(
    manifest_path: &Path,
) -> Result<(
    OsmScenarioAnchorManifest,
    Vec<u8>,
    String,
    chiyoda_core::Scenario,
)> {
    let manifest_bytes =
        fs::read(manifest_path).with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: OsmScenarioAnchorManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("parsing {}", manifest_path.display()))?;
    validate_osm_scenario_anchor_manifest(&manifest).map_err(|errors| {
        anyhow::anyhow!(
            "invalid OSM scenario-anchor manifest:\n{}",
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;
    let parent = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let source = read_text(&parent.join(&manifest.scenario_source))?;
    let scenario = parse(&source).map_err(|error| anyhow::anyhow!(error))?;
    validate(&scenario).map_err(|errors| validation_error(&errors))?;
    Ok((manifest, manifest_bytes, source, scenario))
}

fn declared_experiment_source_reports(
    manifest: &ExperimentManifest,
) -> Vec<ExperimentSourceReportSnapshot> {
    manifest
        .sources
        .iter()
        .filter_map(|source| {
            source
                .derived_report
                .as_ref()
                .map(|report| ExperimentSourceReportSnapshot {
                    source_id: source.id.clone(),
                    source_path: report.path.clone(),
                    snapshot_path: format!("source-reports/{}.json", source.id),
                    sha256: report.sha256.clone(),
                })
        })
        .collect()
}

fn capture_experiment_source_reports(
    manifest: &ExperimentManifest,
    manifest_path: &Path,
) -> Result<Vec<CapturedExperimentSourceReport>> {
    let parent = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    declared_experiment_source_reports(manifest)
        .into_iter()
        .map(|snapshot| {
            let path = parent.join(&snapshot.source_path);
            let bytes = fs::read(&path)
                .with_context(|| format!("reading source report {}", path.display()))?;
            if !sha256_hex(&bytes).eq_ignore_ascii_case(&snapshot.sha256) {
                bail!(
                    "source report hash does not match declaration for {}: {}",
                    snapshot.source_id,
                    path.display()
                );
            }
            let _: serde_json::Value = serde_json::from_slice(&bytes)
                .with_context(|| format!("parsing source report {}", path.display()))?;
            Ok(CapturedExperimentSourceReport { snapshot, bytes })
        })
        .collect()
}

fn write_experiment_source_reports(
    output: &Path,
    reports: &[CapturedExperimentSourceReport],
) -> Result<()> {
    for report in reports {
        let path = output.join(&report.snapshot.snapshot_path);
        let parent = path
            .parent()
            .context("experiment source snapshot must have a parent")?;
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        fs::write(&path, &report.bytes)
            .with_context(|| format!("writing source report snapshot {}", path.display()))?;
    }
    Ok(())
}

fn capture_experiment_osm_attestations(
    manifest: &ExperimentManifest,
    manifest_path: &Path,
    source_reports: &[CapturedExperimentSourceReport],
    scenario_source: &str,
    scenario: &chiyoda_core::Scenario,
) -> Result<Vec<CapturedExperimentOsmAttestation>> {
    let parent = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    manifest
        .source_attestations
        .iter()
        .map(|attestation| match attestation {
            ExperimentSourceAttestation::OsmLocalProjection {
                source_id,
                catalog_path,
                data_root,
                observation_report_path,
            } => capture_experiment_osm_local_projection_attestation(
                parent,
                source_reports,
                source_id,
                catalog_path,
                data_root,
                observation_report_path,
            ),
            ExperimentSourceAttestation::OsmScenarioAnchor {
                source_id,
                projection_source_id,
                anchor_manifest_path,
            } => capture_experiment_osm_scenario_anchor_attestation(
                manifest,
                parent,
                source_reports,
                scenario_source,
                scenario,
                source_id,
                projection_source_id,
                anchor_manifest_path,
            ),
        })
        .collect()
}

fn capture_experiment_osm_local_projection_attestation(
    parent: &Path,
    source_reports: &[CapturedExperimentSourceReport],
    source_id: &str,
    catalog_path: &str,
    data_root: &str,
    observation_report_path: &str,
) -> Result<CapturedExperimentOsmAttestation> {
    let source_report = source_reports
        .iter()
        .find(|report| report.snapshot.source_id == source_id)
        .context("validated source attestation has no captured projection report")?;
    let catalog_path = parent.join(catalog_path);
    let catalog_bytes = fs::read(&catalog_path)
        .with_context(|| format!("reading OSM catalog {}", catalog_path.display()))?;
    let catalog: EvidenceCatalog = serde_json::from_slice(&catalog_bytes)
        .with_context(|| format!("parsing OSM catalog {}", catalog_path.display()))?;
    let observation_path = parent.join(observation_report_path);
    let observation_bytes = fs::read(&observation_path).with_context(|| {
        format!(
            "reading OSM observation report {}",
            observation_path.display()
        )
    })?;
    let observation: OpenStreetMapLayoutReport = serde_json::from_slice(&observation_bytes)
        .with_context(|| {
            format!(
                "parsing OSM observation report {}",
                observation_path.display()
            )
        })?;
    let projection: OpenStreetMapLocalProjectionReport =
        serde_json::from_slice(&source_report.bytes).with_context(|| {
            format!("parsing declared OSM projection report for source `{source_id}`")
        })?;
    verify_openstreetmap_layout_report(&catalog, &parent.join(data_root), &observation)
        .with_context(|| format!("verifying OSM source attestation `{source_id}`"))?;
    verify_openstreetmap_local_projection_report(&observation, &projection)
        .with_context(|| format!("verifying OSM projection attestation `{source_id}`"))?;
    Ok(CapturedExperimentOsmAttestation::LocalProjection {
        source_id: source_id.to_owned(),
        catalog_bytes,
        observation_bytes,
    })
}

#[allow(clippy::too_many_arguments)] // one attestation explicitly binds these independent inputs
fn capture_experiment_osm_scenario_anchor_attestation(
    manifest: &ExperimentManifest,
    parent: &Path,
    source_reports: &[CapturedExperimentSourceReport],
    scenario_source: &str,
    scenario: &chiyoda_core::Scenario,
    source_id: &str,
    projection_source_id: &str,
    anchor_manifest_path: &str,
) -> Result<CapturedExperimentOsmAttestation> {
    let anchor_report = source_reports
        .iter()
        .find(|report| report.snapshot.source_id == source_id)
        .context("validated OSM scenario-anchor attestation has no captured report")?;
    let projection_report = source_reports
        .iter()
        .find(|report| report.snapshot.source_id == projection_source_id)
        .context("validated OSM scenario-anchor attestation has no captured projection")?;
    let anchor_path = parent.join(anchor_manifest_path);
    let anchor_manifest_bytes = fs::read(&anchor_path).with_context(|| {
        format!(
            "reading OSM scenario-anchor manifest {}",
            anchor_path.display()
        )
    })?;
    let anchor_manifest = parse_experiment_osm_anchor_manifest(
        &anchor_manifest_bytes,
        source_id,
        &format!("{}", anchor_path.display()),
    )?;
    verify_declared_anchor_scenario_source(
        &anchor_path,
        &anchor_manifest,
        scenario_source,
        source_id,
    )?;
    let anchored: OsmScenarioAnchorReport = serde_json::from_slice(&anchor_report.bytes)
        .with_context(|| {
            format!("parsing declared OSM scenario-anchor report for source `{source_id}`")
        })?;
    let projection: OpenStreetMapLocalProjectionReport =
        serde_json::from_slice(&projection_report.bytes).with_context(|| {
            format!("parsing declared OSM projection report for source `{projection_source_id}`")
        })?;
    verify_osm_scenario_anchor_report(
        &anchor_manifest,
        &sha256_hex(&anchor_manifest_bytes),
        scenario,
        &sha256_hex(scenario_source.as_bytes()),
        &projection,
        &sha256_hex(&projection_report.bytes),
        &anchored,
    )
    .with_context(|| format!("verifying OSM scenario-anchor attestation `{source_id}`"))?;
    verify_declared_anchor_source_hash(manifest, source_id, &anchored)?;
    Ok(CapturedExperimentOsmAttestation::ScenarioAnchor {
        source_id: source_id.to_owned(),
        anchor_manifest_bytes,
    })
}

fn parse_experiment_osm_anchor_manifest(
    bytes: &[u8],
    source_id: &str,
    location: &str,
) -> Result<OsmScenarioAnchorManifest> {
    let manifest: OsmScenarioAnchorManifest = serde_json::from_slice(bytes)
        .with_context(|| format!("parsing OSM scenario-anchor manifest {location}"))?;
    validate_osm_scenario_anchor_manifest(&manifest).map_err(|errors| {
        anyhow::anyhow!(
            "invalid OSM scenario-anchor manifest for attestation `{source_id}`:\n{}",
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;
    Ok(manifest)
}

fn verify_declared_anchor_scenario_source(
    anchor_path: &Path,
    anchor_manifest: &OsmScenarioAnchorManifest,
    scenario_source: &str,
    source_id: &str,
) -> Result<()> {
    let scenario_path = anchor_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(&anchor_manifest.scenario_source);
    let declared = fs::read(&scenario_path).with_context(|| {
        format!(
            "reading OSM scenario-anchor source {}",
            scenario_path.display()
        )
    })?;
    if declared != scenario_source.as_bytes() {
        bail!(
            "OSM scenario-anchor manifest for `{source_id}` does not resolve to the experiment scenario source"
        );
    }
    Ok(())
}

fn verify_declared_anchor_source_hash(
    manifest: &ExperimentManifest,
    source_id: &str,
    anchored: &OsmScenarioAnchorReport,
) -> Result<()> {
    let declared_source = manifest
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .context("validated OSM scenario-anchor attestation has no source declaration")?;
    let expected_source_sha256 = declared_source
        .source_sha256
        .as_deref()
        .context("validated OSM scenario-anchor attestation has no declared raw source hash")?;
    if !expected_source_sha256.eq_ignore_ascii_case(&anchored.source.source_sha256) {
        bail!(
            "OSM scenario-anchor report source hash does not match declaration for `{source_id}`"
        );
    }
    Ok(())
}

fn write_experiment_osm_attestations(
    output: &Path,
    attestations: &[CapturedExperimentOsmAttestation],
) -> Result<()> {
    for attestation in attestations {
        let directory = output
            .join("source-attestations")
            .join(attestation.source_id());
        fs::create_dir_all(&directory)
            .with_context(|| format!("creating {}", directory.display()))?;
        match attestation {
            CapturedExperimentOsmAttestation::LocalProjection {
                source_id,
                catalog_bytes,
                observation_bytes,
            } => {
                fs::write(directory.join("catalog.json"), catalog_bytes)
                    .with_context(|| format!("writing OSM catalog snapshot for `{source_id}`"))?;
                fs::write(directory.join("observation.json"), observation_bytes).with_context(
                    || format!("writing OSM observation snapshot for `{source_id}`"),
                )?;
            }
            CapturedExperimentOsmAttestation::ScenarioAnchor {
                source_id,
                anchor_manifest_bytes,
            } => {
                fs::write(
                    directory.join("anchor-manifest.json"),
                    anchor_manifest_bytes,
                )
                .with_context(|| {
                    format!("writing OSM scenario-anchor manifest snapshot for `{source_id}`")
                })?;
            }
        }
    }
    Ok(())
}

fn verify_experiment_source_reports(
    directory: &Path,
    manifest: &ExperimentManifest,
) -> Result<Vec<ExperimentSourceReportSnapshot>> {
    let expected = declared_experiment_source_reports(manifest);
    let root = directory.join("source-reports");
    if expected.is_empty() {
        if root.exists() {
            bail!("experiment has undeclared source report snapshots");
        }
        return Ok(expected);
    }
    let actual = fs::read_dir(&root)
        .with_context(|| format!("reading {}", root.display()))?
        .map(|entry| {
            let entry = entry.with_context(|| format!("reading {}", root.display()))?;
            if !entry
                .file_type()
                .with_context(|| format!("reading {}", entry.path().display()))?
                .is_file()
            {
                bail!(
                    "experiment source report is not a file: {}",
                    entry.path().display()
                );
            }
            entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("experiment source report name is not UTF-8"))
                .map(|name| format!("source-reports/{name}"))
        })
        .collect::<Result<std::collections::BTreeSet<_>>>()?;
    let expected_paths = expected
        .iter()
        .map(|snapshot| snapshot.snapshot_path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if actual != expected_paths {
        bail!("experiment source report snapshots do not match manifest declarations");
    }
    for snapshot in &expected {
        let bytes = fs::read(directory.join(&snapshot.snapshot_path)).with_context(|| {
            format!("reading source report snapshot {}", snapshot.snapshot_path)
        })?;
        if !sha256_hex(&bytes).eq_ignore_ascii_case(&snapshot.sha256) {
            bail!(
                "source report snapshot hash does not match declaration for {}",
                snapshot.source_id
            );
        }
        let _: serde_json::Value = serde_json::from_slice(&bytes).with_context(|| {
            format!("parsing source report snapshot {}", snapshot.snapshot_path)
        })?;
    }
    Ok(expected)
}

fn verify_experiment_osm_attestations(
    directory: &Path,
    manifest: &ExperimentManifest,
    source_reports: &[ExperimentSourceReportSnapshot],
    scenario_source: &str,
    scenario: &chiyoda_core::Scenario,
) -> Result<()> {
    let root = directory.join("source-attestations");
    if manifest.source_attestations.is_empty() {
        if root.exists() {
            bail!("experiment has undeclared OSM source attestations");
        }
        return Ok(());
    }
    verify_experiment_osm_attestation_layout(&root, manifest)?;

    for attestation in &manifest.source_attestations {
        if let ExperimentSourceAttestation::OsmLocalProjection { source_id, .. } = attestation {
            verify_experiment_osm_local_projection_attestation(
                directory,
                manifest,
                source_reports,
                source_id,
            )?;
        }
    }
    for attestation in &manifest.source_attestations {
        if let ExperimentSourceAttestation::OsmScenarioAnchor {
            source_id,
            projection_source_id,
            ..
        } = attestation
        {
            verify_experiment_osm_scenario_anchor_attestation(
                directory,
                manifest,
                source_reports,
                scenario_source,
                scenario,
                source_id,
                projection_source_id,
            )?;
        }
    }
    Ok(())
}

fn verify_experiment_osm_attestation_layout(
    root: &Path,
    manifest: &ExperimentManifest,
) -> Result<()> {
    let expected_ids = manifest
        .source_attestations
        .iter()
        .map(|attestation| attestation.source_id().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    let actual_ids = fs::read_dir(root)
        .with_context(|| format!("reading {}", root.display()))?
        .map(|entry| {
            let entry = entry.with_context(|| format!("reading {}", root.display()))?;
            if !entry
                .file_type()
                .with_context(|| format!("reading {}", entry.path().display()))?
                .is_dir()
            {
                bail!(
                    "OSM source-attestation entry is not a directory: {}",
                    entry.path().display()
                );
            }
            entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("OSM source-attestation identifier is not UTF-8"))
        })
        .collect::<Result<std::collections::BTreeSet<_>>>()?;
    if actual_ids != expected_ids {
        bail!("OSM source-attestation directories do not match manifest declarations");
    }

    for attestation in &manifest.source_attestations {
        let source_id = attestation.source_id();
        let snapshot_directory = root.join(source_id);
        let actual_files = fs::read_dir(&snapshot_directory)
            .with_context(|| format!("reading {}", snapshot_directory.display()))?
            .map(|entry| {
                let entry =
                    entry.with_context(|| format!("reading {}", snapshot_directory.display()))?;
                if !entry
                    .file_type()
                    .with_context(|| format!("reading {}", entry.path().display()))?
                    .is_file()
                {
                    bail!(
                        "OSM source-attestation entry is not a file: {}",
                        entry.path().display()
                    );
                }
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("OSM source-attestation file name is not UTF-8"))
            })
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        let expected_files = match attestation {
            ExperimentSourceAttestation::OsmLocalProjection { .. } => {
                std::collections::BTreeSet::from([
                    "catalog.json".to_owned(),
                    "observation.json".to_owned(),
                ])
            }
            ExperimentSourceAttestation::OsmScenarioAnchor { .. } => {
                std::collections::BTreeSet::from(["anchor-manifest.json".to_owned()])
            }
        };
        if actual_files != expected_files {
            bail!("OSM source-attestation files do not match the artifact contract");
        }
    }
    Ok(())
}

fn verify_experiment_osm_local_projection_attestation(
    directory: &Path,
    manifest: &ExperimentManifest,
    source_reports: &[ExperimentSourceReportSnapshot],
    source_id: &str,
) -> Result<()> {
    let snapshot_directory = directory.join("source-attestations").join(source_id);
    let catalog: EvidenceCatalog = read_json(&snapshot_directory.join("catalog.json"))?;
    let observation: OpenStreetMapLayoutReport =
        read_json(&snapshot_directory.join("observation.json"))?;
    verify_openstreetmap_layout_catalog_contract(&catalog, &observation)
        .with_context(|| format!("verifying OSM catalog snapshot `{source_id}`"))?;
    let projection_bytes = read_experiment_source_report_snapshot(
        directory,
        source_reports,
        source_id,
        "OSM projection",
    )?;
    let projection: OpenStreetMapLocalProjectionReport = serde_json::from_slice(&projection_bytes)
        .with_context(|| format!("parsing OSM projection report snapshot `{source_id}`"))?;
    verify_openstreetmap_local_projection_report(&observation, &projection)
        .with_context(|| format!("verifying OSM projection snapshot `{source_id}`"))?;
    let declared_source = manifest
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .context("validated OSM source attestation has no source declaration")?;
    if let Some(expected_source_sha256) = &declared_source.source_sha256
        && !expected_source_sha256.eq_ignore_ascii_case(&observation.source.source_sha256)
    {
        bail!("OSM observation snapshot source hash does not match declaration for `{source_id}`");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // one attestation explicitly binds these independent inputs
fn verify_experiment_osm_scenario_anchor_attestation(
    directory: &Path,
    manifest: &ExperimentManifest,
    source_reports: &[ExperimentSourceReportSnapshot],
    scenario_source: &str,
    scenario: &chiyoda_core::Scenario,
    source_id: &str,
    projection_source_id: &str,
) -> Result<()> {
    let snapshot_directory = directory.join("source-attestations").join(source_id);
    let anchor_manifest_bytes = fs::read(snapshot_directory.join("anchor-manifest.json"))
        .with_context(|| format!("reading OSM scenario-anchor manifest snapshot `{source_id}`"))?;
    let anchor_manifest = parse_experiment_osm_anchor_manifest(
        &anchor_manifest_bytes,
        source_id,
        "artifact snapshot",
    )?;
    let anchor_report_bytes = read_experiment_source_report_snapshot(
        directory,
        source_reports,
        source_id,
        "OSM scenario-anchor",
    )?;
    let anchored: OsmScenarioAnchorReport = serde_json::from_slice(&anchor_report_bytes)
        .with_context(|| format!("parsing OSM scenario-anchor report snapshot `{source_id}`"))?;
    let projection_report_bytes = read_experiment_source_report_snapshot(
        directory,
        source_reports,
        projection_source_id,
        "OSM projection",
    )?;
    let projection: OpenStreetMapLocalProjectionReport =
        serde_json::from_slice(&projection_report_bytes).with_context(|| {
            format!("parsing OSM projection report snapshot `{projection_source_id}`")
        })?;
    verify_osm_scenario_anchor_report(
        &anchor_manifest,
        &sha256_hex(&anchor_manifest_bytes),
        scenario,
        &sha256_hex(scenario_source.as_bytes()),
        &projection,
        &sha256_hex(&projection_report_bytes),
        &anchored,
    )
    .with_context(|| format!("verifying OSM scenario-anchor snapshot `{source_id}`"))?;
    verify_declared_anchor_source_hash(manifest, source_id, &anchored)
}

fn read_experiment_source_report_snapshot(
    directory: &Path,
    source_reports: &[ExperimentSourceReportSnapshot],
    source_id: &str,
    report_kind: &str,
) -> Result<Vec<u8>> {
    let snapshot = source_reports
        .iter()
        .find(|report| report.source_id == source_id)
        .with_context(|| format!("validated {report_kind} attestation has no report snapshot"))?;
    fs::read(directory.join(&snapshot.snapshot_path)).with_context(|| {
        format!(
            "reading {report_kind} report snapshot {}",
            snapshot.snapshot_path
        )
    })
}

fn verify_experiment_layout(
    directory: &Path,
    has_source_reports: bool,
    has_source_attestations: bool,
    has_sensitivity_studies: bool,
) -> Result<()> {
    let expected = ["manifest.json", "scenario.chy", "run.json", "report.json"]
        .into_iter()
        .chain(has_source_reports.then_some("source-reports"))
        .chain(has_source_attestations.then_some("source-attestations"))
        .chain(has_sensitivity_studies.then_some("sensitivity-studies"))
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
    let actual = fs::read_dir(directory)
        .with_context(|| format!("reading {}", directory.display()))?
        .map(|entry| {
            let entry = entry.with_context(|| format!("reading {}", directory.display()))?;
            entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("experiment artifact name is not UTF-8"))
        })
        .collect::<Result<std::collections::BTreeSet<_>>>()?;
    if actual != expected {
        bail!("experiment artifact files do not match the manifest contract");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // the report binds independent artifact inputs that must remain explicit
fn experiment_report(
    manifest: &ExperimentManifest,
    manifest_bytes: &[u8],
    scenario_source: &[u8],
    bundle: &RunBundle,
    source_report_snapshots: Vec<ExperimentSourceReportSnapshot>,
    resolved_assumption_targets: Vec<ExperimentResolvedAssumptionTarget>,
    sensitivity_coverage: Option<ExperimentSensitivityCoverage>,
    schema_version: &str,
) -> ExperimentReport {
    ExperimentReport {
        base: experiment_report_base(
            manifest,
            manifest_bytes,
            scenario_source,
            bundle,
            source_report_snapshots,
            resolved_assumption_targets,
            sensitivity_coverage,
            schema_version,
        ),
        runtime_metrics: bundle.metrics.clone(),
    }
}

fn legacy_experiment_report(
    manifest: &ExperimentManifest,
    manifest_bytes: &[u8],
    scenario_source: &[u8],
    bundle: &RunBundle,
    source_report_snapshots: Vec<ExperimentSourceReportSnapshot>,
) -> LegacyExperimentReport {
    LegacyExperimentReport {
        base: experiment_report_base(
            manifest,
            manifest_bytes,
            scenario_source,
            bundle,
            source_report_snapshots,
            Vec::new(),
            None,
            "0.1",
        ),
    }
}

#[allow(clippy::too_many_arguments)] // the base report binds independent artifact inputs that must remain explicit
fn experiment_report_base(
    manifest: &ExperimentManifest,
    manifest_bytes: &[u8],
    scenario_source: &[u8],
    bundle: &RunBundle,
    source_report_snapshots: Vec<ExperimentSourceReportSnapshot>,
    resolved_assumption_targets: Vec<ExperimentResolvedAssumptionTarget>,
    sensitivity_coverage: Option<ExperimentSensitivityCoverage>,
    schema_version: &str,
) -> ExperimentReportBase {
    ExperimentReportBase {
        schema_version: schema_version.to_owned(),
        experiment_name: manifest.name.clone(),
        description: manifest.description.clone(),
        manifest_snapshot: "manifest.json".to_owned(),
        manifest_sha256: sha256_hex(manifest_bytes),
        scenario_snapshot: "scenario.chy".to_owned(),
        scenario_source_sha256: sha256_hex(scenario_source),
        scenario_hash: bundle.scenario_hash.clone(),
        run_bundle: "run.json".to_owned(),
        bundle_hash: bundle.bundle_hash.clone(),
        trace_every_steps: manifest.trace_every_steps,
        source_report_snapshots,
        resolved_assumption_targets,
        sensitivity_coverage,
        author_claim_boundary: manifest.claim_boundary.clone(),
        claim_boundary: "This artifact snapshots one authored, deterministic, uncalibrated structural experiment and its disclosed inputs. It does not establish parameter likelihoods, population behavior, real-world performance, causal effects, predictive validity, operational suitability, or safety.".to_owned(),
    }
}

fn handle_calibration(command: CalibrateCommand) -> Result<()> {
    match command {
        CalibrateCommand::EindhovenPlatform {
            catalog,
            data_root,
            partition,
            output,
        } => {
            let catalog: EvidenceCatalog = read_json(&catalog)?;
            let report =
                calibrate_eindhoven_platform(&catalog, &data_root, dataset_role(partition))?;
            write_json(&output, &report)?;
            println!("descriptive report: {}", output.display());
            println!("status: {}", report.status);
        }
    }
    Ok(())
}

fn handle_reference(command: ReferenceCommand) -> Result<()> {
    match command {
        ReferenceCommand::VruTrajectory {
            catalog,
            data_root,
            output,
        } => {
            let catalog: EvidenceCatalog = read_json(&catalog)?;
            let report = summarize_vru_trajectory_reference(&catalog, &data_root)?;
            write_json(&output, &report)?;
            println!("uncalibrated reference report: {}", output.display());
            println!("status: {}", report.status);
        }
        ReferenceCommand::CrowdQueue {
            catalog,
            data_root,
            output,
        } => {
            let catalog: EvidenceCatalog = read_json(&catalog)?;
            let report = summarize_crowd_queue_reference(&catalog, &data_root)?;
            write_json(&output, &report)?;
            println!("uncalibrated reference report: {}", output.display());
            println!("status: {}", report.status);
        }
    }
    Ok(())
}

fn run_sweep(first_seed: u64, count: u32, output: &Path, trace_every_steps: u32) -> Result<()> {
    prepare_sweep_output(count, output, trace_every_steps)?;
    write_sweep_batch(
        first_seed,
        count,
        output,
        trace_every_steps,
        SweepSource::Generated,
        chiyoda_core::LANGUAGE_VERSION.to_owned(),
        |seed| Ok((generator::source(seed), generator::scenario(seed)?)),
    )
}

fn run_replicates(
    source_path: &Path,
    first_seed: u64,
    count: u32,
    output: &Path,
    trace_every_steps: u32,
) -> Result<()> {
    let template = read_scenario(source_path)?;
    run_authored_replicates(&template, first_seed, count, output, trace_every_steps)
}

fn run_authored_replicates(
    template: &chiyoda_core::Scenario,
    first_seed: u64,
    count: u32,
    output: &Path,
    trace_every_steps: u32,
) -> Result<()> {
    let template_hash =
        chiyoda_core::bundle::canonical_hash(&CanonicalScenario::from(template.clone()));
    prepare_sweep_output(count, output, trace_every_steps)?;
    fs::write(output.join("template.chy"), format_scenario(template))
        .with_context(|| format!("writing template into {}", output.display()))?;
    let template = template.clone();
    write_sweep_batch(
        first_seed,
        count,
        output,
        trace_every_steps,
        SweepSource::Authored {
            template_scenario_hash: template_hash,
        },
        "authored-template".to_owned(),
        move |seed| {
            let mut scenario = template.clone();
            scenario.seed = seed;
            let source = format_scenario(&scenario);
            Ok((source, scenario))
        },
    )
}

fn run_sensitivity(manifest_path: &Path, output: &Path) -> Result<()> {
    let (manifest, baseline_template, study) = load_sensitivity_study(manifest_path)?;
    let reference_reports = capture_sensitivity_reference_reports(&manifest, manifest_path)?;
    let reference_report_snapshots = reference_reports
        .iter()
        .map(|report| report.snapshot.clone())
        .collect();
    prepare_sweep_output(manifest.count, output, manifest.trace_every_steps)?;
    write_json(&output.join("manifest.json"), &manifest)?;
    write_sensitivity_reference_reports(output, &reference_reports)?;

    let baseline_directory = output.join("baseline");
    run_authored_replicates(
        &baseline_template,
        manifest.first_seed,
        manifest.count,
        &baseline_directory,
        manifest.trace_every_steps,
    )?;
    let baseline_template_scenario_hash =
        chiyoda_core::bundle::canonical_hash(&CanonicalScenario::from(baseline_template));
    let mut conditions = Vec::with_capacity(study.conditions.len());
    for condition in study.conditions {
        let condition_id = condition.id.clone();
        let condition_directory = output.join("conditions").join(&condition_id);
        run_authored_replicates(
            &condition.scenario,
            manifest.first_seed,
            manifest.count,
            &condition_directory,
            manifest.trace_every_steps,
        )?;
        let comparison = build_sensitivity_comparison(&baseline_directory, &condition_directory)?;
        let comparison_path = format!("comparisons/{condition_id}.json");
        write_json(&output.join(&comparison_path), &comparison)?;
        conditions.push(sensitivity_condition_report(
            &condition,
            comparison_path,
            &comparison,
        ));
    }
    let report = sensitivity_report(
        &manifest,
        baseline_template_scenario_hash,
        &study.baseline_values,
        reference_report_snapshots,
        conditions,
    );
    let report_path = output.join("report.json");
    write_json(&report_path, &report)?;
    println!("sensitivity study: {}", report_path.display());
    Ok(())
}

fn sensitivity_plan(manifest_path: &Path, output: Option<&Path>) -> Result<()> {
    let (manifest, baseline_template, study) = load_sensitivity_study(manifest_path)?;
    let reference_reports = capture_sensitivity_reference_reports(&manifest, manifest_path)?;
    let condition_count = u64::try_from(study.conditions.len())
        .context("sensitivity condition count does not fit into u64")?;
    let baseline_runs = u64::from(manifest.count);
    let condition_runs = baseline_runs
        .checked_mul(condition_count)
        .context("sensitivity condition run count overflowed")?;
    let total_runs = baseline_runs
        .checked_add(condition_runs)
        .context("sensitivity total run count overflowed")?;
    let integration_steps_per_run = chiyoda_core::integration_step_count(
        baseline_template.duration_s,
        baseline_template.timestep_s,
    );
    let stored_trace_frames_per_run =
        stored_trace_frame_count(integration_steps_per_run, manifest.trace_every_steps)
            .context("sensitivity plan trace-frame count overflowed")?;
    let total_integration_steps = integration_steps_per_run
        .checked_mul(total_runs)
        .context("sensitivity total integration-step count overflowed")?;
    let total_stored_trace_frames = stored_trace_frames_per_run
        .checked_mul(total_runs)
        .context("sensitivity total trace-frame count overflowed")?;
    let report = SensitivityPlanReport {
        schema_version: "0.1".to_owned(),
        study_name: manifest.name,
        description: manifest.description,
        manifest: manifest_path.display().to_string(),
        design: manifest.design,
        baseline: SensitivityBaseline {
            source: manifest.baseline_source,
            template_scenario_hash: chiyoda_core::bundle::canonical_hash(
                &CanonicalScenario::from(baseline_template),
            ),
            sweep_directory: "baseline".to_owned(),
        },
        first_seed: manifest.first_seed,
        run_count_per_condition: manifest.count,
        execution: SensitivityPlanExecution {
            baseline_runs,
            condition_count,
            condition_runs,
            total_runs,
            integration_steps_per_run,
            stored_trace_frames_per_run,
            total_integration_steps,
            total_stored_trace_frames,
        },
        trace_every_steps: manifest.trace_every_steps,
        reference_report_snapshots: reference_reports
            .iter()
            .map(|report| report.snapshot.clone())
            .collect(),
        factors: sensitivity_factor_reports(&manifest.factors, &study.baseline_values),
        conditions: study
            .conditions
            .iter()
            .map(|condition| SensitivityPlanCondition {
                id: condition.id.clone(),
                factor_values: condition.factor_values.clone(),
                template_scenario_hash: chiyoda_core::bundle::canonical_hash(
                    &CanonicalScenario::from(condition.scenario.clone()),
                ),
            })
            .collect(),
        author_claim_boundary: manifest.claim_boundary,
        claim_boundary: "This plan validates and enumerates deterministic structural conditions without executing them. It does not estimate likelihoods, probability distributions, uncertainty intervals, causal effects, real-world performance, or safety.".to_owned(),
    };
    if let Some(output) = output {
        write_json(output, &report)?;
        println!("sensitivity plan: {}", output.display());
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).context("serializing sensitivity plan")?
        );
    }
    Ok(())
}

fn load_sensitivity_study(
    manifest_path: &Path,
) -> Result<(
    SensitivityManifest,
    chiyoda_core::Scenario,
    chiyoda_core::SensitivityStudy,
)> {
    let manifest: SensitivityManifest = read_json(manifest_path)?;
    let manifest_directory = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let baseline_template = read_scenario(&manifest_directory.join(&manifest.baseline_source))?;
    let study =
        plan_sensitivity(&manifest, &baseline_template).map_err(|error| anyhow::anyhow!(error))?;
    Ok((manifest, baseline_template, study))
}

fn declared_sensitivity_reference_reports(
    manifest: &SensitivityManifest,
) -> Vec<SensitivityReferenceReportSnapshot> {
    manifest
        .factors
        .iter()
        .flat_map(|factor| {
            factor.references.iter().filter_map(move |reference| {
                reference
                    .derived_report
                    .as_ref()
                    .map(|report| SensitivityReferenceReportSnapshot {
                        factor_id: factor.id.clone(),
                        reference_id: reference.id.clone(),
                        source_path: report.path.clone(),
                        snapshot_path: format!(
                            "reference-reports/{}/{}.json",
                            factor.id, reference.id
                        ),
                        sha256: report.sha256.clone(),
                    })
            })
        })
        .collect()
}

fn capture_sensitivity_reference_reports(
    manifest: &SensitivityManifest,
    manifest_path: &Path,
) -> Result<Vec<CapturedSensitivityReferenceReport>> {
    let manifest_directory = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    declared_sensitivity_reference_reports(manifest)
        .into_iter()
        .map(|snapshot| {
            let source_path = manifest_directory.join(&snapshot.source_path);
            let bytes = fs::read(&source_path).with_context(|| {
                format!("reading derived reference report {}", source_path.display())
            })?;
            let actual_hash = sha256_hex(&bytes);
            if !actual_hash.eq_ignore_ascii_case(&snapshot.sha256) {
                bail!(
                    "derived reference report hash does not match declaration for {}/{}: {}",
                    snapshot.factor_id,
                    snapshot.reference_id,
                    source_path.display()
                );
            }
            let _: serde_json::Value = serde_json::from_slice(&bytes).with_context(|| {
                format!("parsing derived reference report {}", source_path.display())
            })?;
            Ok(CapturedSensitivityReferenceReport { snapshot, bytes })
        })
        .collect()
}

fn write_sensitivity_reference_reports(
    output: &Path,
    reports: &[CapturedSensitivityReferenceReport],
) -> Result<()> {
    for report in reports {
        let path = output.join(&report.snapshot.snapshot_path);
        let parent = path
            .parent()
            .context("reference snapshot must have a parent")?;
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        fs::write(&path, &report.bytes)
            .with_context(|| format!("writing reference snapshot {}", path.display()))?;
    }
    Ok(())
}

fn verify_sensitivity_reference_reports(
    directory: &Path,
    manifest: &SensitivityManifest,
) -> Result<Vec<SensitivityReferenceReportSnapshot>> {
    let expected = declared_sensitivity_reference_reports(manifest);
    verify_sensitivity_reference_report_layout(directory, &expected)?;
    for snapshot in &expected {
        let path = directory.join(&snapshot.snapshot_path);
        let bytes = fs::read(&path)
            .with_context(|| format!("reading reference snapshot {}", path.display()))?;
        if !sha256_hex(&bytes).eq_ignore_ascii_case(&snapshot.sha256) {
            bail!(
                "reference snapshot hash does not match declaration for {}/{}",
                snapshot.factor_id,
                snapshot.reference_id
            );
        }
        let _: serde_json::Value = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing reference snapshot {}", path.display()))?;
    }
    Ok(expected)
}

fn verify_sensitivity_reference_report_layout(
    directory: &Path,
    expected: &[SensitivityReferenceReportSnapshot],
) -> Result<()> {
    let root = directory.join("reference-reports");
    if expected.is_empty() {
        if !root.exists() {
            return Ok(());
        }
        let mut entries =
            fs::read_dir(&root).with_context(|| format!("reading {}", root.display()))?;
        if entries.next().is_some() {
            bail!("sensitivity study has undeclared reference snapshots");
        }
        return Ok(());
    }

    let actual = fs::read_dir(&root)
        .with_context(|| format!("reading {}", root.display()))?
        .map(|factor| {
            let factor = factor.with_context(|| format!("reading {}", root.display()))?;
            if !factor
                .file_type()
                .with_context(|| format!("reading {}", factor.path().display()))?
                .is_dir()
            {
                bail!(
                    "reference snapshot root contains a non-directory: {}",
                    factor.path().display()
                );
            }
            let factor_id = factor
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("reference snapshot directory name is not UTF-8"))?;
            fs::read_dir(factor.path())
                .with_context(|| format!("reading {}", factor.path().display()))?
                .map(|entry| {
                    let entry =
                        entry.with_context(|| format!("reading {}", factor.path().display()))?;
                    if !entry
                        .file_type()
                        .with_context(|| format!("reading {}", entry.path().display()))?
                        .is_file()
                    {
                        bail!(
                            "reference snapshot directory contains a non-file: {}",
                            entry.path().display()
                        );
                    }
                    let name = entry.file_name().into_string().map_err(|_| {
                        anyhow::anyhow!("reference snapshot file name is not UTF-8")
                    })?;
                    if !Path::new(&name)
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
                    {
                        bail!("reference snapshot file must end in .json: {name}");
                    }
                    Ok(format!("reference-reports/{factor_id}/{name}"))
                })
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<std::collections::BTreeSet<_>>();
    let expected = expected
        .iter()
        .map(|snapshot| snapshot.snapshot_path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if actual != expected {
        bail!("reference snapshot files do not match the manifest declarations");
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn verify_sensitivity(directory: &Path) -> Result<()> {
    let manifest: SensitivityManifest = read_json(&directory.join("manifest.json"))?;
    let baseline_directory = directory.join("baseline");
    let baseline_summary = load_and_verify_sweep(&baseline_directory)?;
    let baseline_template = authored_template(&baseline_directory, &baseline_summary, "baseline")?;
    let study =
        plan_sensitivity(&manifest, &baseline_template).map_err(|error| anyhow::anyhow!(error))?;
    let reference_report_snapshots = verify_sensitivity_reference_reports(directory, &manifest)?;
    let expected_ids = study
        .conditions
        .iter()
        .map(|condition| condition.id.clone())
        .collect();
    verify_sensitivity_children(directory, &expected_ids)?;

    let mut reports = Vec::with_capacity(study.conditions.len());
    for condition in &study.conditions {
        let condition_directory = directory.join("conditions").join(&condition.id);
        let condition_summary = load_and_verify_sweep(&condition_directory)?;
        let persisted_template =
            authored_template(&condition_directory, &condition_summary, &condition.id)?;
        if CanonicalScenario::from(persisted_template.clone())
            != CanonicalScenario::from(condition.scenario.clone())
        {
            bail!(
                "sensitivity condition template does not match its manifest-derived scenario: {}",
                condition.id
            );
        }

        let comparison = build_sensitivity_comparison_from_verified(
            &baseline_summary,
            &condition_summary,
            &baseline_template,
            &persisted_template,
        )?;
        let comparison_path = format!("comparisons/{}.json", condition.id);
        let persisted_comparison: serde_json::Value = read_json(&directory.join(&comparison_path))?;
        let expected_comparison =
            serde_json::to_value(&comparison).context("serializing sensitivity comparison")?;
        if persisted_comparison != expected_comparison {
            bail!(
                "persisted sensitivity comparison does not match reconstructed comparison: {}",
                condition.id
            );
        }
        reports.push(sensitivity_condition_report(
            condition,
            comparison_path,
            &comparison,
        ));
    }

    let baseline_template_scenario_hash =
        chiyoda_core::bundle::canonical_hash(&CanonicalScenario::from(baseline_template));
    let report = sensitivity_report(
        &manifest,
        baseline_template_scenario_hash,
        &study.baseline_values,
        reference_report_snapshots,
        reports,
    );
    let persisted_report: serde_json::Value = read_json(&directory.join("report.json"))?;
    let expected_report =
        serde_json::to_value(&report).context("serializing reconstructed sensitivity report")?;
    if persisted_report != expected_report {
        bail!("persisted sensitivity report does not match reconstructed study");
    }
    println!("verified sensitivity study: {}", directory.display());
    Ok(())
}

fn verify_sensitivity_children(
    directory: &Path,
    expected_ids: &std::collections::BTreeSet<String>,
) -> Result<()> {
    let condition_directory = directory.join("conditions");
    let actual_condition_ids = fs::read_dir(&condition_directory)
        .with_context(|| format!("reading {}", condition_directory.display()))?
        .map(|entry| {
            let entry =
                entry.with_context(|| format!("reading {}", condition_directory.display()))?;
            if !entry
                .file_type()
                .with_context(|| format!("reading {}", entry.path().display()))?
                .is_dir()
            {
                bail!(
                    "sensitivity conditions contains a non-directory: {}",
                    entry.path().display()
                );
            }
            entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("sensitivity condition directory name is not UTF-8"))
        })
        .collect::<Result<std::collections::BTreeSet<_>>>()?;
    if actual_condition_ids != *expected_ids {
        bail!("sensitivity condition directories do not match the manifest plan");
    }

    let comparison_directory = directory.join("comparisons");
    let actual_comparison_ids = fs::read_dir(&comparison_directory)
        .with_context(|| format!("reading {}", comparison_directory.display()))?
        .map(|entry| {
            let entry =
                entry.with_context(|| format!("reading {}", comparison_directory.display()))?;
            if !entry
                .file_type()
                .with_context(|| format!("reading {}", entry.path().display()))?
                .is_file()
            {
                bail!(
                    "sensitivity comparisons contains a non-file: {}",
                    entry.path().display()
                );
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("sensitivity comparison file name is not UTF-8"))?;
            let Some(id) = name.strip_suffix(".json") else {
                bail!("sensitivity comparison file must end in .json: {name}");
            };
            Ok(id.to_owned())
        })
        .collect::<Result<std::collections::BTreeSet<_>>>()?;
    if actual_comparison_ids != *expected_ids {
        bail!("sensitivity comparison files do not match the manifest plan");
    }
    Ok(())
}

fn sensitivity_report(
    manifest: &SensitivityManifest,
    baseline_template_scenario_hash: String,
    baseline_values: &BTreeMap<String, f64>,
    reference_report_snapshots: Vec<SensitivityReferenceReportSnapshot>,
    conditions: Vec<SensitivityConditionReport>,
) -> SensitivityReport {
    let one_at_a_time_responses = one_at_a_time_responses(
        manifest.design,
        &manifest.factors,
        baseline_values,
        &conditions,
    );
    SensitivityReport {
        schema_version: "0.1".to_owned(),
        study_name: manifest.name.clone(),
        description: manifest.description.clone(),
        manifest_snapshot: "manifest.json".to_owned(),
        design: manifest.design,
        baseline: SensitivityBaseline {
            source: manifest.baseline_source.clone(),
            template_scenario_hash: baseline_template_scenario_hash,
            sweep_directory: "baseline".to_owned(),
        },
        first_seed: manifest.first_seed,
        run_count_per_condition: manifest.count,
        trace_every_steps: manifest.trace_every_steps,
        reference_report_snapshots,
        factors: sensitivity_factor_reports(&manifest.factors, baseline_values),
        conditions,
        one_at_a_time_responses,
        author_claim_boundary: manifest.claim_boundary.clone(),
        claim_boundary: "This report enumerates explicit, uncalibrated input alternatives under fixed deterministic seed labels. It does not estimate parameter likelihoods, probability distributions, uncertainty intervals, causal effects, real-world performance, or safety.".to_owned(),
    }
}

fn sensitivity_condition_report(
    condition: &chiyoda_core::SensitivityCondition,
    comparison_path: String,
    comparison: &SweepComparison,
) -> SensitivityConditionReport {
    SensitivityConditionReport {
        id: condition.id.clone(),
        factor_values: condition.factor_values.clone(),
        template_scenario_hash: chiyoda_core::bundle::canonical_hash(&CanonicalScenario::from(
            condition.scenario.clone(),
        )),
        sweep_directory: format!("conditions/{}", condition.id),
        comparison_path,
        outcome: sensitivity_outcome(comparison),
    }
}

fn one_at_a_time_responses(
    design: chiyoda_core::SensitivityDesign,
    factors: &[SensitivityFactor],
    baseline_values: &BTreeMap<String, f64>,
    conditions: &[SensitivityConditionReport],
) -> Option<Vec<SensitivityFactorResponse>> {
    if design != chiyoda_core::SensitivityDesign::OneAtATime {
        return None;
    }
    Some(
        factors
            .iter()
            .map(|factor| {
                let mut alternatives = conditions
                    .iter()
                    .filter_map(|condition| {
                        (condition.factor_values.len() == 1)
                            .then(|| condition.factor_values.get(&factor.id))
                            .flatten()
                            .map(|value| SensitivityResponseObservation {
                                value: *value,
                                outcome: condition.outcome.clone(),
                            })
                    })
                    .collect::<Vec<_>>();
                alternatives.sort_by(|left, right| left.value.total_cmp(&right.value));
                SensitivityFactorResponse {
                    factor_id: factor.id.clone(),
                    baseline_value: baseline_values[&factor.id],
                    unit: factor.target.unit().to_owned(),
                    alternatives,
                }
            })
            .collect(),
    )
}

fn sensitivity_factor_reports(
    factors: &[SensitivityFactor],
    baseline_values: &BTreeMap<String, f64>,
) -> Vec<SensitivityFactorReport> {
    factors
        .iter()
        .cloned()
        .map(|factor| SensitivityFactorReport {
            baseline_value: baseline_values[&factor.id],
            unit: factor.target.unit().to_owned(),
            factor,
        })
        .collect()
}

fn sensitivity_outcome(comparison: &SweepComparison) -> SensitivityOutcome {
    SensitivityOutcome {
        evacuated_agents_delta: comparison
            .aggregate
            .candidate_minus_baseline
            .evacuated_agents,
        un_evacuated_agents_delta: comparison
            .aggregate
            .candidate_minus_baseline
            .un_evacuated_agents,
        baseline_fully_evacuated_runs: comparison.baseline.fully_evacuated_runs,
        candidate_fully_evacuated_runs: comparison.candidate.fully_evacuated_runs,
        queue_experience_delta: comparison
            .aggregate
            .candidate_minus_baseline
            .queue_experience
            .clone(),
        queue_telemetry_delta: comparison
            .aggregate
            .candidate_minus_baseline
            .queue_telemetry
            .clone(),
        movement_telemetry_delta: comparison
            .aggregate
            .candidate_minus_baseline
            .movement_telemetry
            .clone(),
        clearance_time_s: sensitivity_timing(&comparison.aggregate.clearance_time_s),
        last_exit_time_s: sensitivity_timing(&comparison.aggregate.last_exit_time_s),
    }
}

fn sensitivity_timing(timing: &PairedTime) -> SensitivityTiming {
    SensitivityTiming {
        both_recorded_runs: timing.both_recorded_runs,
        baseline_only_recorded_runs: timing.baseline_only_recorded_runs,
        candidate_only_recorded_runs: timing.candidate_only_recorded_runs,
        neither_recorded_runs: timing.neither_recorded_runs,
        candidate_earlier_runs: timing.candidate_earlier_runs,
        candidate_later_runs: timing.candidate_later_runs,
        unchanged_runs: timing.unchanged_runs,
        candidate_minus_baseline_s: timing.candidate_minus_baseline_s.clone(),
    }
}

fn prepare_sweep_output(count: u32, output: &Path, trace_every_steps: u32) -> Result<()> {
    if count == 0 {
        bail!("sweep count must be greater than zero");
    }
    if trace_every_steps == 0 {
        bail!("trace-every must be greater than zero");
    }
    ensure_empty_directory(output)
}

fn write_sweep_batch<F>(
    first_seed: u64,
    count: u32,
    output: &Path,
    trace_every_steps: u32,
    source: SweepSource,
    generator_version: String,
    mut scenario_for_seed: F,
) -> Result<()>
where
    F: FnMut(u64) -> Result<(String, chiyoda_core::Scenario)>,
{
    let mut runs = Vec::with_capacity(usize::try_from(count).expect("u32 fits usize"));
    for offset in 0..count {
        let seed = first_seed
            .checked_add(u64::from(offset))
            .context("sweep seed range exceeds u64")?;
        let (source, scenario) = scenario_for_seed(seed)?;
        let bundle = run(&scenario, RunOptions { trace_every_steps })?;
        let run_directory = output.join(format!("seed-{seed}"));
        fs::create_dir(&run_directory)
            .with_context(|| format!("creating {}", run_directory.display()))?;
        fs::write(run_directory.join("scenario.chy"), source)
            .with_context(|| format!("writing source into {}", run_directory.display()))?;
        write_json(&run_directory.join("run.json"), &bundle)?;
        runs.push(SweepRun {
            seed,
            scenario_name: scenario.name,
            bundle_hash: bundle.bundle_hash,
            bundle_version: Some(bundle.bundle_version),
            runtime_version: Some(bundle.runtime_version),
            total_agents: bundle.metrics.total_agents,
            evacuated_agents: bundle.metrics.evacuated_agents,
            evacuated_by_exit: bundle.metrics.evacuated_by_exit.clone(),
            remaining_by_state: bundle.metrics.remaining_by_state.clone(),
            information_delivery: bundle.metrics.information_delivery.clone(),
            queue_experience: Some(queue_experience_from_metrics(&bundle.metrics)),
            queue_metrics: bundle.metrics.queue_metrics.clone(),
            movement_metrics: bundle.metrics.movement_metrics.clone(),
            clearance_time_s: bundle.metrics.clearance_time_s,
            last_exit_time_s: bundle.metrics.last_exit_time_s,
        });
    }
    let summary = SweepSummary {
        schema_version: "0.1".to_owned(),
        generator_version,
        source,
        first_seed,
        count,
        trace_every_steps,
        runs,
    };
    let summary_path = output.join("summary.json");
    write_json(&summary_path, &summary)?;
    println!("sweep: {}", summary_path.display());
    Ok(())
}

fn verify_sweep(directory: &Path) -> Result<()> {
    let _summary = load_and_verify_sweep(directory)?;
    println!("verified sweep: {}", directory.display());
    Ok(())
}

fn analyze_sweep(directory: &Path, output: Option<&Path>) -> Result<()> {
    let summary = load_and_verify_sweep(directory)?;
    let analysis = describe_sweep(&summary);
    if let Some(output) = output {
        write_json(output, &analysis)?;
        println!("sweep analysis: {}", output.display());
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&analysis).context("serializing sweep analysis")?
        );
    }
    Ok(())
}

fn compare_sweeps(
    baseline_directory: &Path,
    candidate_directory: &Path,
    output: Option<&Path>,
) -> Result<()> {
    let comparison = build_sweep_comparison(baseline_directory, candidate_directory)?;
    if let Some(output) = output {
        write_json(output, &comparison)?;
        println!("sweep comparison: {}", output.display());
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&comparison).context("serializing sweep comparison")?
        );
    }
    Ok(())
}

fn build_sweep_comparison(
    baseline_directory: &Path,
    candidate_directory: &Path,
) -> Result<SweepComparison> {
    build_sweep_comparison_with_policy(
        baseline_directory,
        candidate_directory,
        ComparisonPolicy::RequireMatchingAgentDeclarations,
    )
}

fn build_sensitivity_comparison(
    baseline_directory: &Path,
    candidate_directory: &Path,
) -> Result<SweepComparison> {
    let baseline = load_and_verify_sweep(baseline_directory)?;
    let candidate = load_and_verify_sweep(candidate_directory)?;
    let baseline_template = authored_template(baseline_directory, &baseline, "baseline")?;
    let candidate_template = authored_template(candidate_directory, &candidate, "candidate")?;
    build_sensitivity_comparison_from_verified(
        &baseline,
        &candidate,
        &baseline_template,
        &candidate_template,
    )
}

fn build_sensitivity_comparison_from_verified(
    baseline: &SweepSummary,
    candidate: &SweepSummary,
    baseline_template: &chiyoda_core::Scenario,
    candidate_template: &chiyoda_core::Scenario,
) -> Result<SweepComparison> {
    let changed_scenario_sections = compatible_comparison_sections(
        baseline_template,
        candidate_template,
        ComparisonPolicy::AllowSensitivityAgentDeclarationChanges,
    )?;
    let information_sampling =
        information_sampling_alignment(baseline_template, candidate_template);
    compare_sweep_summaries_with_policy(
        baseline,
        candidate,
        changed_scenario_sections,
        information_sampling,
        ComparisonPolicy::AllowSensitivityAgentDeclarationChanges,
    )
}

fn build_sweep_comparison_with_policy(
    baseline_directory: &Path,
    candidate_directory: &Path,
    policy: ComparisonPolicy,
) -> Result<SweepComparison> {
    let baseline = load_and_verify_sweep(baseline_directory)?;
    let candidate = load_and_verify_sweep(candidate_directory)?;
    let baseline_template = authored_template(baseline_directory, &baseline, "baseline")?;
    let candidate_template = authored_template(candidate_directory, &candidate, "candidate")?;
    let changed_scenario_sections =
        compatible_comparison_sections(&baseline_template, &candidate_template, policy)?;
    let information_sampling =
        information_sampling_alignment(&baseline_template, &candidate_template);
    match policy {
        ComparisonPolicy::RequireMatchingAgentDeclarations => compare_sweep_summaries(
            &baseline,
            &candidate,
            changed_scenario_sections,
            information_sampling,
        ),
        ComparisonPolicy::AllowSensitivityAgentDeclarationChanges => {
            compare_sweep_summaries_with_policy(
                &baseline,
                &candidate,
                changed_scenario_sections,
                information_sampling,
                policy,
            )
        }
    }
}

fn authored_template(
    directory: &Path,
    summary: &SweepSummary,
    arm: &str,
) -> Result<chiyoda_core::Scenario> {
    let SweepSource::Authored {
        template_scenario_hash,
    } = &summary.source
    else {
        bail!("{arm} sweep must be produced by `chiyoda replicate`, not `chiyoda sweep`");
    };
    let template = read_scenario(&directory.join("template.chy"))?;
    let actual_hash =
        chiyoda_core::bundle::canonical_hash(&CanonicalScenario::from(template.clone()));
    if actual_hash != *template_scenario_hash {
        bail!("{arm} authored template hash does not match its summary");
    }
    Ok(template)
}

fn compatible_comparison_sections(
    baseline: &chiyoda_core::Scenario,
    candidate: &chiyoda_core::Scenario,
    policy: ComparisonPolicy,
) -> Result<Vec<String>> {
    if baseline.duration_s.total_cmp(&candidate.duration_s) != std::cmp::Ordering::Equal {
        bail!("comparison requires identical scenario durations");
    }
    if baseline.timestep_s.total_cmp(&candidate.timestep_s) != std::cmp::Ordering::Equal {
        bail!("comparison requires identical simulation timesteps");
    }
    if !policy.allows_agent_declaration_changes() && baseline.agents != candidate.agents {
        bail!("comparison requires identical authored agent demand and journeys");
    }

    let mut changed = Vec::new();
    if baseline.name != candidate.name {
        changed.push("scenario_name".to_owned());
    }
    if baseline.surfaces != candidate.surfaces {
        changed.push("surfaces".to_owned());
    }
    if baseline.obstacles != candidate.obstacles {
        changed.push("obstacles".to_owned());
    }
    if baseline.waypoints != candidate.waypoints {
        changed.push("waypoints".to_owned());
    }
    if baseline.agents != candidate.agents {
        changed.push("agents".to_owned());
    }
    if baseline.exits != candidate.exits {
        changed.push("exits".to_owned());
    }
    if baseline.connectors != candidate.connectors {
        changed.push("connectors".to_owned());
    }
    if baseline.connector_states != candidate.connector_states {
        changed.push("connector_states".to_owned());
    }
    if baseline.exit_states != candidate.exit_states {
        changed.push("exit_states".to_owned());
    }
    if baseline.gates != candidate.gates {
        changed.push("gates".to_owned());
    }
    if baseline.messages != candidate.messages {
        changed.push("messages".to_owned());
    }
    if baseline.countermeasures != candidate.countermeasures {
        changed.push("countermeasures".to_owned());
    }
    Ok(changed)
}

fn information_sampling_alignment(
    baseline: &chiyoda_core::Scenario,
    candidate: &chiyoda_core::Scenario,
) -> InformationSamplingAlignment {
    let mut baseline_keys = declared_sampling_keys(baseline);
    let mut candidate_keys = declared_sampling_keys(candidate);
    let mut shared = BTreeMap::new();
    for sampling_key in baseline_keys.keys().cloned().collect::<Vec<_>>() {
        if candidate_keys.contains_key(&sampling_key) {
            let baseline = baseline_keys
                .remove(&sampling_key)
                .expect("key came from baseline declarations");
            let candidate = candidate_keys
                .remove(&sampling_key)
                .expect("key exists in candidate declarations");
            shared.insert(
                sampling_key,
                SamplingPair {
                    baseline,
                    candidate,
                },
            );
        }
    }
    InformationSamplingAlignment {
        shared,
        baseline_only: baseline_keys,
        candidate_only: candidate_keys,
    }
}

fn declared_sampling_keys(
    scenario: &chiyoda_core::Scenario,
) -> BTreeMap<String, SamplingDeclaration> {
    let mut declarations = BTreeMap::new();
    for message in &scenario.messages {
        declarations.insert(
            message
                .sampling_key
                .clone()
                .unwrap_or_else(|| message.id.clone()),
            SamplingDeclaration {
                intervention: message.id.clone(),
                kind: "message".to_owned(),
            },
        );
    }
    for countermeasure in &scenario.countermeasures {
        declarations.insert(
            countermeasure
                .sampling_key
                .clone()
                .unwrap_or_else(|| countermeasure.id.clone()),
            SamplingDeclaration {
                intervention: countermeasure.id.clone(),
                kind: "countermeasure".to_owned(),
            },
        );
    }
    declarations
}

fn load_and_verify_sweep(directory: &Path) -> Result<SweepSummary> {
    let mut summary: SweepSummary = read_json(&directory.join("summary.json"))?;
    if summary.schema_version != "0.1" {
        bail!(
            "unsupported sweep summary schema `{}`",
            summary.schema_version
        );
    }
    if summary.generator_version.trim().is_empty() {
        bail!("sweep summary must declare a generator version");
    }
    if summary.count == 0 {
        bail!("sweep summary count must be greater than zero");
    }
    if summary.trace_every_steps == 0 {
        bail!("sweep summary trace interval must be greater than zero");
    }
    if summary.runs.len() != usize::try_from(summary.count).expect("u32 fits usize") {
        bail!(
            "sweep summary count {} does not match {} recorded runs",
            summary.count,
            summary.runs.len()
        );
    }
    let authored_template = match &summary.source {
        SweepSource::Generated => None,
        SweepSource::Authored {
            template_scenario_hash,
        } => {
            let template = read_scenario(&directory.join("template.chy"))?;
            let actual_hash =
                chiyoda_core::bundle::canonical_hash(&CanonicalScenario::from(template.clone()));
            if actual_hash != *template_scenario_hash {
                bail!(
                    "authored template hash does not match summary: {}",
                    directory.display()
                );
            }
            Some(template)
        }
    };
    for (offset, record) in summary.runs.iter_mut().enumerate() {
        let expected_seed = summary
            .first_seed
            .checked_add(u64::try_from(offset).expect("usize index fits u64"))
            .context("sweep summary seed range exceeds u64")?;
        if record.seed != expected_seed {
            bail!(
                "sweep run at index {offset} has seed {}, expected {expected_seed}",
                record.seed
            );
        }
        verify_sweep_run(directory, record, authored_template.as_ref())?;
    }
    Ok(summary)
}

fn verify_sweep_run(
    directory: &Path,
    record: &mut SweepRun,
    authored_template: Option<&chiyoda_core::Scenario>,
) -> Result<()> {
    let run_directory = directory.join(format!("seed-{}", record.seed));
    let bundle: RunBundle = read_json(&run_directory.join("run.json"))?;
    let _verification = verify_run_bundle(&bundle)
        .with_context(|| format!("verifying run bundle {}", run_directory.display()))?;
    validate_bundle_metrics(&bundle, &run_directory)?;
    let source = read_text(&run_directory.join("scenario.chy"))?;
    let scenario = parse(&source).map_err(|error| anyhow::anyhow!(error))?;
    validate(&scenario).map_err(|errors| validation_error(&errors))?;
    if CanonicalScenario::from(scenario.clone()) != bundle.scenario {
        bail!(
            "source and canonical scenario disagree: {}",
            run_directory.display()
        );
    }
    if let Some(template) = authored_template {
        let mut expected = template.clone();
        expected.seed = record.seed;
        if CanonicalScenario::from(expected) != bundle.scenario {
            bail!(
                "replication differs from its authored template: {}",
                run_directory.display()
            );
        }
    }
    reconcile_run_provenance(record, &bundle, &run_directory)?;
    if record.scenario_name != bundle.scenario.scenario.name
        || record.bundle_hash != bundle.bundle_hash
        || record.total_agents != bundle.metrics.total_agents
        || record.evacuated_agents != bundle.metrics.evacuated_agents
        || record.evacuated_by_exit != bundle.metrics.evacuated_by_exit
        || record.remaining_by_state != bundle.metrics.remaining_by_state
        || record.information_delivery != bundle.metrics.information_delivery
        || record.clearance_time_s != bundle.metrics.clearance_time_s
        || record.last_exit_time_s != bundle.metrics.last_exit_time_s
    {
        bail!(
            "summary and run bundle disagree: {}",
            run_directory.display()
        );
    }
    let queue_experience = queue_experience_from_metrics(&bundle.metrics);
    if let Some(persisted_queue_experience) = &record.queue_experience
        && persisted_queue_experience != &queue_experience
    {
        bail!(
            "summary queue experience and run bundle disagree: {}",
            run_directory.display()
        );
    }
    // Older summaries omit queue experience. Hydrate it from the verified
    // bundle for downstream analysis without rewriting historical artifacts.
    record.queue_experience = Some(queue_experience);
    if let Some(persisted_queue_metrics) = &record.queue_metrics
        && bundle.metrics.queue_metrics.as_ref() != Some(persisted_queue_metrics)
    {
        bail!(
            "summary queue telemetry and run bundle disagree: {}",
            run_directory.display()
        );
    }
    // Older summaries and bundles may lack detailed queue telemetry. Preserve
    // that absence instead of manufacturing a zero-valued record.
    record
        .queue_metrics
        .clone_from(&bundle.metrics.queue_metrics);
    if let Some(persisted_movement_metrics) = &record.movement_metrics
        && bundle.metrics.movement_metrics.as_ref() != Some(persisted_movement_metrics)
    {
        bail!(
            "summary local-clearance telemetry and run bundle disagree: {}",
            run_directory.display()
        );
    }
    // Older summaries and bundles may lack resolver telemetry. Preserve that
    // absence instead of deriving it from an incomplete trace cadence.
    record
        .movement_metrics
        .clone_from(&bundle.metrics.movement_metrics);
    Ok(())
}

fn queue_experience_from_metrics(metrics: &RunMetrics) -> QueueExperience {
    QueueExperience {
        queued_for_lift_agents: metrics.queued_for_lift_agents,
        queued_for_connector_agents: metrics.queued_for_connector_agents,
        queued_for_gate_agents: metrics.queued_for_gate_agents,
        queued_for_exit_agents: metrics.queued_for_exit_agents,
    }
}

fn reconcile_run_provenance(
    record: &mut SweepRun,
    bundle: &RunBundle,
    run_directory: &Path,
) -> Result<()> {
    if let Some(bundle_version) = &record.bundle_version
        && bundle_version != &bundle.bundle_version
    {
        bail!(
            "summary bundle version disagrees with run bundle: {}",
            run_directory.display()
        );
    }
    if let Some(runtime_version) = &record.runtime_version
        && runtime_version != &bundle.runtime_version
    {
        bail!(
            "summary runtime version disagrees with run bundle: {}",
            run_directory.display()
        );
    }
    // Older summaries did not persist this provenance. The bundle is hash-verified
    // before this call, so hydrate it in memory for analysis and comparison without
    // mutating the source artifact; new summaries always write it explicitly.
    record.bundle_version = Some(bundle.bundle_version.clone());
    record.runtime_version = Some(bundle.runtime_version.clone());
    Ok(())
}

fn validate_on_surface_clearance_audit(
    bundle: &RunBundle,
    directory: &Path,
    audit: &OnSurfaceClearanceMetrics,
) -> Result<()> {
    let total_agents = u64::from(bundle.metrics.total_agents);
    let maximum_pair_steps = chiyoda_core::integration_step_count(
        bundle.scenario.scenario.duration_s,
        bundle.scenario.scenario.timestep_s,
    )
    .saturating_mul(total_agents.saturating_mul(total_agents.saturating_sub(1)) / 2);
    let maximum_disc_overlap_m = bundle
        .scenario
        .scenario
        .agents
        .iter()
        .map(|group| group.radius_m)
        .fold(0.0_f64, f64::max)
        * 2.0;
    let all_zero = audit.agents_with_disc_overlaps == 0
        && audit.disc_overlap_pair_steps == 0
        && audit.maximum_disc_overlap_m == 0.0;
    if u64::from(audit.agents_with_disc_overlaps) > total_agents
        || audit.disc_overlap_pair_steps > maximum_pair_steps
        || !audit.maximum_disc_overlap_m.is_finite()
        || audit.maximum_disc_overlap_m < 0.0
        || audit.maximum_disc_overlap_m > maximum_disc_overlap_m
        || (audit.disc_overlap_pair_steps == 0 && !all_zero)
        || (audit.disc_overlap_pair_steps > 0
            && (audit.agents_with_disc_overlaps < 2 || audit.maximum_disc_overlap_m == 0.0))
    {
        bail!(
            "bundle has invalid on-surface reference-disc audit telemetry: {}",
            directory.display()
        );
    }
    Ok(())
}

fn validate_swept_on_surface_clearance_audit(
    bundle: &RunBundle,
    directory: &Path,
    audit: &SweptOnSurfaceClearanceMetrics,
) -> Result<()> {
    let total_agents = u64::from(bundle.metrics.total_agents);
    let maximum_pair_steps = chiyoda_core::integration_step_count(
        bundle.scenario.scenario.duration_s,
        bundle.scenario.scenario.timestep_s,
    )
    .saturating_mul(total_agents.saturating_mul(total_agents.saturating_sub(1)) / 2);
    let maximum_disc_overlap_m = bundle
        .scenario
        .scenario
        .agents
        .iter()
        .map(|group| group.radius_m)
        .fold(0.0_f64, f64::max)
        * 2.0;
    let all_zero = audit.agents_with_swept_disc_overlaps == 0
        && audit.swept_disc_overlap_pair_steps == 0
        && audit.maximum_swept_disc_overlap_m == 0.0;
    if u64::from(audit.agents_with_swept_disc_overlaps) > total_agents
        || audit.swept_disc_overlap_pair_steps > maximum_pair_steps
        || !audit.maximum_swept_disc_overlap_m.is_finite()
        || audit.maximum_swept_disc_overlap_m < 0.0
        || audit.maximum_swept_disc_overlap_m > maximum_disc_overlap_m
        || (audit.swept_disc_overlap_pair_steps == 0 && !all_zero)
        || (audit.swept_disc_overlap_pair_steps > 0
            && (audit.agents_with_swept_disc_overlaps < 2
                || audit.maximum_swept_disc_overlap_m == 0.0))
    {
        bail!(
            "bundle has invalid swept on-surface reference-disc audit telemetry: {}",
            directory.display()
        );
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // every persisted metric invariant is checked together
fn validate_bundle_metrics(bundle: &RunBundle, directory: &Path) -> Result<()> {
    let metrics = &bundle.metrics;
    if metrics.evacuated_agents > metrics.total_agents {
        bail!(
            "bundle evacuation count exceeds total agents: {}",
            directory.display()
        );
    }
    if metrics.last_exit_time_s.is_some() && metrics.evacuated_agents == 0 {
        bail!(
            "bundle records a last exit time without any evacuations: {}",
            directory.display()
        );
    }
    if let (Some(clearance_time_s), Some(last_exit_time_s)) =
        (metrics.clearance_time_s, metrics.last_exit_time_s)
        && clearance_time_s.total_cmp(&last_exit_time_s) != std::cmp::Ordering::Equal
    {
        bail!(
            "bundle clearance time disagrees with its last exit time: {}",
            directory.display()
        );
    }
    if matches!(
        bundle.bundle_version.as_str(),
        "0.17"
            | "0.18"
            | "0.19"
            | "0.20"
            | "0.21"
            | "0.22"
            | "0.23"
            | "0.24"
            | "0.25"
            | "0.26"
            | "0.27"
            | "0.28"
            | "0.29"
            | "0.30"
            | "0.31"
            | "0.32"
            | "0.33"
            | "0.34"
            | "0.35"
            | "0.36"
            | "0.37"
            | "0.38"
            | "0.39"
            | "0.40"
            | "0.41"
            | "0.42"
    ) {
        let fully_evacuated = metrics.evacuated_agents == metrics.total_agents;
        if metrics.clearance_time_s.is_some() != fully_evacuated {
            bail!(
                "current bundle clearance time does not match full evacuation: {}",
                directory.display()
            );
        }
        if metrics.last_exit_time_s.is_some() != (metrics.evacuated_agents > 0) {
            bail!(
                "current bundle last exit time does not match evacuations: {}",
                directory.display()
            );
        }
    }
    let mut attributed = 0_u32;
    for (exit_id, count) in &metrics.evacuated_by_exit {
        if !bundle
            .scenario
            .scenario
            .exits
            .iter()
            .any(|exit| exit.id == *exit_id)
        {
            bail!(
                "bundle attributes an evacuation to unknown exit `{exit_id}`: {}",
                directory.display()
            );
        }
        attributed = attributed.checked_add(*count).ok_or_else(|| {
            anyhow::anyhow!(
                "bundle exit-attribution count overflows u32: {}",
                directory.display()
            )
        })?;
    }
    if !metrics.evacuated_by_exit.is_empty() && attributed != metrics.evacuated_agents {
        bail!(
            "bundle exit-attribution count does not match evacuations: {}",
            directory.display()
        );
    }
    let mut expected_interventions = BTreeMap::new();
    for message in &bundle.scenario.scenario.messages {
        expected_interventions.insert(
            message.id.clone(),
            chiyoda_core::InformationInterventionKind::Message,
        );
    }
    for countermeasure in &bundle.scenario.scenario.countermeasures {
        expected_interventions.insert(
            countermeasure.id.clone(),
            chiyoda_core::InformationInterventionKind::Countermeasure,
        );
    }
    for (intervention, delivery) in &metrics.information_delivery {
        let Some(expected_kind) = expected_interventions.remove(intervention) else {
            bail!(
                "bundle attributes information delivery to unknown intervention `{intervention}`: {}",
                directory.display()
            );
        };
        if delivery.kind != expected_kind {
            bail!(
                "bundle information-delivery kind disagrees with intervention `{intervention}`: {}",
                directory.display()
            );
        }
        if delivery.accepted_agents > delivery.received_agents {
            bail!(
                "bundle information acceptance exceeds delivery for `{intervention}`: {}",
                directory.display()
            );
        }
    }
    if matches!(
        bundle.bundle_version.as_str(),
        "0.18"
            | "0.19"
            | "0.20"
            | "0.21"
            | "0.22"
            | "0.23"
            | "0.24"
            | "0.25"
            | "0.26"
            | "0.27"
            | "0.28"
            | "0.29"
            | "0.30"
            | "0.31"
            | "0.32"
            | "0.33"
            | "0.34"
            | "0.35"
            | "0.36"
            | "0.37"
            | "0.38"
            | "0.39"
            | "0.40"
            | "0.41"
            | "0.42"
    ) && !expected_interventions.is_empty()
    {
        bail!(
            "0.18 bundle omits information-delivery metrics for: {}",
            expected_interventions
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let mut remaining = 0_u32;
    for (state, count) in &metrics.remaining_by_state {
        if ![
            "moving",
            "waiting_to_depart",
            "waiting_at_waypoint",
            "waiting_for_route",
            "waiting_for_lift",
            "waiting_for_connector",
            "waiting_for_gate",
            "waiting_for_exit",
            "in_transit",
        ]
        .contains(&state.as_str())
        {
            bail!(
                "bundle attributes a remaining agent to unknown state `{state}`: {}",
                directory.display()
            );
        }
        remaining = remaining.checked_add(*count).ok_or_else(|| {
            anyhow::anyhow!(
                "bundle remaining-state count overflows u32: {}",
                directory.display()
            )
        })?;
    }
    if !metrics.remaining_by_state.is_empty()
        && remaining != metrics.total_agents - metrics.evacuated_agents
    {
        bail!(
            "bundle remaining-state count does not match non-evacuated agents: {}",
            directory.display()
        );
    }
    if matches!(
        bundle.bundle_version.as_str(),
        "0.22"
            | "0.23"
            | "0.24"
            | "0.25"
            | "0.26"
            | "0.27"
            | "0.28"
            | "0.29"
            | "0.30"
            | "0.31"
            | "0.32"
            | "0.33"
            | "0.34"
            | "0.35"
            | "0.36"
            | "0.37"
            | "0.38"
            | "0.39"
            | "0.40"
            | "0.41"
            | "0.42"
    ) {
        let queue_metrics = metrics.queue_metrics.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "current bundle omits queue telemetry: {}",
                directory.display()
            )
        })?;
        for (resource, queue, legacy_exposure) in [
            ("lift", &queue_metrics.lift, metrics.queued_for_lift_agents),
            (
                "connector",
                &queue_metrics.connector,
                metrics.queued_for_connector_agents,
            ),
            ("gate", &queue_metrics.gate, metrics.queued_for_gate_agents),
            ("exit", &queue_metrics.exit, metrics.queued_for_exit_agents),
        ] {
            if queue.ever_queued_agents > metrics.total_agents
                || queue.peak_waiting_agents > queue.ever_queued_agents
                || !queue.cumulative_wait_agent_seconds.is_finite()
                || queue.cumulative_wait_agent_seconds < 0.0
            {
                bail!(
                    "bundle has invalid {resource} queue telemetry: {}",
                    directory.display()
                );
            }
            if queue.ever_queued_agents != legacy_exposure {
                bail!(
                    "bundle {resource} queue exposure disagrees with its legacy metric: {}",
                    directory.display()
                );
            }
        }
        if matches!(
            bundle.bundle_version.as_str(),
            "0.23"
                | "0.24"
                | "0.25"
                | "0.26"
                | "0.27"
                | "0.28"
                | "0.29"
                | "0.30"
                | "0.31"
                | "0.32"
                | "0.33"
                | "0.34"
                | "0.35"
                | "0.36"
                | "0.37"
                | "0.38"
                | "0.39"
                | "0.40"
                | "0.41"
                | "0.42"
        ) {
            let by_resource = queue_metrics.by_resource.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "current bundle omits resource-level queue telemetry: {}",
                    directory.display()
                )
            })?;
            let scenario = &bundle.scenario.scenario;
            validate_queue_resource_breakdown(
                directory,
                "lift",
                &by_resource.lifts,
                scenario
                    .connectors
                    .iter()
                    .filter(|connector| connector.is_lift())
                    .map(|connector| connector.id().to_owned()),
                &queue_metrics.lift,
                metrics.total_agents,
            )?;
            validate_queue_resource_breakdown(
                directory,
                "connector",
                &by_resource.connectors,
                scenario
                    .connectors
                    .iter()
                    .filter(|connector| {
                        !connector.is_lift() && connector.service_rate_per_s().is_some()
                    })
                    .map(|connector| connector.id().to_owned()),
                &queue_metrics.connector,
                metrics.total_agents,
            )?;
            validate_queue_resource_breakdown(
                directory,
                "gate",
                &by_resource.gates,
                scenario.gates.iter().map(|gate| gate.id.clone()),
                &queue_metrics.gate,
                metrics.total_agents,
            )?;
            validate_queue_resource_breakdown(
                directory,
                "exit",
                &by_resource.exits,
                scenario
                    .exits
                    .iter()
                    .filter(|exit| exit.capacity_per_s.is_some())
                    .map(|exit| exit.id.clone()),
                &queue_metrics.exit,
                metrics.total_agents,
            )?;
            if matches!(
                bundle.bundle_version.as_str(),
                "0.24"
                    | "0.25"
                    | "0.26"
                    | "0.27"
                    | "0.28"
                    | "0.29"
                    | "0.30"
                    | "0.31"
                    | "0.32"
                    | "0.33"
                    | "0.34"
                    | "0.35"
                    | "0.36"
                    | "0.37"
                    | "0.38"
                    | "0.39"
                    | "0.40"
                    | "0.41"
                    | "0.42"
            ) {
                validate_queue_entry_events(bundle, directory, by_resource, queue_metrics)?;
                if matches!(
                    bundle.bundle_version.as_str(),
                    "0.33"
                        | "0.34"
                        | "0.35"
                        | "0.36"
                        | "0.37"
                        | "0.38"
                        | "0.39"
                        | "0.40"
                        | "0.41"
                        | "0.42"
                ) {
                    validate_queue_service_reservation_events(bundle, directory)?;
                }
                if matches!(bundle.bundle_version.as_str(), "0.41" | "0.42") {
                    validate_queue_grid_preallocation_events(bundle, directory)?;
                }
            }
        }
    }
    if matches!(
        bundle.bundle_version.as_str(),
        "0.28"
            | "0.29"
            | "0.30"
            | "0.31"
            | "0.32"
            | "0.33"
            | "0.34"
            | "0.35"
            | "0.36"
            | "0.37"
            | "0.38"
            | "0.39"
            | "0.40"
            | "0.41"
            | "0.42"
    ) {
        let movement = metrics.movement_metrics.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "current bundle omits local-clearance telemetry: {}",
                directory.display()
            )
        })?;
        let maximum_possible_steps = chiyoda_core::integration_step_count(
            bundle.scenario.scenario.duration_s,
            bundle.scenario.scenario.timestep_s,
        )
        .saturating_mul(u64::from(metrics.total_agents));
        let all_zero = movement.agents_with_local_clearance_adjustments == 0
            && movement.local_clearance_adjustment_steps == 0
            && movement.cumulative_local_clearance_adjustment_m == 0.0
            && movement.maximum_local_clearance_adjustment_m == 0.0;
        if movement.agents_with_local_clearance_adjustments > metrics.total_agents
            || movement.local_clearance_adjustment_steps > maximum_possible_steps
            || movement.local_clearance_adjustment_steps
                < u64::from(movement.agents_with_local_clearance_adjustments)
            || !movement.cumulative_local_clearance_adjustment_m.is_finite()
            || movement.cumulative_local_clearance_adjustment_m < 0.0
            || !movement.maximum_local_clearance_adjustment_m.is_finite()
            || movement.maximum_local_clearance_adjustment_m < 0.0
            || movement.maximum_local_clearance_adjustment_m
                > movement.cumulative_local_clearance_adjustment_m
            || (movement.local_clearance_adjustment_steps == 0 && !all_zero)
        {
            bail!(
                "bundle has invalid local-clearance telemetry: {}",
                directory.display()
            );
        }
        match bundle.bundle_version.as_str() {
            "0.31" | "0.32" | "0.33" | "0.34" | "0.35" | "0.36" | "0.37" | "0.38" | "0.39"
            | "0.40" | "0.41" | "0.42" => {
                let fallback_steps = movement
                    .local_avoidance_constraint_fallback_steps
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "current bundle omits local-motion ORCA fallback telemetry: {}",
                            directory.display()
                        )
                    })?;
                if fallback_steps > maximum_possible_steps {
                    bail!(
                        "bundle has invalid local-motion ORCA fallback telemetry: {}",
                        directory.display()
                    );
                }
                validate_local_avoidance_fallback_events(bundle, directory, fallback_steps)?;
            }
            _ if movement.local_avoidance_constraint_fallback_steps.is_some() => {
                bail!(
                    "pre-0.31 bundle unexpectedly contains local-motion ORCA fallback telemetry: {}",
                    directory.display()
                );
            }
            _ => {}
        }
        match bundle.bundle_version.as_str() {
            "0.36" | "0.37" | "0.38" | "0.39" | "0.40" | "0.41" | "0.42" => {
                let audit = movement
                    .on_surface_clearance_audit
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "current bundle omits on-surface reference-disc audit telemetry: {}",
                            directory.display()
                        )
                    })?;
                validate_on_surface_clearance_audit(bundle, directory, audit)?;
            }
            _ if movement.on_surface_clearance_audit.is_some() => {
                bail!(
                    "pre-0.36 bundle unexpectedly contains on-surface reference-disc audit telemetry: {}",
                    directory.display()
                );
            }
            _ => {}
        }
        match bundle.bundle_version.as_str() {
            "0.37" | "0.38" | "0.39" | "0.40" | "0.41" | "0.42" => {
                let audit = movement
                    .swept_on_surface_clearance_audit
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "current bundle omits swept on-surface reference-disc audit telemetry: {}",
                            directory.display()
                        )
                    })?;
                validate_swept_on_surface_clearance_audit(bundle, directory, audit)?;
            }
            _ if movement.swept_on_surface_clearance_audit.is_some() => {
                bail!(
                    "pre-0.37 bundle unexpectedly contains swept on-surface reference-disc audit telemetry: {}",
                    directory.display()
                );
            }
            _ => {}
        }
    }
    if bundle.bundle_version == "0.42" {
        validate_release_clearance_deferral_events(bundle, directory)?;
    }
    Ok(())
}

/// Verify the 0.42 audit trail for releases held outside the modeled surface.
/// Current bundle reconstruction independently verifies the geometric
/// clearance decision; this reader verifies the event's stable attribution and
/// one-event-per-agent contract.
fn validate_release_clearance_deferral_events(bundle: &RunBundle, directory: &Path) -> Result<()> {
    let initial_agents = bundle
        .trace
        .first()
        .ok_or_else(|| anyhow::anyhow!("bundle lacks initial trace: {}", directory.display()))?
        .agents
        .iter()
        .map(|agent| (agent.id.as_str(), agent.group.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut deferred = BTreeMap::new();
    for event in &bundle.events {
        if event.kind != "agent_release_deferred_for_clearance" {
            continue;
        }
        if !event.time_s.is_finite()
            || event.time_s < 0.0
            || event.time_s > bundle.scenario.scenario.duration_s
            || event.subject.is_empty()
            || initial_agents.get(event.subject.as_str()) != Some(&event.detail.as_str())
            || deferred
                .insert(event.subject.as_str(), event.time_s)
                .is_some()
        {
            bail!(
                "bundle has invalid release-clearance deferral event: {}",
                directory.display()
            );
        }
    }
    for event in &bundle.events {
        let Some(deferred_at_s) = deferred.get(event.subject.as_str()) else {
            continue;
        };
        if event.kind == "agent_released" && event.time_s < *deferred_at_s {
            bail!(
                "bundle releases an agent before its clearance-deferral event: {}",
                directory.display()
            );
        }
    }
    Ok(())
}

fn validate_local_avoidance_fallback_events(
    bundle: &RunBundle,
    directory: &Path,
    expected_steps: u64,
) -> Result<()> {
    const FALLBACK_DETAIL: &str = "the speed-bounded reciprocal constraints were infeasible";
    let mut observed_steps = 0_u64;
    let mut agent_ids = None;
    for event in &bundle.events {
        if event.kind != "local_avoidance_constraint_fallback" {
            continue;
        }
        if !event.time_s.is_finite()
            || event.time_s < 0.0
            || event.subject.is_empty()
            || event.detail != FALLBACK_DETAIL
        {
            bail!(
                "bundle has malformed local-motion ORCA fallback event: {}",
                directory.display()
            );
        }
        let ids = agent_ids.get_or_insert_with(|| {
            bundle
                .trace
                .first()
                .map(|frame| {
                    frame
                        .agents
                        .iter()
                        .map(|agent| agent.id.as_str())
                        .collect::<BTreeSet<_>>()
                })
                .unwrap_or_default()
        });
        if !ids.contains(event.subject.as_str()) {
            bail!(
                "bundle local-motion ORCA fallback event names an unknown agent `{}`: {}",
                event.subject,
                directory.display()
            );
        }
        observed_steps = observed_steps.checked_add(1).ok_or_else(|| {
            anyhow::anyhow!(
                "bundle local-motion ORCA fallback event count overflows: {}",
                directory.display()
            )
        })?;
    }
    if observed_steps != expected_steps {
        bail!(
            "bundle local-motion ORCA fallback events disagree with telemetry: {}",
            directory.display()
        );
    }
    Ok(())
}

#[derive(Default)]
struct QueueEntryEventSets {
    lifts: BTreeMap<String, BTreeSet<String>>,
    connectors: BTreeMap<String, BTreeSet<String>>,
    gates: BTreeMap<String, BTreeSet<String>>,
    exits: BTreeMap<String, BTreeSet<String>>,
}

fn validate_queue_entry_events(
    bundle: &RunBundle,
    directory: &Path,
    by_resource: &chiyoda_core::QueueResourceBreakdown,
    aggregate: &QueueMetrics,
) -> Result<()> {
    let mut entries = QueueEntryEventSets::default();
    for event in &bundle.events {
        let (resource_kind, resources, entries) = match event.kind.as_str() {
            "queue_entered_lift" => ("lift", &by_resource.lifts, &mut entries.lifts),
            "queue_entered_connector" => (
                "connector",
                &by_resource.connectors,
                &mut entries.connectors,
            ),
            "queue_entered_gate" => ("gate", &by_resource.gates, &mut entries.gates),
            "queue_entered_exit" => ("exit", &by_resource.exits, &mut entries.exits),
            _ => continue,
        };
        if !event.time_s.is_finite() || event.time_s < 0.0 || event.subject.is_empty() {
            bail!(
                "bundle has invalid queue-entry event metadata: {}",
                directory.display()
            );
        }
        if event.detail.is_empty() {
            bail!(
                "bundle has queue-entry event without a resource identifier: {}",
                directory.display()
            );
        }
        record_queue_entry_event(
            entries,
            resources,
            &event.detail,
            &event.subject,
            resource_kind,
            directory,
        )?;
    }
    validate_queue_entry_event_group(
        directory,
        "lift",
        &entries.lifts,
        &by_resource.lifts,
        &aggregate.lift,
    )?;
    validate_queue_entry_event_group(
        directory,
        "connector",
        &entries.connectors,
        &by_resource.connectors,
        &aggregate.connector,
    )?;
    validate_queue_entry_event_group(
        directory,
        "gate",
        &entries.gates,
        &by_resource.gates,
        &aggregate.gate,
    )?;
    validate_queue_entry_event_group(
        directory,
        "exit",
        &entries.exits,
        &by_resource.exits,
        &aggregate.exit,
    )?;
    Ok(())
}

fn record_queue_entry_event(
    entries: &mut BTreeMap<String, BTreeSet<String>>,
    resources: &BTreeMap<String, QueueResourceMetrics>,
    resource_id: &str,
    agent_id: &str,
    resource_kind: &str,
    directory: &Path,
) -> Result<()> {
    if !resources.contains_key(resource_id) {
        bail!(
            "bundle has queue-entry event for unknown {resource_kind} `{resource_id}`: {}",
            directory.display()
        );
    }
    if !entries
        .entry(resource_id.to_owned())
        .or_default()
        .insert(agent_id.to_owned())
    {
        bail!(
            "bundle repeats a queue-entry event for {resource_kind} `{resource_id}` and agent `{agent_id}`: {}",
            directory.display()
        );
    }
    Ok(())
}

fn validate_queue_service_reservation_events(bundle: &RunBundle, directory: &Path) -> Result<()> {
    let scenario = &bundle.scenario.scenario;
    let mut footprints = BTreeMap::new();
    for footprint in &scenario.queue_footprints {
        let entry_kind = match &footprint.resource {
            chiyoda_core::model::PortalResource::Connector { id } => scenario
                .connectors
                .iter()
                .find(|connector| connector.id() == id)
                .map_or_else(
                    || {
                        bail!(
                            "bundle queue footprint references an unknown connector: {}",
                            directory.display()
                        )
                    },
                    |connector| {
                        Ok(if connector.is_lift() {
                            "queue_entered_lift"
                        } else {
                            "queue_entered_connector"
                        })
                    },
                )?,
            chiyoda_core::model::PortalResource::Exit { id } => {
                if !scenario.exits.iter().any(|exit| exit.id == *id) {
                    bail!(
                        "bundle queue footprint references an unknown exit: {}",
                        directory.display()
                    );
                }
                "queue_entered_exit"
            }
            chiyoda_core::model::PortalResource::Gate { id } => {
                if !scenario.gates.iter().any(|gate| gate.id == *id) {
                    bail!(
                        "bundle queue footprint references an unknown gate: {}",
                        directory.display()
                    );
                }
                "queue_entered_gate"
            }
        };
        let resource = format!("{}:{}", footprint.resource.kind(), footprint.resource.id());
        if footprints.insert(resource, entry_kind).is_some() {
            bail!(
                "bundle has duplicate queue footprints for one resource: {}",
                directory.display()
            );
        }
    }
    let mut reservations = BTreeSet::new();
    for event in &bundle.events {
        if event.kind != "queue_service_reserved" {
            continue;
        }
        if !event.time_s.is_finite() || event.time_s < 0.0 || event.subject.is_empty() {
            bail!(
                "bundle has invalid queue-service reservation metadata: {}",
                directory.display()
            );
        }
        let Some((resource_kind, resource_id)) = event.detail.split_once(':') else {
            bail!(
                "bundle has malformed queue-service reservation detail: {}",
                directory.display()
            );
        };
        if !matches!(resource_kind, "connector" | "gate" | "exit")
            || resource_id.is_empty()
            || !footprints.contains_key(&event.detail)
        {
            bail!(
                "bundle has queue-service reservation for an unauthored footprint: {}",
                directory.display()
            );
        }
        if !reservations.insert((event.subject.clone(), event.detail.clone())) {
            bail!(
                "bundle repeats a queue-service reservation for one agent/resource pair: {}",
                directory.display()
            );
        }
        let entry_kind = footprints
            .get(&event.detail)
            .expect("validated queue footprint exists");
        let has_prior_entry = bundle.events.iter().any(|entry| {
            entry.kind == *entry_kind
                && entry.subject == event.subject
                && entry.detail == resource_id
                && entry.time_s <= event.time_s
        });
        if !has_prior_entry {
            bail!(
                "bundle queue-service reservation lacks a prior matching queue entry: {}",
                directory.display()
            );
        }
    }
    Ok(())
}

/// Verify the 0.41+ distinction between a grid ticket, physical queue entry,
/// and service reservation without reconstructing the runtime. Full current
/// bundle verification additionally reruns the reference interpreter.
#[allow(clippy::too_many_lines)] // preserves the related ticket/entry/reservation invariants in one audit
fn validate_queue_grid_preallocation_events(bundle: &RunBundle, directory: &Path) -> Result<()> {
    let scenario = &bundle.scenario.scenario;
    let mut grids = BTreeMap::new();
    for footprint in &scenario.queue_footprints {
        if footprint.width_m.is_none() {
            continue;
        }
        let entry_kind = match &footprint.resource {
            chiyoda_core::model::PortalResource::Connector { id } => scenario
                .connectors
                .iter()
                .find(|connector| connector.id() == id)
                .map_or_else(
                    || {
                        bail!(
                            "bundle queue grid references an unknown connector: {}",
                            directory.display()
                        )
                    },
                    |connector| {
                        Ok(if connector.is_lift() {
                            "queue_entered_lift"
                        } else {
                            "queue_entered_connector"
                        })
                    },
                )?,
            chiyoda_core::model::PortalResource::Exit { id } => {
                if !scenario.exits.iter().any(|exit| exit.id == *id) {
                    bail!(
                        "bundle queue grid references an unknown exit: {}",
                        directory.display()
                    );
                }
                "queue_entered_exit"
            }
            chiyoda_core::model::PortalResource::Gate { id } => {
                if !scenario.gates.iter().any(|gate| gate.id == *id) {
                    bail!(
                        "bundle queue grid references an unknown gate: {}",
                        directory.display()
                    );
                }
                "queue_entered_gate"
            }
        };
        let resource = format!("{}:{}", footprint.resource.kind(), footprint.resource.id());
        if grids.insert(resource, entry_kind).is_some() {
            bail!(
                "bundle has duplicate queue grids for one resource: {}",
                directory.display()
            );
        }
    }
    let agent_ids = bundle
        .trace
        .first()
        .map(|frame| {
            frame
                .agents
                .iter()
                .map(|agent| agent.id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let mut assignments = BTreeMap::new();
    let mut prior_ticket = None;
    for event in &bundle.events {
        if event.kind != "queue_slot_preallocated" {
            continue;
        }
        let Some((resource, ticket)) = event.detail.rsplit_once(':') else {
            bail!(
                "bundle has malformed queue-slot preallocation detail: {}",
                directory.display()
            );
        };
        let ticket = ticket.parse::<u64>().map_err(|_| {
            anyhow::anyhow!(
                "bundle has non-numeric queue-slot preallocation ticket: {}",
                directory.display()
            )
        })?;
        if !event.time_s.is_finite()
            || event.time_s < 0.0
            || !agent_ids.contains(event.subject.as_str())
            || !grids.contains_key(resource)
        {
            bail!(
                "bundle has invalid queue-slot preallocation event: {}",
                directory.display()
            );
        }
        if prior_ticket.is_some_and(|previous| ticket <= previous)
            || assignments
                .insert((resource.to_owned(), event.subject.clone()), event.time_s)
                .is_some()
        {
            bail!(
                "bundle repeats or reorders queue-slot preallocation tickets: {}",
                directory.display()
            );
        }
        prior_ticket = Some(ticket);
    }
    for event in &bundle.events {
        let resource = match event.kind.as_str() {
            "queue_entered_lift" | "queue_entered_connector" => {
                format!("connector:{}", event.detail)
            }
            "queue_entered_gate" => format!("gate:{}", event.detail),
            "queue_entered_exit" => format!("exit:{}", event.detail),
            "queue_service_reserved" => event.detail.clone(),
            _ => continue,
        };
        let Some(expected_entry_kind) = grids.get(&resource) else {
            continue;
        };
        if event.kind != "queue_service_reserved" && event.kind != *expected_entry_kind {
            bail!(
                "bundle queue grid uses an incompatible physical-entry event: {}",
                directory.display()
            );
        }
        let Some(assigned_at_s) = assignments.get(&(resource, event.subject.clone())) else {
            bail!(
                "bundle grid queue entry or reservation lacks a slot preallocation: {}",
                directory.display()
            );
        };
        if *assigned_at_s > event.time_s {
            bail!(
                "bundle grid queue entry or reservation precedes its slot preallocation: {}",
                directory.display()
            );
        }
    }
    Ok(())
}

fn validate_queue_entry_event_group(
    directory: &Path,
    resource_kind: &str,
    entries: &BTreeMap<String, BTreeSet<String>>,
    resources: &BTreeMap<String, QueueResourceMetrics>,
    aggregate: &QueueResourceMetrics,
) -> Result<()> {
    let mut agents = BTreeSet::new();
    for (resource_id, telemetry) in resources {
        let resource_agents = entries.get(resource_id).map_or(0, BTreeSet::len);
        if resource_agents != usize::try_from(telemetry.ever_queued_agents).expect("u32 fits usize")
        {
            bail!(
                "bundle {resource_kind} queue-entry events disagree with `{resource_id}` telemetry: {}",
                directory.display()
            );
        }
        agents.extend(entries.get(resource_id).into_iter().flatten().cloned());
    }
    if agents.len() != usize::try_from(aggregate.ever_queued_agents).expect("u32 fits usize") {
        bail!(
            "bundle {resource_kind} queue-entry events disagree with aggregate telemetry: {}",
            directory.display()
        );
    }
    Ok(())
}

fn validate_queue_resource_breakdown(
    directory: &Path,
    resource_kind: &str,
    observed: &BTreeMap<String, QueueResourceMetrics>,
    expected_ids: impl Iterator<Item = String>,
    aggregate: &QueueResourceMetrics,
    total_agents: u32,
) -> Result<()> {
    let expected_ids: BTreeSet<_> = expected_ids.collect();
    let observed_ids: BTreeSet<_> = observed.keys().cloned().collect();
    if observed_ids != expected_ids {
        bail!(
            "bundle {resource_kind} queue-resource identifiers disagree with its scenario: {}",
            directory.display()
        );
    }

    let mut cumulative_wait_agent_seconds = 0.0;
    let mut resource_exposures = 0_u64;
    for (resource_id, queue) in observed {
        if queue.ever_queued_agents > total_agents
            || queue.peak_waiting_agents > queue.ever_queued_agents
            || queue.peak_waiting_agents > aggregate.peak_waiting_agents
            || !queue.cumulative_wait_agent_seconds.is_finite()
            || queue.cumulative_wait_agent_seconds < 0.0
        {
            bail!(
                "bundle has invalid {resource_kind} queue telemetry for `{resource_id}`: {}",
                directory.display()
            );
        }
        cumulative_wait_agent_seconds = canonical_report_number(
            cumulative_wait_agent_seconds + queue.cumulative_wait_agent_seconds,
        );
        resource_exposures += u64::from(queue.ever_queued_agents);
    }
    if cumulative_wait_agent_seconds.total_cmp(&aggregate.cumulative_wait_agent_seconds)
        != std::cmp::Ordering::Equal
    {
        bail!(
            "bundle {resource_kind} queue-resource wait time does not add to its aggregate: {}",
            directory.display()
        );
    }
    if resource_exposures < u64::from(aggregate.ever_queued_agents) {
        bail!(
            "bundle {resource_kind} queue-resource exposures cannot cover its aggregate: {}",
            directory.display()
        );
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // one loop keeps every aggregate attribution auditable
fn describe_sweep(summary: &SweepSummary) -> SweepAnalysis {
    let mut total_agents = 0_u64;
    let mut evacuated_agents = 0_u64;
    let mut runs_with_any_evacuation = 0_u32;
    let mut fully_evacuated_runs = 0_u32;
    let mut evacuated_by_exit = BTreeMap::new();
    let mut remaining_by_state = BTreeMap::new();
    let mut information_delivery = BTreeMap::new();
    let mut queue_experience = AggregateQueueExperience {
        observed_runs: 0,
        unobserved_legacy_runs: 0,
        queued_for_lift_agents: 0,
        queued_for_connector_agents: 0,
        queued_for_gate_agents: 0,
        queued_for_exit_agents: 0,
    };
    let mut queue_telemetry = AggregateQueueTelemetry::default();
    let mut movement_telemetry = AggregateMovementTelemetry::default();
    let mut clearance_times = Vec::new();
    let mut last_exit_times = Vec::new();

    for run in &summary.runs {
        total_agents += u64::from(run.total_agents);
        evacuated_agents += u64::from(run.evacuated_agents);
        if run.evacuated_agents > 0 {
            runs_with_any_evacuation += 1;
        }
        if run.total_agents > 0 && run.evacuated_agents == run.total_agents {
            fully_evacuated_runs += 1;
        }
        for (exit_id, count) in &run.evacuated_by_exit {
            *evacuated_by_exit.entry(exit_id.clone()).or_default() += u64::from(*count);
        }
        for (state, count) in &run.remaining_by_state {
            *remaining_by_state.entry(state.clone()).or_default() += u64::from(*count);
        }
        for (intervention, delivery) in &run.information_delivery {
            let aggregate = information_delivery.entry(intervention.clone()).or_insert(
                AggregateInformationDelivery {
                    received_agents: 0,
                    accepted_agents: 0,
                },
            );
            aggregate.received_agents += u64::from(delivery.received_agents);
            aggregate.accepted_agents += u64::from(delivery.accepted_agents);
        }
        if let Some(run_queue_experience) = &run.queue_experience {
            queue_experience.observed_runs += 1;
            queue_experience.queued_for_lift_agents +=
                u64::from(run_queue_experience.queued_for_lift_agents);
            queue_experience.queued_for_connector_agents +=
                u64::from(run_queue_experience.queued_for_connector_agents);
            queue_experience.queued_for_gate_agents +=
                u64::from(run_queue_experience.queued_for_gate_agents);
            queue_experience.queued_for_exit_agents +=
                u64::from(run_queue_experience.queued_for_exit_agents);
        } else {
            queue_experience.unobserved_legacy_runs += 1;
        }
        if let Some(run_queue_metrics) = &run.queue_metrics {
            queue_telemetry.observed_runs += 1;
            accumulate_queue_resource_telemetry(&mut queue_telemetry.lift, &run_queue_metrics.lift);
            accumulate_queue_resource_telemetry(
                &mut queue_telemetry.connector,
                &run_queue_metrics.connector,
            );
            accumulate_queue_resource_telemetry(&mut queue_telemetry.gate, &run_queue_metrics.gate);
            accumulate_queue_resource_telemetry(&mut queue_telemetry.exit, &run_queue_metrics.exit);
            if let Some(by_resource) = &run_queue_metrics.by_resource {
                queue_telemetry.by_resource.observed_runs += 1;
                accumulate_queue_resource_attribution(
                    &mut queue_telemetry.by_resource,
                    by_resource,
                );
            } else {
                queue_telemetry.by_resource.unobserved_legacy_runs += 1;
            }
        } else {
            queue_telemetry.unobserved_legacy_runs += 1;
            queue_telemetry.by_resource.unobserved_legacy_runs += 1;
        }
        if let Some(run_movement_metrics) = &run.movement_metrics {
            movement_telemetry.observed_runs += 1;
            movement_telemetry.agents_with_local_clearance_adjustments +=
                u64::from(run_movement_metrics.agents_with_local_clearance_adjustments);
            movement_telemetry.local_clearance_adjustment_steps +=
                run_movement_metrics.local_clearance_adjustment_steps;
            if let Some(fallback_steps) =
                run_movement_metrics.local_avoidance_constraint_fallback_steps
            {
                movement_telemetry.constraint_fallback_observed_runs += 1;
                movement_telemetry.local_avoidance_constraint_fallback_steps += fallback_steps;
            } else {
                movement_telemetry.constraint_fallback_unobserved_legacy_runs += 1;
            }
            if let Some(audit) = &run_movement_metrics.on_surface_clearance_audit {
                movement_telemetry.on_surface_clearance_audit_observed_runs += 1;
                movement_telemetry.agents_with_on_surface_disc_overlaps +=
                    u64::from(audit.agents_with_disc_overlaps);
                movement_telemetry.on_surface_disc_overlap_pair_steps +=
                    audit.disc_overlap_pair_steps;
                movement_telemetry.maximum_on_surface_disc_overlap_m = movement_telemetry
                    .maximum_on_surface_disc_overlap_m
                    .max(audit.maximum_disc_overlap_m);
            } else {
                movement_telemetry.on_surface_clearance_audit_unobserved_legacy_runs += 1;
            }
            if let Some(audit) = &run_movement_metrics.swept_on_surface_clearance_audit {
                movement_telemetry.swept_on_surface_clearance_audit_observed_runs += 1;
                movement_telemetry.agents_with_swept_disc_overlaps +=
                    u64::from(audit.agents_with_swept_disc_overlaps);
                movement_telemetry.swept_disc_overlap_pair_steps +=
                    audit.swept_disc_overlap_pair_steps;
                movement_telemetry.maximum_swept_disc_overlap_m = movement_telemetry
                    .maximum_swept_disc_overlap_m
                    .max(audit.maximum_swept_disc_overlap_m);
            } else {
                movement_telemetry.swept_on_surface_clearance_audit_unobserved_legacy_runs += 1;
            }
            movement_telemetry.cumulative_local_clearance_adjustment_m = canonical_report_number(
                movement_telemetry.cumulative_local_clearance_adjustment_m
                    + run_movement_metrics.cumulative_local_clearance_adjustment_m,
            );
            movement_telemetry.maximum_local_clearance_adjustment_m = movement_telemetry
                .maximum_local_clearance_adjustment_m
                .max(run_movement_metrics.maximum_local_clearance_adjustment_m);
        } else {
            movement_telemetry.unobserved_legacy_runs += 1;
        }
        if let Some(clearance_time_s) = observed_clearance_time(run) {
            clearance_times.push(clearance_time_s);
        }
        if let Some(last_exit_time_s) = run.last_exit_time_s.or(run.clearance_time_s) {
            last_exit_times.push(last_exit_time_s);
        }
    }

    let attributed_evacuations = evacuated_by_exit.values().sum::<u64>();
    SweepAnalysis {
        schema_version: "0.1".to_owned(),
        input_sweep_schema_version: summary.schema_version.clone(),
        generator_version: summary.generator_version.clone(),
        source: summary.source.clone(),
        first_seed: summary.first_seed,
        run_count: summary.count,
        total_agents,
        evacuated_agents,
        un_evacuated_agents: total_agents.saturating_sub(evacuated_agents),
        overall_evacuation_fraction: ExactRatio {
            numerator: evacuated_agents,
            denominator: total_agents,
        },
        runs_with_any_evacuation,
        fully_evacuated_runs,
        evacuated_by_exit,
        unattributed_evacuations: evacuated_agents.saturating_sub(attributed_evacuations),
        unattributed_remaining_agents: total_agents
            .saturating_sub(evacuated_agents)
            .saturating_sub(remaining_by_state.values().sum()),
        remaining_by_state,
        information_delivery,
        queue_experience,
        queue_telemetry,
        movement_telemetry,
        clearance_time_s: descriptive_range(&clearance_times),
        last_exit_time_s: descriptive_range(&last_exit_times),
        claim_boundary: "This report aggregates deterministic structural runs. It is not a benchmark score, calibration result, uncertainty estimate, or predictive claim.".to_owned(),
    }
}

fn descriptive_range(values: &[f64]) -> Option<DescriptiveRange> {
    (!values.is_empty()).then(|| {
        let measured_runs = u32::try_from(values.len()).expect("sweep count fits u32");
        DescriptiveRange {
            measured_runs,
            minimum_s: values
                .iter()
                .copied()
                .reduce(f64::min)
                .expect("a non-empty collection has a minimum"),
            mean_s: canonical_report_number(values.iter().sum::<f64>() / f64::from(measured_runs)),
            maximum_s: values
                .iter()
                .copied()
                .reduce(f64::max)
                .expect("a non-empty collection has a maximum"),
        }
    })
}

#[allow(clippy::too_many_lines)] // paired outcome accounting remains auditable in one routine
fn compare_sweep_summaries(
    baseline: &SweepSummary,
    candidate: &SweepSummary,
    changed_scenario_sections: Vec<String>,
    information_sampling: InformationSamplingAlignment,
) -> Result<SweepComparison> {
    compare_sweep_summaries_with_policy(
        baseline,
        candidate,
        changed_scenario_sections,
        information_sampling,
        ComparisonPolicy::RequireMatchingAgentDeclarations,
    )
}

#[allow(clippy::too_many_lines)] // paired outcome accounting remains auditable in one routine
fn compare_sweep_summaries_with_policy(
    baseline: &SweepSummary,
    candidate: &SweepSummary,
    changed_scenario_sections: Vec<String>,
    information_sampling: InformationSamplingAlignment,
    policy: ComparisonPolicy,
) -> Result<SweepComparison> {
    if baseline.first_seed != candidate.first_seed || baseline.count != candidate.count {
        bail!("comparison requires matching contiguous seed ranges");
    }
    let baseline_template_scenario_hash = template_hash_for_comparison(baseline, "baseline")?;
    let candidate_template_scenario_hash = template_hash_for_comparison(candidate, "candidate")?;
    let execution_contract = compatible_execution_contract(baseline, candidate)?;
    let agent_declarations_matched = !changed_scenario_sections
        .iter()
        .any(|section| section == "agents");
    let mut paired_runs = Vec::with_capacity(baseline.runs.len());
    let mut evacuation_delta = 0_i64;
    let mut un_evacuated_delta = 0_i64;
    let mut evacuated_by_exit_delta = BTreeMap::new();
    let mut remaining_by_state_delta = BTreeMap::new();
    let mut information_delivery_delta = BTreeMap::new();
    let mut more_candidate_evacuations = 0_u32;
    let mut fewer_candidate_evacuations = 0_u32;
    let mut unchanged_evacuations = 0_u32;
    let mut clearance_times = PairedTimeAccumulator::default();
    let mut last_exit_times = PairedTimeAccumulator::default();

    for (baseline_run, candidate_run) in baseline.runs.iter().zip(&candidate.runs) {
        if baseline_run.seed != candidate_run.seed {
            bail!(
                "comparison requires matching seed records; baseline has {}, candidate has {}",
                baseline_run.seed,
                candidate_run.seed
            );
        }
        if !policy.allows_agent_declaration_changes()
            && baseline_run.total_agents != candidate_run.total_agents
        {
            bail!(
                "comparison requires equal agents for seed {}; baseline has {}, candidate has {}",
                baseline_run.seed,
                baseline_run.total_agents,
                candidate_run.total_agents
            );
        }
        let current_evacuation_delta = signed_count_delta(
            candidate_run.evacuated_agents,
            baseline_run.evacuated_agents,
        );
        let baseline_un_evacuated = baseline_run.total_agents - baseline_run.evacuated_agents;
        let candidate_un_evacuated = candidate_run.total_agents - candidate_run.evacuated_agents;
        let current_un_evacuated_delta =
            signed_count_delta(candidate_un_evacuated, baseline_un_evacuated);
        evacuation_delta += current_evacuation_delta;
        un_evacuated_delta += current_un_evacuated_delta;
        match current_evacuation_delta.cmp(&0) {
            std::cmp::Ordering::Greater => more_candidate_evacuations += 1,
            std::cmp::Ordering::Less => fewer_candidate_evacuations += 1,
            std::cmp::Ordering::Equal => unchanged_evacuations += 1,
        }
        accumulate_count_delta(
            &mut evacuated_by_exit_delta,
            &candidate_run.evacuated_by_exit,
            &baseline_run.evacuated_by_exit,
        );
        accumulate_count_delta(
            &mut remaining_by_state_delta,
            &candidate_run.remaining_by_state,
            &baseline_run.remaining_by_state,
        );
        accumulate_information_delivery_delta(
            &mut information_delivery_delta,
            &candidate_run.information_delivery,
            &baseline_run.information_delivery,
        );

        let clearance_time_delta_s = clearance_times.record(
            observed_clearance_time(baseline_run),
            observed_clearance_time(candidate_run),
        );
        let last_exit_time_delta_s = last_exit_times.record(
            observed_last_exit_time(baseline_run),
            observed_last_exit_time(candidate_run),
        );
        paired_runs.push(PairedRun {
            seed: baseline_run.seed,
            baseline_total_agents: baseline_run.total_agents,
            candidate_total_agents: candidate_run.total_agents,
            baseline: paired_run_arm(baseline_run),
            candidate: paired_run_arm(candidate_run),
            candidate_minus_baseline: PairedRunDelta {
                evacuated_agents: current_evacuation_delta,
                un_evacuated_agents: current_un_evacuated_delta,
                clearance_time_s: clearance_time_delta_s,
                last_exit_time_s: last_exit_time_delta_s,
            },
        });
    }

    Ok(SweepComparison {
        schema_version: "0.1".to_owned(),
        pairing: SweepPairing {
            first_seed: baseline.first_seed,
            run_count: baseline.count,
            execution_contract,
            baseline_template_scenario_hash,
            candidate_template_scenario_hash,
            agent_declarations_matched,
            changed_scenario_sections,
            information_sampling,
        },
        baseline: describe_sweep(baseline),
        candidate: describe_sweep(candidate),
        paired_runs,
        aggregate: PairedAggregate {
            candidate_minus_baseline: AggregateDelta {
                evacuated_agents: evacuation_delta,
                un_evacuated_agents: un_evacuated_delta,
                evacuated_by_exit: evacuated_by_exit_delta,
                remaining_by_state: remaining_by_state_delta,
                information_delivery: information_delivery_delta,
                queue_experience: queue_experience_delta(&baseline.runs, &candidate.runs),
                queue_telemetry: queue_telemetry_delta(&baseline.runs, &candidate.runs),
                movement_telemetry: movement_telemetry_delta(&baseline.runs, &candidate.runs),
            },
            runs_with_more_candidate_evacuations: more_candidate_evacuations,
            runs_with_fewer_candidate_evacuations: fewer_candidate_evacuations,
            runs_with_unchanged_evacuations: unchanged_evacuations,
            clearance_time_s: clearance_times.report(),
            last_exit_time_s: last_exit_times.report(),
        },
        claim_boundary: comparison_claim_boundary(policy),
    })
}

fn comparison_claim_boundary(policy: ComparisonPolicy) -> String {
    match policy {
        ComparisonPolicy::RequireMatchingAgentDeclarations => "This report compares deterministic structural runs sharing authored demand and seed labels. It is not an empirical control group, a statistical uncertainty estimate, a causal-effect estimate, a benchmark score, calibration result, or predictive claim.".to_owned(),
        ComparisonPolicy::AllowSensitivityAgentDeclarationChanges => "This sensitivity comparison pairs deterministic seed labels, but its declared agent configuration may differ between arms. The per-seed baseline and candidate denominators are retained; seed alignment does not normalize changed demand or make outcome deltas causal, empirical, calibrated, predictive, or safety claims.".to_owned(),
    }
}

fn compatible_execution_contract(
    baseline: &SweepSummary,
    candidate: &SweepSummary,
) -> Result<ExecutionContract> {
    let baseline_contract = execution_contract_for_arm(baseline, "baseline")?;
    let candidate_contract = execution_contract_for_arm(candidate, "candidate")?;
    if baseline_contract != candidate_contract {
        bail!(
            "comparison requires identical bundle and runtime versions; baseline uses bundle `{}` runtime `{}`, candidate uses bundle `{}` runtime `{}`",
            baseline_contract.bundle_version,
            baseline_contract.runtime_version,
            candidate_contract.bundle_version,
            candidate_contract.runtime_version,
        );
    }
    Ok(baseline_contract)
}

fn execution_contract_for_arm(summary: &SweepSummary, arm: &str) -> Result<ExecutionContract> {
    let Some(first_run) = summary.runs.first() else {
        bail!("comparison requires at least one {arm} run");
    };
    let contract = ExecutionContract {
        bundle_version: first_run
            .bundle_version
            .clone()
            .with_context(|| {
                format!(
                    "comparison requires bundle-version provenance for every {arm} run; rerun `chiyoda replicate`"
                )
            })?,
        runtime_version: first_run
            .runtime_version
            .clone()
            .with_context(|| {
                format!(
                    "comparison requires runtime-version provenance for every {arm} run; rerun `chiyoda replicate`"
                )
            })?,
    };
    for run in &summary.runs[1..] {
        if run.bundle_version.as_deref() != Some(contract.bundle_version.as_str())
            || run.runtime_version.as_deref() != Some(contract.runtime_version.as_str())
        {
            bail!(
                "comparison requires every {arm} run to use the same bundle and runtime versions"
            );
        }
    }
    Ok(contract)
}

fn template_hash_for_comparison(summary: &SweepSummary, arm: &str) -> Result<String> {
    match &summary.source {
        SweepSource::Authored {
            template_scenario_hash,
        } => Ok(template_scenario_hash.clone()),
        SweepSource::Generated => {
            bail!("{arm} sweep must be produced by `chiyoda replicate`, not `chiyoda sweep`")
        }
    }
}

fn paired_run_arm(run: &SweepRun) -> PairedRunArm {
    PairedRunArm {
        bundle_hash: run.bundle_hash.clone(),
        evacuated_agents: run.evacuated_agents,
        evacuated_by_exit: run.evacuated_by_exit.clone(),
        remaining_by_state: run.remaining_by_state.clone(),
        information_delivery: run.information_delivery.clone(),
        queue_metrics: run.queue_metrics.clone(),
        movement_metrics: run.movement_metrics.clone(),
        clearance_time_s: observed_clearance_time(run),
        last_exit_time_s: observed_last_exit_time(run),
    }
}

fn observed_last_exit_time(run: &SweepRun) -> Option<f64> {
    run.last_exit_time_s.or(run.clearance_time_s)
}

fn observed_clearance_time(run: &SweepRun) -> Option<f64> {
    (run.evacuated_agents == run.total_agents)
        .then_some(run.clearance_time_s)
        .flatten()
}

impl PairedTimeAccumulator {
    fn record(&mut self, baseline: Option<f64>, candidate: Option<f64>) -> Option<f64> {
        match (baseline, candidate) {
            (Some(baseline_time), Some(candidate_time)) => {
                self.both_recorded_runs += 1;
                let delta = canonical_report_number(candidate_time - baseline_time);
                match delta.total_cmp(&0.0) {
                    std::cmp::Ordering::Less => self.candidate_earlier_runs += 1,
                    std::cmp::Ordering::Greater => self.candidate_later_runs += 1,
                    std::cmp::Ordering::Equal => self.unchanged_runs += 1,
                }
                self.deltas.push(delta);
                Some(delta)
            }
            (Some(_), None) => {
                self.baseline_only_recorded_runs += 1;
                None
            }
            (None, Some(_)) => {
                self.candidate_only_recorded_runs += 1;
                None
            }
            (None, None) => {
                self.neither_recorded_runs += 1;
                None
            }
        }
    }

    fn report(self) -> PairedTime {
        PairedTime {
            both_recorded_runs: self.both_recorded_runs,
            baseline_only_recorded_runs: self.baseline_only_recorded_runs,
            candidate_only_recorded_runs: self.candidate_only_recorded_runs,
            neither_recorded_runs: self.neither_recorded_runs,
            candidate_earlier_runs: self.candidate_earlier_runs,
            candidate_later_runs: self.candidate_later_runs,
            unchanged_runs: self.unchanged_runs,
            candidate_minus_baseline_s: descriptive_range(&self.deltas),
        }
    }
}

fn accumulate_count_delta(
    aggregate: &mut BTreeMap<String, i64>,
    candidate: &BTreeMap<String, u32>,
    baseline: &BTreeMap<String, u32>,
) {
    for key in candidate.keys() {
        let delta = signed_count_delta(
            *candidate.get(key).unwrap_or(&0),
            *baseline.get(key).unwrap_or(&0),
        );
        if delta != 0 {
            *aggregate.entry(key.clone()).or_default() += delta;
        }
    }
    for key in baseline.keys().filter(|key| !candidate.contains_key(*key)) {
        let delta = -i64::from(*baseline.get(key).expect("key came from baseline"));
        *aggregate.entry(key.clone()).or_default() += delta;
    }
    aggregate.retain(|_, count| *count != 0);
}

fn accumulate_information_delivery_delta(
    aggregate: &mut BTreeMap<String, InformationDeliveryDelta>,
    candidate: &BTreeMap<String, InformationDeliveryMetrics>,
    baseline: &BTreeMap<String, InformationDeliveryMetrics>,
) {
    for intervention in candidate.keys().chain(
        baseline
            .keys()
            .filter(|intervention| !candidate.contains_key(*intervention)),
    ) {
        let candidate_delivery = candidate.get(intervention);
        let baseline_delivery = baseline.get(intervention);
        let delta = InformationDeliveryDelta {
            received_agents: signed_count_delta(
                candidate_delivery.map_or(0, |delivery| delivery.received_agents),
                baseline_delivery.map_or(0, |delivery| delivery.received_agents),
            ),
            accepted_agents: signed_count_delta(
                candidate_delivery.map_or(0, |delivery| delivery.accepted_agents),
                baseline_delivery.map_or(0, |delivery| delivery.accepted_agents),
            ),
        };
        if delta.received_agents != 0 || delta.accepted_agents != 0 {
            let current =
                aggregate
                    .entry(intervention.clone())
                    .or_insert(InformationDeliveryDelta {
                        received_agents: 0,
                        accepted_agents: 0,
                    });
            current.received_agents += delta.received_agents;
            current.accepted_agents += delta.accepted_agents;
        }
    }
    aggregate.retain(|_, delta| delta.received_agents != 0 || delta.accepted_agents != 0);
}

fn signed_count_delta(candidate: u32, baseline: u32) -> i64 {
    i64::from(candidate) - i64::from(baseline)
}

fn accumulate_queue_resource_telemetry(
    aggregate: &mut AggregateQueueResourceTelemetry,
    resource: &QueueResourceMetrics,
) {
    aggregate.ever_queued_agents += u64::from(resource.ever_queued_agents);
    aggregate.cumulative_wait_agent_seconds = canonical_report_number(
        aggregate.cumulative_wait_agent_seconds + resource.cumulative_wait_agent_seconds,
    );
    aggregate.maximum_peak_waiting_agents = aggregate
        .maximum_peak_waiting_agents
        .max(resource.peak_waiting_agents);
}

fn accumulate_queue_resource_attribution(
    aggregate: &mut AggregateQueueResourceAttribution,
    by_resource: &chiyoda_core::QueueResourceBreakdown,
) {
    accumulate_queue_resource_map(&mut aggregate.lifts, &by_resource.lifts);
    accumulate_queue_resource_map(&mut aggregate.connectors, &by_resource.connectors);
    accumulate_queue_resource_map(&mut aggregate.gates, &by_resource.gates);
    accumulate_queue_resource_map(&mut aggregate.exits, &by_resource.exits);
}

fn accumulate_queue_resource_map(
    aggregate: &mut BTreeMap<String, AggregateQueueResourceTelemetry>,
    resources: &BTreeMap<String, QueueResourceMetrics>,
) {
    for (resource_id, telemetry) in resources {
        accumulate_queue_resource_telemetry(
            aggregate.entry(resource_id.clone()).or_default(),
            telemetry,
        );
    }
}

fn queue_experience_delta(
    baseline_runs: &[SweepRun],
    candidate_runs: &[SweepRun],
) -> Option<QueueExperienceDelta> {
    if baseline_runs.len() != candidate_runs.len() {
        return None;
    }
    let mut delta = QueueExperienceDelta {
        queued_for_lift_agents: 0,
        queued_for_connector_agents: 0,
        queued_for_gate_agents: 0,
        queued_for_exit_agents: 0,
    };
    for (baseline, candidate) in baseline_runs.iter().zip(candidate_runs) {
        let (Some(baseline), Some(candidate)) =
            (&baseline.queue_experience, &candidate.queue_experience)
        else {
            return None;
        };
        delta.queued_for_lift_agents += signed_count_delta(
            candidate.queued_for_lift_agents,
            baseline.queued_for_lift_agents,
        );
        delta.queued_for_connector_agents += signed_count_delta(
            candidate.queued_for_connector_agents,
            baseline.queued_for_connector_agents,
        );
        delta.queued_for_gate_agents += signed_count_delta(
            candidate.queued_for_gate_agents,
            baseline.queued_for_gate_agents,
        );
        delta.queued_for_exit_agents += signed_count_delta(
            candidate.queued_for_exit_agents,
            baseline.queued_for_exit_agents,
        );
    }
    Some(delta)
}

fn queue_telemetry_delta(
    baseline_runs: &[SweepRun],
    candidate_runs: &[SweepRun],
) -> Option<QueueTelemetryDelta> {
    if baseline_runs.len() != candidate_runs.len() {
        return None;
    }
    let mut delta = QueueTelemetryDelta::default();
    let mut baseline_peaks = [0_u32; 4];
    let mut candidate_peaks = [0_u32; 4];
    let mut resource_delta = QueueResourceTelemetryDeltaBreakdown::default();
    let mut resource_peaks = QueueResourcePeakBreakdown::default();
    let mut has_complete_resource_attribution = true;
    for (baseline, candidate) in baseline_runs.iter().zip(candidate_runs) {
        let (Some(baseline), Some(candidate)) = (&baseline.queue_metrics, &candidate.queue_metrics)
        else {
            return None;
        };
        for (
            (resource_delta, baseline_resource, candidate_resource),
            (baseline_peak, candidate_peak),
        ) in [
            (&mut delta.lift, &baseline.lift, &candidate.lift),
            (
                &mut delta.connector,
                &baseline.connector,
                &candidate.connector,
            ),
            (&mut delta.gate, &baseline.gate, &candidate.gate),
            (&mut delta.exit, &baseline.exit, &candidate.exit),
        ]
        .into_iter()
        .zip(baseline_peaks.iter_mut().zip(candidate_peaks.iter_mut()))
        {
            accumulate_queue_resource_delta(resource_delta, candidate_resource, baseline_resource);
            *baseline_peak = (*baseline_peak).max(baseline_resource.peak_waiting_agents);
            *candidate_peak = (*candidate_peak).max(candidate_resource.peak_waiting_agents);
        }
        if has_complete_resource_attribution {
            let (Some(baseline_by_resource), Some(candidate_by_resource)) =
                (&baseline.by_resource, &candidate.by_resource)
            else {
                has_complete_resource_attribution = false;
                continue;
            };
            accumulate_attributed_queue_resource_delta(
                &mut resource_delta.lifts,
                &mut resource_peaks.lifts,
                &baseline_by_resource.lifts,
                &candidate_by_resource.lifts,
            );
            accumulate_attributed_queue_resource_delta(
                &mut resource_delta.connectors,
                &mut resource_peaks.connectors,
                &baseline_by_resource.connectors,
                &candidate_by_resource.connectors,
            );
            accumulate_attributed_queue_resource_delta(
                &mut resource_delta.gates,
                &mut resource_peaks.gates,
                &baseline_by_resource.gates,
                &candidate_by_resource.gates,
            );
            accumulate_attributed_queue_resource_delta(
                &mut resource_delta.exits,
                &mut resource_peaks.exits,
                &baseline_by_resource.exits,
                &candidate_by_resource.exits,
            );
        }
    }
    for (resource_delta, (candidate_peak, baseline_peak)) in [
        &mut delta.lift,
        &mut delta.connector,
        &mut delta.gate,
        &mut delta.exit,
    ]
    .into_iter()
    .zip(candidate_peaks.into_iter().zip(baseline_peaks))
    {
        resource_delta.maximum_peak_waiting_agents =
            signed_count_delta(candidate_peak, baseline_peak);
    }
    if has_complete_resource_attribution {
        finalize_attributed_queue_resource_delta(&mut resource_delta.lifts, &resource_peaks.lifts);
        finalize_attributed_queue_resource_delta(
            &mut resource_delta.connectors,
            &resource_peaks.connectors,
        );
        finalize_attributed_queue_resource_delta(&mut resource_delta.gates, &resource_peaks.gates);
        finalize_attributed_queue_resource_delta(&mut resource_delta.exits, &resource_peaks.exits);
        delta.by_resource = Some(resource_delta);
    }
    Some(delta)
}

#[allow(clippy::too_many_lines)] // every independently coverage-labeled movement audit is paired together
fn movement_telemetry_delta(
    baseline_runs: &[SweepRun],
    candidate_runs: &[SweepRun],
) -> Option<MovementTelemetryDelta> {
    if baseline_runs.len() != candidate_runs.len() {
        return None;
    }
    let mut delta = MovementTelemetryDelta::default();
    let mut baseline_maximum_adjustment_m = 0.0_f64;
    let mut candidate_maximum_adjustment_m = 0.0_f64;
    let mut fallback_delta = 0_i128;
    let mut fallback_coverage_complete = true;
    let mut on_surface_clearance_delta = OnSurfaceClearanceAuditDelta::default();
    let mut baseline_maximum_disc_overlap_m = 0.0_f64;
    let mut candidate_maximum_disc_overlap_m = 0.0_f64;
    let mut on_surface_clearance_coverage_complete = true;
    let mut swept_on_surface_clearance_delta = SweptOnSurfaceClearanceAuditDelta::default();
    let mut baseline_maximum_swept_disc_overlap_m = 0.0_f64;
    let mut candidate_maximum_swept_disc_overlap_m = 0.0_f64;
    let mut swept_on_surface_clearance_coverage_complete = true;
    for (baseline, candidate) in baseline_runs.iter().zip(candidate_runs) {
        let (Some(baseline), Some(candidate)) =
            (&baseline.movement_metrics, &candidate.movement_metrics)
        else {
            return None;
        };
        delta.agents_with_local_clearance_adjustments += signed_count_delta(
            candidate.agents_with_local_clearance_adjustments,
            baseline.agents_with_local_clearance_adjustments,
        );
        delta.local_clearance_adjustment_steps +=
            i128::from(candidate.local_clearance_adjustment_steps)
                - i128::from(baseline.local_clearance_adjustment_steps);
        match (
            baseline.local_avoidance_constraint_fallback_steps,
            candidate.local_avoidance_constraint_fallback_steps,
        ) {
            (Some(baseline_steps), Some(candidate_steps)) => {
                fallback_delta += i128::from(candidate_steps) - i128::from(baseline_steps);
            }
            _ => fallback_coverage_complete = false,
        }
        match (
            &baseline.on_surface_clearance_audit,
            &candidate.on_surface_clearance_audit,
        ) {
            (Some(baseline_audit), Some(candidate_audit)) => {
                on_surface_clearance_delta.agents_with_disc_overlaps += signed_count_delta(
                    candidate_audit.agents_with_disc_overlaps,
                    baseline_audit.agents_with_disc_overlaps,
                );
                on_surface_clearance_delta.disc_overlap_pair_steps +=
                    i128::from(candidate_audit.disc_overlap_pair_steps)
                        - i128::from(baseline_audit.disc_overlap_pair_steps);
                baseline_maximum_disc_overlap_m =
                    baseline_maximum_disc_overlap_m.max(baseline_audit.maximum_disc_overlap_m);
                candidate_maximum_disc_overlap_m =
                    candidate_maximum_disc_overlap_m.max(candidate_audit.maximum_disc_overlap_m);
            }
            _ => on_surface_clearance_coverage_complete = false,
        }
        match (
            &baseline.swept_on_surface_clearance_audit,
            &candidate.swept_on_surface_clearance_audit,
        ) {
            (Some(baseline_audit), Some(candidate_audit)) => {
                swept_on_surface_clearance_delta.agents_with_swept_disc_overlaps +=
                    signed_count_delta(
                        candidate_audit.agents_with_swept_disc_overlaps,
                        baseline_audit.agents_with_swept_disc_overlaps,
                    );
                swept_on_surface_clearance_delta.swept_disc_overlap_pair_steps +=
                    i128::from(candidate_audit.swept_disc_overlap_pair_steps)
                        - i128::from(baseline_audit.swept_disc_overlap_pair_steps);
                baseline_maximum_swept_disc_overlap_m = baseline_maximum_swept_disc_overlap_m
                    .max(baseline_audit.maximum_swept_disc_overlap_m);
                candidate_maximum_swept_disc_overlap_m = candidate_maximum_swept_disc_overlap_m
                    .max(candidate_audit.maximum_swept_disc_overlap_m);
            }
            _ => swept_on_surface_clearance_coverage_complete = false,
        }
        delta.cumulative_local_clearance_adjustment_m = canonical_report_number(
            delta.cumulative_local_clearance_adjustment_m
                + candidate.cumulative_local_clearance_adjustment_m
                - baseline.cumulative_local_clearance_adjustment_m,
        );
        baseline_maximum_adjustment_m =
            baseline_maximum_adjustment_m.max(baseline.maximum_local_clearance_adjustment_m);
        candidate_maximum_adjustment_m =
            candidate_maximum_adjustment_m.max(candidate.maximum_local_clearance_adjustment_m);
    }
    delta.maximum_local_clearance_adjustment_m =
        canonical_report_number(candidate_maximum_adjustment_m - baseline_maximum_adjustment_m);
    delta.local_avoidance_constraint_fallback_steps =
        fallback_coverage_complete.then_some(fallback_delta);
    if on_surface_clearance_coverage_complete {
        on_surface_clearance_delta.maximum_disc_overlap_m = canonical_report_number(
            candidate_maximum_disc_overlap_m - baseline_maximum_disc_overlap_m,
        );
        delta.on_surface_clearance_audit = Some(on_surface_clearance_delta);
    }
    if swept_on_surface_clearance_coverage_complete {
        swept_on_surface_clearance_delta.maximum_swept_disc_overlap_m = canonical_report_number(
            candidate_maximum_swept_disc_overlap_m - baseline_maximum_swept_disc_overlap_m,
        );
        delta.swept_on_surface_clearance_audit = Some(swept_on_surface_clearance_delta);
    }
    Some(delta)
}

fn accumulate_queue_resource_delta(
    delta: &mut QueueResourceTelemetryDelta,
    candidate: &QueueResourceMetrics,
    baseline: &QueueResourceMetrics,
) {
    delta.ever_queued_agents +=
        signed_count_delta(candidate.ever_queued_agents, baseline.ever_queued_agents);
    delta.cumulative_wait_agent_seconds = canonical_report_number(
        delta.cumulative_wait_agent_seconds + candidate.cumulative_wait_agent_seconds
            - baseline.cumulative_wait_agent_seconds,
    );
}

#[derive(Default)]
struct QueueResourcePeakBreakdown {
    lifts: BTreeMap<String, (u32, u32)>,
    connectors: BTreeMap<String, (u32, u32)>,
    gates: BTreeMap<String, (u32, u32)>,
    exits: BTreeMap<String, (u32, u32)>,
}

fn accumulate_attributed_queue_resource_delta(
    deltas: &mut BTreeMap<String, AttributedQueueResourceTelemetryDelta>,
    peaks: &mut BTreeMap<String, (u32, u32)>,
    baseline: &BTreeMap<String, QueueResourceMetrics>,
    candidate: &BTreeMap<String, QueueResourceMetrics>,
) {
    for resource_id in baseline.keys().chain(
        candidate
            .keys()
            .filter(|resource_id| !baseline.contains_key(*resource_id)),
    ) {
        let baseline_resource = baseline.get(resource_id);
        let candidate_resource = candidate.get(resource_id);
        let delta = deltas.entry(resource_id.clone()).or_default();
        delta.baseline_resource_declared |= baseline_resource.is_some();
        delta.candidate_resource_declared |= candidate_resource.is_some();
        delta.ever_queued_agents += signed_count_delta(
            candidate_resource.map_or(0, |resource| resource.ever_queued_agents),
            baseline_resource.map_or(0, |resource| resource.ever_queued_agents),
        );
        delta.cumulative_wait_agent_seconds = canonical_report_number(
            delta.cumulative_wait_agent_seconds
                + candidate_resource.map_or(0.0, |resource| resource.cumulative_wait_agent_seconds)
                - baseline_resource.map_or(0.0, |resource| resource.cumulative_wait_agent_seconds),
        );
        let peak = peaks.entry(resource_id.clone()).or_default();
        peak.0 = peak
            .0
            .max(baseline_resource.map_or(0, |resource| resource.peak_waiting_agents));
        peak.1 = peak
            .1
            .max(candidate_resource.map_or(0, |resource| resource.peak_waiting_agents));
    }
}

fn finalize_attributed_queue_resource_delta(
    deltas: &mut BTreeMap<String, AttributedQueueResourceTelemetryDelta>,
    peaks: &BTreeMap<String, (u32, u32)>,
) {
    for (resource_id, delta) in deltas {
        let (baseline_peak, candidate_peak) = peaks
            .get(resource_id)
            .expect("every attributed queue delta has tracked peaks");
        delta.maximum_peak_waiting_agents = signed_count_delta(*candidate_peak, *baseline_peak);
    }
}

fn canonical_report_number(value: f64) -> f64 {
    (value * 1_000_000_000.0).round() / 1_000_000_000.0
}

fn ensure_empty_directory(path: &Path) -> Result<()> {
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_dir() => bail!("{} must be a directory", path.display()),
        Ok(_) => {
            if fs::read_dir(path)
                .with_context(|| format!("reading {}", path.display()))?
                .next()
                .is_some()
            {
                bail!("{} must be empty", path.display());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).with_context(|| format!("creating {}", path.display()))?;
        }
        Err(error) => return Err(error).with_context(|| format!("checking {}", path.display())),
    }
    Ok(())
}

fn read_scenario(path: &Path) -> Result<chiyoda_core::Scenario> {
    let text = read_text(path)?;
    let scenario = parse(&text).map_err(|error| anyhow::anyhow!(error))?;
    validate(&scenario).map_err(|errors| validation_error(&errors))?;
    Ok(scenario)
}

fn read_text(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text = read_text(path)?;
    serde_json::from_str(&text).with_context(|| format!("parsing JSON {}", path.display()))
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path
        .parent()
        .context("output must have a parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let mut bytes = serde_json::to_vec_pretty(value).context("serializing canonical JSON")?;
    bytes.push(b'\n');
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

fn validation_error(errors: &[chiyoda_core::ValidationError]) -> anyhow::Error {
    anyhow::anyhow!(
        "scenario is invalid:\n{}",
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn benchmark_error(errors: &[chiyoda_core::BenchmarkValidationError]) -> anyhow::Error {
    anyhow::anyhow!(
        "benchmark manifest is invalid:\n{}",
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn evidence_error(errors: &[chiyoda_core::EvidenceValidationError]) -> anyhow::Error {
    anyhow::anyhow!(
        "evidence catalog is invalid:\n{}",
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn experiment_error(errors: &[chiyoda_core::ExperimentValidationError]) -> anyhow::Error {
    anyhow::anyhow!(
        "experiment manifest is invalid:\n{}",
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn dataset_role(partition: EvidencePartition) -> chiyoda_core::benchmark::DatasetRole {
    match partition {
        EvidencePartition::Calibration => chiyoda_core::benchmark::DatasetRole::Calibration,
        EvidencePartition::HeldOut => chiyoda_core::benchmark::DatasetRole::HeldOut,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExperimentCommand, InformationSamplingAlignment, LayoutCommand, QueueExperience, SweepRun,
        SweepSource, SweepSummary, compare_sweep_summaries, describe_sweep, handle_experiment,
        handle_layout, information_sampling_alignment, require_zero_reference_clearance,
        validate_bundle_metrics, validate_queue_service_reservation_events,
    };
    use chiyoda_core::{
        InformationDeliveryMetrics, InformationInterventionKind, MovementMetrics,
        OnSurfaceClearanceMetrics, QueueMetrics, QueueResourceBreakdown, QueueResourceMetrics,
        RunBundle, RunOptions, SensitivityManifest, SweptOnSurfaceClearanceMetrics, generator,
        parse, plan_sensitivity, run,
    };
    use sha2::{Digest, Sha256};
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TestDirectory(PathBuf);

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn test_directory(name: &str) -> TestDirectory {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after the Unix epoch")
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("chiyoda-{name}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&directory).expect("creating temporary test directory");
        TestDirectory(directory)
    }

    fn test_queue_metrics(
        lift: (u32, f64, u32),
        connector: (u32, f64, u32),
        gate: (u32, f64, u32),
        exit: (u32, f64, u32),
    ) -> QueueMetrics {
        let resource =
            |(ever_queued_agents, cumulative_wait_agent_seconds, peak_waiting_agents)| {
                QueueResourceMetrics {
                    ever_queued_agents,
                    cumulative_wait_agent_seconds,
                    peak_waiting_agents,
                }
            };
        QueueMetrics {
            lift: resource(lift),
            connector: resource(connector),
            gate: resource(gate),
            exit: resource(exit),
            by_resource: Some(QueueResourceBreakdown {
                lifts: if lift.0 > 0 {
                    BTreeMap::from([("fixture_lift".to_owned(), resource(lift))])
                } else {
                    BTreeMap::new()
                },
                connectors: if connector.0 > 0 {
                    BTreeMap::from([("fixture_connector".to_owned(), resource(connector))])
                } else {
                    BTreeMap::new()
                },
                gates: if gate.0 > 0 {
                    BTreeMap::from([("fixture_gate".to_owned(), resource(gate))])
                } else {
                    BTreeMap::new()
                },
                exits: if exit.0 > 0 {
                    BTreeMap::from([("fixture_exit".to_owned(), resource(exit))])
                } else {
                    BTreeMap::new()
                },
            }),
        }
    }

    fn test_movement_metrics(
        agents_with_local_clearance_adjustments: u32,
        local_clearance_adjustment_steps: u64,
        local_avoidance_constraint_fallback_steps: Option<u64>,
        cumulative_local_clearance_adjustment_m: f64,
        maximum_local_clearance_adjustment_m: f64,
    ) -> MovementMetrics {
        MovementMetrics {
            agents_with_local_clearance_adjustments,
            local_clearance_adjustment_steps,
            local_avoidance_constraint_fallback_steps,
            on_surface_clearance_audit: None,
            swept_on_surface_clearance_audit: None,
            cumulative_local_clearance_adjustment_m,
            maximum_local_clearance_adjustment_m,
        }
    }

    #[test]
    fn resource_queue_deltas_mark_resources_added_or_removed_by_an_arm() {
        let telemetry = |ever_queued_agents, cumulative_wait_agent_seconds, peak_waiting_agents| {
            QueueResourceMetrics {
                ever_queued_agents,
                cumulative_wait_agent_seconds,
                peak_waiting_agents,
            }
        };
        let baseline = BTreeMap::from([("east_gate".to_owned(), telemetry(1, 2.0, 1))]);
        let candidate = BTreeMap::from([("west_gate".to_owned(), telemetry(3, 5.0, 2))]);
        let mut deltas = BTreeMap::new();
        let mut peaks = BTreeMap::new();

        super::accumulate_attributed_queue_resource_delta(
            &mut deltas,
            &mut peaks,
            &baseline,
            &candidate,
        );
        super::finalize_attributed_queue_resource_delta(&mut deltas, &peaks);

        let east = deltas
            .get("east_gate")
            .expect("removed resource is reported");
        assert!(east.baseline_resource_declared);
        assert!(!east.candidate_resource_declared);
        assert_eq!(east.ever_queued_agents, -1);
        assert!((east.cumulative_wait_agent_seconds + 2.0).abs() < f64::EPSILON);
        assert_eq!(east.maximum_peak_waiting_agents, -1);
        let west = deltas.get("west_gate").expect("added resource is reported");
        assert!(!west.baseline_resource_declared);
        assert!(west.candidate_resource_declared);
        assert_eq!(west.ever_queued_agents, 3);
        assert!((west.cumulative_wait_agent_seconds - 5.0).abs() < f64::EPSILON);
        assert_eq!(west.maximum_peak_waiting_agents, 2);
    }

    #[test]
    fn layout_osm_writes_a_content_locked_source_observation_report() {
        let directory = test_directory("layout-osm");
        let data_root = directory.0.join("raw");
        fs::create_dir_all(&data_root).expect("creating layout data root");
        let source = br#"<osm version="0.6">
  <node id="1" lat="1.3" lon="103.8"><tag k="entrance" v="yes"/></node>
  <node id="2" lat="1.3001" lon="103.8001"/>
  <way id="9"><nd ref="1"/><nd ref="2"/><tag k="highway" v="steps"/></way>
</osm>"#;
        let source_path = data_root.join("station.osm");
        fs::write(&source_path, source).expect("writing OSM source");
        let catalog_path = directory.0.join("catalog.json");
        let catalog = serde_json::json!({
            "schema_version": "0.1",
            "purpose": "uncalibrated_reference",
            "dataset_id": "cli-osm-fixture",
            "title": "CLI OSM fixture",
            "landing_page": "https://www.openstreetmap.org/",
            "license": "ODbL-1.0",
            "redistributable": true,
            "attribution": "© OpenStreetMap contributors",
            "citation": "OpenStreetMap contributors",
            "files": [{
                "id": "station",
                "source_url": "https://example.test/station.osm",
                "local_path": "station.osm",
                "sha256": format!("{:x}", Sha256::digest(source)),
                "size_bytes": source.len(),
                "transformation": "inspect geographic map observations only"
            }],
            "supported_primitives": "mapped tags only",
            "exclusions": "scenario geometry and operational claims"
        });
        fs::write(
            &catalog_path,
            serde_json::to_vec_pretty(&catalog).expect("serializing catalog"),
        )
        .expect("writing catalog");
        let output = directory.0.join("layout.json");

        handle_layout(LayoutCommand::Osm {
            catalog: catalog_path.clone(),
            data_root: data_root.clone(),
            max_nodes: 2,
            max_ways: 1,
            output: output.clone(),
        })
        .expect("layout command succeeds");
        handle_layout(LayoutCommand::VerifyOsm {
            catalog: catalog_path.clone(),
            report: output.clone(),
            data_root: data_root.clone(),
        })
        .expect("layout verification command succeeds");
        let projection = directory.0.join("projection.json");
        handle_layout(LayoutCommand::ProjectOsm {
            catalog: catalog_path.clone(),
            report: output.clone(),
            data_root: data_root.clone(),
            origin_latitude: 1.3,
            origin_longitude: 103.8,
            output: projection.clone(),
        })
        .expect("projection command succeeds");
        handle_layout(LayoutCommand::VerifyProjection {
            catalog: catalog_path,
            report: output.clone(),
            projection: projection.clone(),
            data_root,
        })
        .expect("projection verification command succeeds");

        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(output).expect("reading observation report"))
                .expect("parsing observation report");
        assert_eq!(report["status"], "source_observation_only");
        assert_eq!(report["counts"]["selected_node_features"], 1);
        assert_eq!(
            report["features"][0]["categories"],
            serde_json::json!(["entrance"])
        );
        assert_eq!(report["schema_version"], "0.2");
        assert_eq!(
            report["features"][1]["geometry"]["kind"],
            "way_node_sequence"
        );
        assert_eq!(report["features"][1]["geometry"]["nodes"][0]["node_id"], 1);
        let projection: serde_json::Value =
            serde_json::from_slice(&fs::read(projection).expect("reading projection report"))
                .expect("parsing projection report");
        assert_eq!(projection["status"], "source_projection_only");
        assert_eq!(
            projection["coordinate_reference"]["origin"]["latitude"],
            1.3
        );
        assert_eq!(
            projection["features"][0]["geometry"]["coordinate"]["east_m"],
            0.0
        );
        assert_eq!(
            projection["features"][1]["geometry"]["kind"],
            "way_node_sequence"
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)] // one end-to-end fixture creates and reconstructs the complete OSM anchor chain
    fn layout_anchor_osm_reconstructs_exact_scenario_point_provenance() {
        let directory = test_directory("layout-anchor-osm");
        let data_root = directory.0.join("raw");
        fs::create_dir_all(&data_root).expect("creating layout data root");
        let source = br#"<osm version="0.6">
  <node id="1" lat="1.3" lon="103.8"><tag k="entrance" v="yes"/></node>
  <node id="2" lat="1.3001" lon="103.8001"/>
  <way id="9"><nd ref="1"/><nd ref="2"/><tag k="highway" v="steps"/></way>
</osm>"#;
        let source_path = data_root.join("station.osm");
        fs::write(&source_path, source).expect("writing OSM source");
        let catalog_path = directory.0.join("catalog.json");
        let catalog = serde_json::json!({
            "schema_version": "0.1",
            "purpose": "uncalibrated_reference",
            "dataset_id": "cli-anchor-osm-fixture",
            "title": "CLI OSM anchor fixture",
            "landing_page": "https://www.openstreetmap.org/",
            "license": "ODbL-1.0",
            "redistributable": true,
            "attribution": "© OpenStreetMap contributors",
            "citation": "OpenStreetMap contributors",
            "files": [{
                "id": "station",
                "source_url": "https://example.test/station.osm",
                "local_path": "station.osm",
                "sha256": format!("{:x}", Sha256::digest(source)),
                "size_bytes": source.len(),
                "transformation": "inspect geographic map observations only"
            }],
            "supported_primitives": "mapped tags only",
            "exclusions": "scenario geometry and operational claims"
        });
        fs::write(
            &catalog_path,
            serde_json::to_vec_pretty(&catalog).expect("serializing catalog"),
        )
        .expect("writing catalog");
        let observation_path = directory.0.join("observation.json");
        handle_layout(LayoutCommand::Osm {
            catalog: catalog_path.clone(),
            data_root: data_root.clone(),
            max_nodes: 2,
            max_ways: 1,
            output: observation_path.clone(),
        })
        .expect("creating OSM observation report");
        let projection_path = directory.0.join("projection.json");
        handle_layout(LayoutCommand::ProjectOsm {
            catalog: catalog_path.clone(),
            report: observation_path.clone(),
            data_root: data_root.clone(),
            origin_latitude: 1.3,
            origin_longitude: 103.8,
            output: projection_path.clone(),
        })
        .expect("creating local projection report");
        let scenario_path = directory.0.join("scenario.chy");
        let scenario = r#"
scenario "source-anchored CLI fixture"
seed 1
duration 5s
timestep 1s
surface concourse at (-1m, -1m, 0m) size (10m, 10m)
exit main_entrance on concourse at (0m, 0m, 0m) width 1m capacity 1/s
agents passengers count 1 on concourse at (1m, 1m, 0m) to main_entrance speed 1m/s radius 0.2m height 1.7m
"#;
        fs::write(&scenario_path, scenario).expect("writing scenario source");
        let manifest_path = directory.0.join("anchors.json");
        let manifest = serde_json::json!({
            "schema_version": "0.1",
            "name": "CLI OSM anchor fixture",
            "description": "one point anchored without importing geometry",
            "scenario_source": "scenario.chy",
            "anchors": [{
                "id": "main_entrance",
                "target": {"kind": "exit", "id": "main_entrance"},
                "source": {
                    "kind": "node_feature",
                    "object_id": 1,
                    "category": "entrance"
                },
                "rationale": "the point preserves source provenance only"
            }],
            "claim_boundary": "the map does not establish geometry or operations"
        });
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).expect("serializing anchor manifest"),
        )
        .expect("writing anchor manifest");
        let anchor_path = directory.0.join("anchor-report.json");
        handle_layout(LayoutCommand::AnchorOsm {
            catalog: catalog_path.clone(),
            observation: observation_path.clone(),
            projection: projection_path.clone(),
            manifest: manifest_path.clone(),
            data_root: data_root.clone(),
            output: anchor_path.clone(),
        })
        .expect("creating anchor report");
        handle_layout(LayoutCommand::VerifyAnchorOsm {
            catalog: catalog_path,
            observation: observation_path,
            projection: projection_path,
            manifest: manifest_path,
            anchor_report: anchor_path.clone(),
            data_root,
        })
        .expect("anchor report verifies");
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(&anchor_path).expect("reading anchor report"))
                .expect("parsing anchor report");
        assert_eq!(report["status"], "source_anchored_scenario_only");
        assert_eq!(report["anchors"][0]["source_coordinate"]["east_m"], 0.0);
        assert_eq!(report["anchors"][0]["scenario_coordinate"]["north_m"], 0.0);
    }

    #[test]
    #[allow(clippy::too_many_lines)] // one starter fixture checks creation, planning, workload, and refusal boundaries
    fn experiment_init_creates_a_no_data_best_guess_starter() {
        let directory = test_directory("experiment-init");
        let output = directory.0.join("starter");
        handle_experiment(ExperimentCommand::Init {
            name: "generated structural draft".to_owned(),
            seed: 73,
            output: output.clone(),
            trace_every: 4,
            with_sensitivity: true,
            sensitivity_runs: Some(3),
        })
        .expect("starter creation succeeds without sources or data");

        let files = fs::read_dir(&output)
            .expect("reading starter directory")
            .map(|entry| {
                entry
                    .expect("reading starter entry")
                    .file_name()
                    .into_string()
                    .expect("starter entry name is UTF-8")
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            files,
            BTreeSet::from([
                "experiment.json".to_owned(),
                "scenario.chy".to_owned(),
                "sensitivity.json".to_owned(),
            ])
        );
        assert_eq!(
            fs::read_to_string(output.join("scenario.chy")).expect("reading starter scenario"),
            generator::source(73)
        );
        let manifest: serde_json::Value =
            super::read_json(&output.join("experiment.json")).expect("reading starter manifest");
        assert_eq!(manifest["schema_version"], "0.4");
        assert_eq!(manifest["name"], "generated structural draft");
        assert_eq!(manifest["trace_every_steps"], 4);
        assert!(manifest.get("sources").is_none());
        assert_eq!(manifest["assumptions"].as_array().map(Vec::len), Some(3));
        assert_eq!(
            manifest["assumptions"][1]["targets"]
                .as_array()
                .map(Vec::len),
            Some(5)
        );
        assert_eq!(
            manifest["sensitivity_studies"],
            serde_json::json!([{
                "id": "generated_best_guess_stress_test",
                "manifest_path": "sensitivity.json"
            }])
        );
        assert!(manifest.get("source_attestations").is_none());

        let plan_output = directory.0.join("plan.json");
        handle_experiment(ExperimentCommand::Plan {
            manifest: output.join("experiment.json"),
            output: Some(plan_output.clone()),
        })
        .expect("starter plan succeeds without data");
        let plan: serde_json::Value = super::read_json(&plan_output).expect("reading starter plan");
        assert_eq!(plan["experiment_name"], "generated structural draft");
        assert_eq!(plan["sources"], serde_json::json!([]));
        assert_eq!(plan["execution"]["integration_steps"], 1200);
        assert_eq!(plan["execution"]["stored_trace_frames"], 301);
        assert_eq!(plan["schema_version"], "0.3");
        assert_eq!(
            plan["resolved_assumption_targets"].as_array().map(Vec::len),
            Some(12)
        );
        assert_eq!(
            plan["resolved_assumption_targets"][0]["target"],
            "agent_count"
        );
        assert_eq!(
            plan["sensitivity_coverage"]["studies"][0]["condition_count"],
            12
        );
        assert_eq!(
            plan["sensitivity_coverage"]["assumption_targets"]
                .as_array()
                .map(Vec::len),
            Some(12)
        );
        let sensitivity_plan_output = directory.0.join("sensitivity-plan.json");
        super::sensitivity_plan(
            &output.join("sensitivity.json"),
            Some(&sensitivity_plan_output),
        )
        .expect("starter sensitivity plan succeeds without data");
        let sensitivity_plan: serde_json::Value =
            super::read_json(&sensitivity_plan_output).expect("reading starter sensitivity plan");
        assert_eq!(sensitivity_plan["execution"]["baseline_runs"], 3);
        assert_eq!(sensitivity_plan["execution"]["condition_count"], 12);
        assert_eq!(sensitivity_plan["execution"]["condition_runs"], 36);
        assert_eq!(sensitivity_plan["execution"]["total_runs"], 39);
        assert_eq!(
            sensitivity_plan["execution"]["integration_steps_per_run"],
            1200
        );
        assert_eq!(
            sensitivity_plan["execution"]["stored_trace_frames_per_run"],
            301
        );
        assert_eq!(
            sensitivity_plan["execution"]["total_integration_steps"],
            46_800
        );
        assert_eq!(
            sensitivity_plan["execution"]["total_stored_trace_frames"],
            11_739
        );
        let sensitivity: SensitivityManifest = super::read_json(&output.join("sensitivity.json"))
            .expect("reading starter sensitivity manifest");
        let starter_scenario = super::read_scenario(&output.join("scenario.chy"))
            .expect("reading starter sensitivity scenario");
        let study = plan_sensitivity(&sensitivity, &starter_scenario)
            .expect("starter sensitivity manifest resolves without data");
        assert_eq!(sensitivity.count, 3);
        assert_eq!(study.conditions.len(), 12);
        assert!(sensitivity.factors.iter().any(|factor| {
            factor.id == "correction_time"
                && factor.target == chiyoda_core::SensitivityTarget::CountermeasureAtS
        }));

        let no_sensitivity_output = directory.0.join("no-sensitivity-starter");
        handle_experiment(ExperimentCommand::Init {
            name: "single draft".to_owned(),
            seed: 74,
            output: no_sensitivity_output.clone(),
            trace_every: 10,
            with_sensitivity: false,
            sensitivity_runs: None,
        })
        .expect("plain starter creation succeeds");
        assert!(!no_sensitivity_output.join("sensitivity.json").exists());

        let zero_run_output = directory.0.join("zero-run-starter");
        let error = handle_experiment(ExperimentCommand::Init {
            name: "invalid sensitivity draft".to_owned(),
            seed: 1,
            output: zero_run_output.clone(),
            trace_every: 10,
            with_sensitivity: true,
            sensitivity_runs: Some(0),
        })
        .expect_err("a starter sensitivity study must have at least one replication");
        assert!(format!("{error:#}").contains("count must be greater than zero"));
        assert!(!zero_run_output.exists());

        let error = handle_experiment(ExperimentCommand::Init {
            name: "second draft".to_owned(),
            seed: 74,
            output,
            trace_every: 10,
            with_sensitivity: false,
            sensitivity_runs: None,
        })
        .expect_err("starter creation must not overwrite an existing draft");
        assert!(format!("{error:#}").contains("must be empty"));
    }

    #[test]
    fn experiment_resolves_typed_assumption_targets_into_plan_and_artifact() {
        let directory = test_directory("experiment-assumption-targets");
        fs::write(
            directory.0.join("scenario.chy"),
            r#"
scenario "typed-assumption-targets"
seed 1
duration 5s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (8m, 1m, 0m) width 1m capacity 1/s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1.2m/s radius 0.3m height 1.7m
"#,
        )
        .expect("writing scenario source");
        let manifest_path = directory.0.join("experiment.json");
        let manifest = serde_json::json!({
            "schema_version": "0.3",
            "name": "typed assumption fixture",
            "description": "one no-data structural run with resolved input links",
            "scenario_source": "scenario.chy",
            "trace_every_steps": 1,
            "assumptions": [{
                "id": "passenger_motion_and_exit_service",
                "subject": "passenger speed and exit service are explicit inputs",
                "basis": "best_guess",
                "rationale": "these inputs remain uncalibrated but their exact baseline values must be reviewable",
                "targets": [
                    {"target": "agent_speed_mps", "subject": "passengers"},
                    {"target": "exit_capacity_per_s", "subject": "street"}
                ]
            }],
            "claim_boundary": "not predictive, operational, or safety guidance"
        });
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).expect("serializing experiment manifest"),
        )
        .expect("writing experiment manifest");

        let plan_path = directory.0.join("plan.json");
        handle_experiment(ExperimentCommand::Plan {
            manifest: manifest_path.clone(),
            output: Some(plan_path.clone()),
        })
        .expect("typed-assumption plan succeeds");
        let plan: serde_json::Value = super::read_json(&plan_path).expect("reading plan");
        assert_eq!(plan["schema_version"], "0.3");
        assert_eq!(
            plan["resolved_assumption_targets"],
            serde_json::json!([
                {
                    "assumption_id": "passenger_motion_and_exit_service",
                    "target": "agent_speed_mps",
                    "subject": "passengers",
                    "baseline_value": 1.2,
                    "unit": "m/s"
                },
                {
                    "assumption_id": "passenger_motion_and_exit_service",
                    "target": "exit_capacity_per_s",
                    "subject": "street",
                    "baseline_value": 1.0,
                    "unit": "/s"
                }
            ])
        );

        let artifact = directory.0.join("artifact");
        handle_experiment(ExperimentCommand::Run {
            manifest: manifest_path,
            output: artifact.clone(),
        })
        .expect("typed-assumption experiment succeeds");
        handle_experiment(ExperimentCommand::Verify {
            directory: artifact.clone(),
        })
        .expect("typed-assumption artifact verifies");
        let report: serde_json::Value =
            super::read_json(&artifact.join("report.json")).expect("reading report");
        assert_eq!(report["schema_version"], "0.3");
        assert_eq!(
            report["resolved_assumption_targets"],
            plan["resolved_assumption_targets"]
        );

        let mut altered = report.clone();
        altered["resolved_assumption_targets"][0]["baseline_value"] = serde_json::json!(9.9);
        super::write_json(&artifact.join("report.json"), &altered)
            .expect("altering resolved assumption target");
        let error = handle_experiment(ExperimentCommand::Verify {
            directory: artifact,
        })
        .expect_err("altered resolved assumption target must not verify");
        assert!(format!("{error:#}").contains("persisted experiment report"));
    }

    #[test]
    #[allow(clippy::too_many_lines)] // one fixture exercises coverage, snapshot independence, and tampering boundaries
    fn experiment_snapshots_linked_sensitivity_coverage_and_reconstructs_it() {
        let directory = test_directory("experiment-sensitivity-coverage");
        fs::write(
            directory.0.join("scenario.chy"),
            r#"
scenario "sensitivity-coverage"
seed 1
duration 5s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (8m, 1m, 0m) width 1m capacity 1/s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1.2m/s radius 0.3m height 1.7m
"#,
        )
        .expect("writing scenario source");
        fs::write(
            directory.0.join("sensitivity.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": "0.1",
                "name": "passenger walking-speed alternatives",
                "description": "declared alternatives for one uncalibrated input",
                "baseline_source": "scenario.chy",
                "first_seed": 1,
                "count": 1,
                "factors": [{
                    "id": "passenger_speed",
                    "target": "agent_speed_mps",
                    "subject": "passengers",
                    "values": [1.0, 1.2, 1.4],
                    "basis": "best_guess",
                    "rationale": "these values expose a stated speed input without estimating a population distribution"
                }],
                "claim_boundary": "not predictive, operational, or safety guidance"
            }))
            .expect("serializing sensitivity manifest"),
        )
        .expect("writing sensitivity manifest");
        let manifest_path = directory.0.join("experiment.json");
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": "0.4",
                "name": "sensitivity coverage fixture",
                "description": "one run with a linked sensitivity contract",
                "scenario_source": "scenario.chy",
                "trace_every_steps": 1,
                "assumptions": [{
                    "id": "passenger_motion_and_exit_service",
                    "subject": "passenger speed and exit service are explicit inputs",
                    "basis": "best_guess",
                    "rationale": "both baselines must remain reviewable even when only speed is varied",
                    "targets": [
                        {"target": "agent_speed_mps", "subject": "passengers"},
                        {"target": "exit_capacity_per_s", "subject": "street"}
                    ]
                }],
                "sensitivity_studies": [{
                    "id": "walking_speed",
                    "manifest_path": "sensitivity.json"
                }],
                "claim_boundary": "not predictive, operational, or safety guidance"
            }))
            .expect("serializing experiment manifest"),
        )
        .expect("writing experiment manifest");

        let plan_path = directory.0.join("plan.json");
        handle_experiment(ExperimentCommand::Plan {
            manifest: manifest_path.clone(),
            output: Some(plan_path.clone()),
        })
        .expect("sensitivity coverage plan succeeds");
        let plan: serde_json::Value = super::read_json(&plan_path).expect("reading plan");
        assert_eq!(plan["schema_version"], "0.3");
        assert_eq!(
            plan["sensitivity_coverage"]["studies"][0]["study_id"],
            "walking_speed"
        );
        assert_eq!(
            plan["sensitivity_coverage"]["studies"][0]["baseline_scenario_hash"],
            plan["scenario_hash"]
        );
        assert_eq!(
            plan["sensitivity_coverage"]["assumption_targets"],
            serde_json::json!([
                {
                    "assumption_id": "passenger_motion_and_exit_service",
                    "target": "agent_speed_mps",
                    "subject": "passengers",
                    "baseline_value": 1.2,
                    "unit": "m/s",
                    "sensitivity_factors": [{
                        "study_id": "walking_speed",
                        "factor_id": "passenger_speed"
                    }]
                },
                {
                    "assumption_id": "passenger_motion_and_exit_service",
                    "target": "exit_capacity_per_s",
                    "subject": "street",
                    "baseline_value": 1.0,
                    "unit": "/s",
                    "sensitivity_factors": []
                }
            ])
        );

        fs::write(
            directory.0.join("untracked-sensitivity.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": "0.1",
                "name": "undeclared body-radius alternatives",
                "description": "a factor without a matching experiment assumption",
                "baseline_source": "scenario.chy",
                "first_seed": 1,
                "count": 1,
                "factors": [{
                    "id": "passenger_radius",
                    "target": "agent_radius_m",
                    "subject": "passengers",
                    "values": [0.2, 0.3, 0.4],
                    "basis": "best_guess",
                    "rationale": "this fixture must be rejected until its baseline is disclosed by the experiment"
                }],
                "claim_boundary": "not predictive, operational, or safety guidance"
            }))
            .expect("serializing untracked sensitivity manifest"),
        )
        .expect("writing untracked sensitivity manifest");
        let mut invalid_manifest: serde_json::Value =
            super::read_json(&manifest_path).expect("reading experiment manifest");
        invalid_manifest["sensitivity_studies"][0]["manifest_path"] =
            serde_json::json!("untracked-sensitivity.json");
        super::write_json(&manifest_path, &invalid_manifest)
            .expect("writing untracked sensitivity link");
        let error = handle_experiment(ExperimentCommand::Plan {
            manifest: manifest_path.clone(),
            output: None,
        })
        .expect_err("an untracked sensitivity factor must be rejected");
        assert!(format!("{error:#}").contains("not declared by an experiment assumption"));
        invalid_manifest["sensitivity_studies"][0]["manifest_path"] =
            serde_json::json!("sensitivity.json");
        super::write_json(&manifest_path, &invalid_manifest)
            .expect("restoring valid sensitivity link");

        let artifact = directory.0.join("artifact");
        handle_experiment(ExperimentCommand::Run {
            manifest: manifest_path.clone(),
            output: artifact.clone(),
        })
        .expect("experiment with sensitivity coverage succeeds");
        assert!(
            artifact
                .join("sensitivity-studies/walking_speed/manifest.json")
                .is_file()
        );
        assert!(
            artifact
                .join("sensitivity-studies/walking_speed/baseline.chy")
                .is_file()
        );
        handle_experiment(ExperimentCommand::Verify {
            directory: artifact.clone(),
        })
        .expect("sensitivity coverage artifact verifies");
        let report: serde_json::Value =
            super::read_json(&artifact.join("report.json")).expect("reading report");
        assert_eq!(report["schema_version"], "0.4");
        assert_eq!(report["sensitivity_coverage"], plan["sensitivity_coverage"]);

        fs::write(
            directory.0.join("sensitivity.json"),
            b"not the captured contract",
        )
        .expect("changing external sensitivity manifest after artifact creation");
        handle_experiment(ExperimentCommand::Verify {
            directory: artifact.clone(),
        })
        .expect("artifact verification uses its sensitivity-study snapshot");

        let snapshot = artifact.join("sensitivity-studies/walking_speed/manifest.json");
        let mut altered: serde_json::Value =
            super::read_json(&snapshot).expect("reading sensitivity-study snapshot");
        altered["factors"][0]["values"][0] = serde_json::json!(0.9);
        super::write_json(&snapshot, &altered).expect("altering sensitivity-study snapshot");
        let error = handle_experiment(ExperimentCommand::Verify {
            directory: artifact,
        })
        .expect_err("altered sensitivity-study snapshot must not verify");
        assert!(format!("{error:#}").contains("persisted experiment report"));
    }

    #[test]
    #[allow(clippy::too_many_lines)] // one artifact fixture exercises every independent verification boundary
    fn experiment_snapshots_assumptions_and_source_reports_then_detects_tampering() {
        let directory = test_directory("experiment");
        fs::write(
            directory.0.join("scenario.chy"),
            r#"
scenario "experiment-fixture"
seed 1
duration 5s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (8m, 1m, 0m) width 1m capacity 1/s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1.2m/s radius 0.3m height 1.7m
"#,
        )
        .expect("writing scenario source");
        let source_report = b"{\"source\":\"fixture\"}\n";
        fs::write(directory.0.join("reference.json"), source_report)
            .expect("writing source report");
        let manifest_path = directory.0.join("experiment.json");
        let manifest = serde_json::json!({
            "schema_version": "0.1",
            "name": "CLI experiment fixture",
            "description": "one explicit uncalibrated structural run",
            "scenario_source": "scenario.chy",
            "trace_every_steps": 1,
            "assumptions": [{
                "id": "exit_capacity",
                "subject": "street.capacity",
                "basis": "documented_estimate",
                "rationale": "keep the chosen input and its source boundary visible",
                "source_ids": ["fixture_reference"]
            }],
            "sources": [{
                "id": "fixture_reference",
                "citation": "Fixture (2026)",
                "url": "https://example.test/reference",
                "applicability": "source report is disclosed context only",
                "limitation": "does not calibrate the runtime",
                "derived_report": {
                    "path": "reference.json",
                    "sha256": format!("{:x}", Sha256::digest(source_report))
                }
            }],
            "claim_boundary": "not predictive, operational, or safety guidance"
        });
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).expect("serializing experiment manifest"),
        )
        .expect("writing experiment manifest");
        let output = directory.0.join("artifact");
        let plan_output = directory.0.join("plan.json");

        handle_experiment(ExperimentCommand::Plan {
            manifest: manifest_path.clone(),
            output: Some(plan_output.clone()),
        })
        .expect("experiment plan succeeds");
        assert!(
            !output.exists(),
            "planning must not create the execution output directory"
        );
        let plan: serde_json::Value =
            super::read_json(&plan_output).expect("parsing experiment plan");
        assert_eq!(plan["schema_version"], "0.3");
        assert_eq!(plan["scenario"]["declared_agents"], 1);
        assert_eq!(
            plan["source_report_snapshots"][0]["source_id"],
            "fixture_reference"
        );

        handle_experiment(ExperimentCommand::Run {
            manifest: manifest_path,
            output: output.clone(),
        })
        .expect("experiment run succeeds");
        handle_experiment(ExperimentCommand::Verify {
            directory: output.clone(),
        })
        .expect("experiment artifact verifies");
        let report: serde_json::Value = serde_json::from_slice(
            &fs::read(output.join("report.json")).expect("reading experiment report"),
        )
        .expect("parsing experiment report");
        assert_eq!(
            report["source_report_snapshots"][0]["source_id"],
            "fixture_reference"
        );
        assert_eq!(report["schema_version"], "0.3");
        assert_eq!(report["runtime_metrics"]["total_agents"], 1);

        let mut altered_metrics = report.clone();
        altered_metrics["runtime_metrics"]["evacuated_agents"] = serde_json::json!(99);
        super::write_json(&output.join("report.json"), &altered_metrics)
            .expect("altering mirrored runtime metrics");
        let error = handle_experiment(ExperimentCommand::Verify {
            directory: output.clone(),
        })
        .expect_err("altered mirrored metrics must not verify");
        assert!(format!("{error:#}").contains("persisted experiment report"));
        super::write_json(&output.join("report.json"), &report).expect("restoring current report");

        let mut legacy_report = report.clone();
        legacy_report
            .as_object_mut()
            .expect("experiment report is an object")
            .remove("runtime_metrics");
        legacy_report["schema_version"] = serde_json::json!("0.1");
        super::write_json(&output.join("report.json"), &legacy_report)
            .expect("writing legacy report");
        handle_experiment(ExperimentCommand::Verify {
            directory: output.clone(),
        })
        .expect("legacy experiment artifact verifies");
        super::write_json(&output.join("report.json"), &report).expect("restoring current report");

        let run_path = output.join("run.json");
        let original_bundle = fs::read(&run_path).expect("reading original run bundle");
        let mut fabricated_bundle: RunBundle =
            super::read_json(&run_path).expect("parsing original run bundle");
        fabricated_bundle.metrics.queued_for_exit_agents = 99;
        fabricated_bundle.bundle_hash = chiyoda_core::bundle::bundle_hash(&fabricated_bundle);
        super::write_json(&run_path, &fabricated_bundle)
            .expect("writing self-hashed fabricated run bundle");
        let error = handle_experiment(ExperimentCommand::Verify {
            directory: output.clone(),
        })
        .expect_err("self-hashed fabricated bundle must not verify");
        assert!(format!("{error:#}").contains("deterministic reconstruction"));
        fs::write(&run_path, original_bundle).expect("restoring original run bundle");

        fs::write(
            output.join("source-reports/fixture_reference.json"),
            b"{\"source\":\"altered\"}\n",
        )
        .expect("altering source report snapshot");
        let error = handle_experiment(ExperimentCommand::Verify { directory: output })
            .expect_err("altered source report must not verify");
        assert!(format!("{error:#}").contains("source report snapshot hash"));
    }

    #[test]
    #[allow(clippy::too_many_lines)] // the fixture verifies source creation and artifact replay boundaries together
    fn experiment_osm_attestation_rechecks_raw_source_then_preserves_provenance() {
        let directory = test_directory("experiment-osm-attestation");
        let data_root = directory.0.join("raw");
        fs::create_dir_all(&data_root).expect("creating OSM data root");
        let source = br#"<osm version="0.6">
  <node id="1" lat="1.3" lon="103.8"><tag k="entrance" v="yes"/></node>
  <node id="2" lat="1.3001" lon="103.8001"/>
  <way id="9"><nd ref="1"/><nd ref="2"/><tag k="highway" v="steps"/></way>
</osm>"#;
        let source_path = data_root.join("station.osm");
        fs::write(&source_path, source).expect("writing OSM source");
        let catalog_path = directory.0.join("catalog.json");
        let catalog = serde_json::json!({
            "schema_version": "0.1",
            "purpose": "uncalibrated_reference",
            "dataset_id": "attested-osm-fixture",
            "title": "Attested OSM fixture",
            "landing_page": "https://www.openstreetmap.org/",
            "license": "ODbL-1.0",
            "redistributable": true,
            "attribution": "© OpenStreetMap contributors",
            "citation": "OpenStreetMap contributors",
            "files": [{
                "id": "station",
                "source_url": "https://example.test/station.osm",
                "local_path": "station.osm",
                "sha256": format!("{:x}", Sha256::digest(source)),
                "size_bytes": source.len(),
                "transformation": "inspect geographic map observations only"
            }],
            "supported_primitives": "mapped tags only",
            "exclusions": "scenario geometry and operational claims"
        });
        fs::write(
            &catalog_path,
            serde_json::to_vec_pretty(&catalog).expect("serializing catalog"),
        )
        .expect("writing catalog");
        let observation_path = directory.0.join("observation.json");
        handle_layout(LayoutCommand::Osm {
            catalog: catalog_path.clone(),
            data_root: data_root.clone(),
            max_nodes: 2,
            max_ways: 1,
            output: observation_path.clone(),
        })
        .expect("creating OSM observation report");
        let projection_path = directory.0.join("projection.json");
        handle_layout(LayoutCommand::ProjectOsm {
            catalog: catalog_path.clone(),
            report: observation_path.clone(),
            data_root: data_root.clone(),
            origin_latitude: 1.3,
            origin_longitude: 103.8,
            output: projection_path.clone(),
        })
        .expect("creating local OSM projection report");

        fs::write(
            directory.0.join("scenario.chy"),
            r#"
scenario "attested OSM experiment"
seed 1
duration 5s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (0m, 0m, 0m) width 1m capacity 1/s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1.2m/s radius 0.3m height 1.7m
"#,
        )
        .expect("writing scenario source");
        let anchor_manifest_path = directory.0.join("anchors.json");
        let anchor_manifest = serde_json::json!({
            "schema_version": "0.1",
            "name": "Attested OSM experiment fixture anchor",
            "description": "one point anchored without importing layout geometry",
            "scenario_source": "scenario.chy",
            "anchors": [{
                "id": "street",
                "target": {"kind": "exit", "id": "street"},
                "source": {
                    "kind": "node_feature",
                    "object_id": 1,
                    "category": "entrance"
                },
                "rationale": "retain the reviewed source point only"
            }],
            "claim_boundary": "the map does not establish a facility layout or operations"
        });
        fs::write(
            &anchor_manifest_path,
            serde_json::to_vec_pretty(&anchor_manifest).expect("serializing anchor manifest"),
        )
        .expect("writing anchor manifest");
        let anchor_report_path = directory.0.join("anchor-report.json");
        handle_layout(LayoutCommand::AnchorOsm {
            catalog: catalog_path.clone(),
            observation: observation_path.clone(),
            projection: projection_path.clone(),
            manifest: anchor_manifest_path.clone(),
            data_root: data_root.clone(),
            output: anchor_report_path.clone(),
        })
        .expect("creating OSM scenario-anchor report");
        let projection = fs::read(&projection_path).expect("reading projection report");
        let anchor_report = fs::read(&anchor_report_path).expect("reading anchor report");
        let manifest_path = directory.0.join("experiment.json");
        let manifest = serde_json::json!({
            "schema_version": "0.2",
            "name": "Attested OSM experiment fixture",
            "description": "an uncalibrated structural run with source provenance",
            "scenario_source": "scenario.chy",
            "trace_every_steps": 1,
            "assumptions": [{
                "id": "exit_capacity",
                "subject": "street.capacity",
                "basis": "documented_estimate",
                "rationale": "keep the chosen input and OSM source boundary visible",
                "source_ids": ["station_anchor"]
            }],
            "sources": [{
                "id": "station_projection",
                "citation": "OpenStreetMap contributors",
                "url": "https://www.openstreetmap.org/",
                "applicability": "map observation informs context only",
                "limitation": "does not calibrate the runtime or author scenario geometry",
                "source_sha256": format!("{:x}", Sha256::digest(source)),
                "derived_report": {
                    "path": "projection.json",
                    "sha256": format!("{:x}", Sha256::digest(&projection))
                }
            }, {
                "id": "station_anchor",
                "citation": "OpenStreetMap contributors",
                "url": "https://www.openstreetmap.org/",
                "applicability": "one selected source point is retained for manual scenario authoring",
                "limitation": "does not import layout geometry, calibrate the runtime, or validate a facility",
                "source_sha256": format!("{:x}", Sha256::digest(source)),
                "derived_report": {
                    "path": "anchor-report.json",
                    "sha256": format!("{:x}", Sha256::digest(&anchor_report))
                }
            }],
            "source_attestations": [
                {
                    "kind": "osm_local_projection",
                    "source_id": "station_projection",
                    "catalog_path": "catalog.json",
                    "data_root": "raw",
                    "observation_report_path": "observation.json"
                },
                {
                    "kind": "osm_scenario_anchor",
                    "source_id": "station_anchor",
                    "projection_source_id": "station_projection",
                    "anchor_manifest_path": "anchors.json"
                }
            ],
            "claim_boundary": "not predictive, operational, or safety guidance"
        });
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).expect("serializing experiment manifest"),
        )
        .expect("writing experiment manifest");
        let output = directory.0.join("artifact");

        fs::write(&source_path, b"<osm version=\"0.6\"/>").expect("tampering raw OSM source");
        let error = handle_experiment(ExperimentCommand::Plan {
            manifest: manifest_path.clone(),
            output: Some(directory.0.join("plan.json")),
        })
        .expect_err("changed raw OSM source must block experiment planning");
        assert!(format!("{error:#}").contains("OSM source attestation"));
        let error = handle_experiment(ExperimentCommand::Run {
            manifest: manifest_path.clone(),
            output: output.clone(),
        })
        .expect_err("changed raw OSM source must block artifact creation");
        assert!(format!("{error:#}").contains("OSM source attestation"));
        fs::write(&source_path, source).expect("restoring raw OSM source");

        let original_anchor_manifest =
            fs::read(&anchor_manifest_path).expect("reading original anchor manifest");
        let mut mismatched_anchor_manifest: serde_json::Value =
            serde_json::from_slice(&original_anchor_manifest)
                .expect("parsing original anchor manifest");
        mismatched_anchor_manifest["scenario_source"] = serde_json::json!("other-scenario.chy");
        super::write_json(&anchor_manifest_path, &mismatched_anchor_manifest)
            .expect("writing mismatched anchor manifest");
        fs::write(
            directory.0.join("other-scenario.chy"),
            "different scenario source\n",
        )
        .expect("writing mismatched scenario source");
        let error = handle_experiment(ExperimentCommand::Plan {
            manifest: manifest_path.clone(),
            output: Some(directory.0.join("plan.json")),
        })
        .expect_err("an anchor manifest for a different scenario must block planning");
        assert!(
            format!("{error:#}").contains("does not resolve to the experiment scenario source")
        );
        fs::write(&anchor_manifest_path, original_anchor_manifest)
            .expect("restoring anchor manifest");

        let plan_output = directory.0.join("plan.json");
        handle_experiment(ExperimentCommand::Plan {
            manifest: manifest_path.clone(),
            output: Some(plan_output.clone()),
        })
        .expect("attested experiment plan succeeds");
        let plan: serde_json::Value =
            super::read_json(&plan_output).expect("parsing experiment plan");
        assert_eq!(
            plan["verified_osm_source_attestations"],
            serde_json::json!(["station_projection", "station_anchor"])
        );

        handle_experiment(ExperimentCommand::Run {
            manifest: manifest_path,
            output: output.clone(),
        })
        .expect("attested experiment run succeeds");
        handle_experiment(ExperimentCommand::Verify {
            directory: output.clone(),
        })
        .expect("attested experiment artifact verifies");
        assert!(
            output
                .join("source-attestations/station_projection/catalog.json")
                .is_file()
        );
        assert!(
            output
                .join("source-attestations/station_projection/observation.json")
                .is_file()
        );
        assert!(
            output
                .join("source-attestations/station_anchor/anchor-manifest.json")
                .is_file()
        );

        let anchor_snapshot =
            fs::read(output.join("source-attestations/station_anchor/anchor-manifest.json"))
                .expect("reading anchor manifest snapshot");
        let mut altered_anchor: serde_json::Value =
            serde_json::from_slice(&anchor_snapshot).expect("parsing anchor manifest snapshot");
        altered_anchor["description"] = serde_json::json!("altered anchor manifest");
        super::write_json(
            &output.join("source-attestations/station_anchor/anchor-manifest.json"),
            &altered_anchor,
        )
        .expect("tampering anchor manifest snapshot");
        let error = handle_experiment(ExperimentCommand::Verify {
            directory: output.clone(),
        })
        .expect_err("changed OSM anchor manifest snapshot must not verify");
        assert!(format!("{error:#}").contains("scenario-anchor"));
        fs::write(
            output.join("source-attestations/station_anchor/anchor-manifest.json"),
            anchor_snapshot,
        )
        .expect("restoring anchor manifest snapshot");

        let mut altered_catalog = catalog;
        altered_catalog["title"] = serde_json::json!("Altered OSM fixture");
        super::write_json(
            &output.join("source-attestations/station_projection/catalog.json"),
            &altered_catalog,
        )
        .expect("tampering catalog snapshot");
        let error = handle_experiment(ExperimentCommand::Verify { directory: output })
            .expect_err("changed OSM catalog snapshot must not verify");
        assert!(format!("{error:#}").contains("source metadata"));
    }

    #[test]
    #[allow(clippy::too_many_lines)] // the fixture retains all legacy-coverage counters together
    fn sweep_analysis_keeps_counts_exact_and_labels_legacy_attribution() {
        let summary = SweepSummary {
            schema_version: "0.1".to_owned(),
            generator_version: "0.15".to_owned(),
            source: SweepSource::Generated,
            first_seed: 10,
            count: 3,
            trace_every_steps: 10,
            runs: vec![
                SweepRun {
                    seed: 10,
                    scenario_name: "one".to_owned(),
                    bundle_hash: "a".repeat(64),
                    bundle_version: None,
                    runtime_version: None,
                    total_agents: 10,
                    evacuated_agents: 8,
                    evacuated_by_exit: BTreeMap::from([
                        ("east".to_owned(), 2),
                        ("west".to_owned(), 6),
                    ]),
                    remaining_by_state: BTreeMap::from([("moving".to_owned(), 2)]),
                    information_delivery: BTreeMap::new(),
                    queue_experience: None,
                    queue_metrics: None,
                    movement_metrics: None,
                    clearance_time_s: Some(2.0),
                    last_exit_time_s: None,
                },
                SweepRun {
                    seed: 11,
                    scenario_name: "two".to_owned(),
                    bundle_hash: "b".repeat(64),
                    bundle_version: None,
                    runtime_version: None,
                    total_agents: 5,
                    evacuated_agents: 0,
                    evacuated_by_exit: BTreeMap::new(),
                    remaining_by_state: BTreeMap::from([("waiting_for_route".to_owned(), 5)]),
                    information_delivery: BTreeMap::new(),
                    queue_experience: None,
                    queue_metrics: None,
                    movement_metrics: None,
                    clearance_time_s: None,
                    last_exit_time_s: None,
                },
                SweepRun {
                    seed: 12,
                    scenario_name: "three".to_owned(),
                    bundle_hash: "c".repeat(64),
                    bundle_version: None,
                    runtime_version: None,
                    total_agents: 5,
                    evacuated_agents: 5,
                    evacuated_by_exit: BTreeMap::from([("east".to_owned(), 5)]),
                    remaining_by_state: BTreeMap::new(),
                    information_delivery: BTreeMap::new(),
                    queue_experience: None,
                    queue_metrics: None,
                    movement_metrics: None,
                    clearance_time_s: Some(4.0),
                    last_exit_time_s: None,
                },
            ],
        };

        let analysis = describe_sweep(&summary);

        assert_eq!(analysis.run_count, 3);
        assert_eq!(analysis.total_agents, 20);
        assert_eq!(analysis.evacuated_agents, 13);
        assert_eq!(analysis.un_evacuated_agents, 7);
        assert_eq!(analysis.overall_evacuation_fraction.numerator, 13);
        assert_eq!(analysis.overall_evacuation_fraction.denominator, 20);
        assert_eq!(analysis.runs_with_any_evacuation, 2);
        assert_eq!(analysis.fully_evacuated_runs, 1);
        assert_eq!(analysis.evacuated_by_exit.get("east"), Some(&7));
        assert_eq!(analysis.evacuated_by_exit.get("west"), Some(&6));
        assert_eq!(analysis.unattributed_evacuations, 0);
        assert_eq!(analysis.remaining_by_state.get("moving"), Some(&2));
        assert_eq!(
            analysis.remaining_by_state.get("waiting_for_route"),
            Some(&5)
        );
        assert_eq!(analysis.unattributed_remaining_agents, 0);
        assert_eq!(analysis.queue_experience.observed_runs, 0);
        assert_eq!(analysis.queue_experience.unobserved_legacy_runs, 3);
        assert_eq!(analysis.queue_experience.queued_for_lift_agents, 0);
        assert_eq!(analysis.queue_experience.queued_for_connector_agents, 0);
        assert_eq!(analysis.queue_experience.queued_for_gate_agents, 0);
        assert_eq!(analysis.queue_experience.queued_for_exit_agents, 0);
        assert_eq!(analysis.queue_telemetry.observed_runs, 0);
        assert_eq!(analysis.queue_telemetry.unobserved_legacy_runs, 3);
        assert_eq!(analysis.movement_telemetry.observed_runs, 0);
        assert_eq!(analysis.movement_telemetry.unobserved_legacy_runs, 3);
        let clearance_time = analysis
            .clearance_time_s
            .expect("one run reached full clearance");
        assert_eq!(clearance_time.measured_runs, 1);
        assert!((clearance_time.minimum_s - 4.0).abs() < f64::EPSILON);
        assert!((clearance_time.mean_s - 4.0).abs() < f64::EPSILON);
        assert!((clearance_time.maximum_s - 4.0).abs() < f64::EPSILON);
        let last_exit_time = analysis
            .last_exit_time_s
            .expect("legacy recorded exits remain descriptively available");
        assert_eq!(last_exit_time.measured_runs, 2);
        assert!((last_exit_time.minimum_s - 2.0).abs() < f64::EPSILON);
        assert!((last_exit_time.maximum_s - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn legacy_sweep_summaries_default_to_generated_provenance() {
        let summary: SweepSummary = serde_json::from_str(
            r#"{
                "schema_version": "0.1",
                "generator_version": "0.15",
                "first_seed": 73,
                "count": 0,
                "trace_every_steps": 10,
                "runs": []
            }"#,
        )
        .expect("legacy summary deserializes");

        assert!(matches!(summary.source, SweepSource::Generated));
        let encoded = serde_json::to_value(summary).expect("summary serializes");
        assert!(encoded.get("source").is_none());
    }

    #[test]
    fn comparison_reports_shared_and_arm_specific_sampling_keys() {
        let baseline = parse(
            r#"
scenario "baseline"
seed 1
duration 10s
timestep 1s
message baseline_notice source signage on concourse at (1m, 1m, 0m) claim exit street closed truth false time 1s reach 2m trust 0.5 sample matched
message baseline_only source signage on concourse at (1m, 1m, 0m) claim exit street closed truth false time 2s reach 2m trust 0.5
"#,
        )
        .expect("baseline parses");
        let candidate = parse(
            r#"
scenario "candidate"
seed 1
duration 10s
timestep 1s
message candidate_notice source signage on concourse at (1m, 1m, 0m) claim exit street closed truth false time 1s reach 2m trust 0.5 sample matched
message candidate_only source signage on concourse at (1m, 1m, 0m) claim exit street closed truth false time 2s reach 2m trust 0.5
"#,
        )
        .expect("candidate parses");

        let alignment = information_sampling_alignment(&baseline, &candidate);

        assert_eq!(
            alignment.shared["matched"].baseline.intervention,
            "baseline_notice"
        );
        assert_eq!(
            alignment.shared["matched"].candidate.intervention,
            "candidate_notice"
        );
        assert!(alignment.baseline_only.contains_key("baseline_only"));
        assert!(alignment.candidate_only.contains_key("candidate_only"));
    }

    #[test]
    #[allow(clippy::too_many_lines)] // this fixture exercises every persisted sensitivity arm
    fn sensitivity_writes_hash_verifiable_condition_and_comparison_artifacts() {
        let directory = test_directory("sensitivity");
        let source_path = directory.0.join("baseline.chy");
        fs::write(
            &source_path,
            r#"
scenario "sensitivity-cli"
seed 1
duration 2s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (8m, 1m, 0m) width 2m capacity 1/s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#,
        )
        .expect("writing baseline source");
        let reference_report_path = directory.0.join("reference.json");
        let reference_report = b"{\"fixture\":true}\n";
        fs::write(&reference_report_path, reference_report).expect("writing reference report");
        let manifest_path = directory.0.join("study.json");
        fs::write(
            &manifest_path,
            r#"{
  "schema_version": "0.1",
  "name": "cli fixture",
  "description": "exercise persisted study artifacts",
  "baseline_source": "baseline.chy",
  "first_seed": 10,
  "count": 1,
  "design": "one_at_a_time",
  "max_conditions": 4,
  "factors": [{
    "id": "street_capacity",
    "target": "exit_capacity_per_s",
    "subject": "street",
    "values": [1.0, 2.0],
    "basis": "best_guess",
    "rationale": "exercise an authored alternative",
    "references": [{
      "id": "fixture_reference",
      "citation": "Fixture source (2026)",
      "url": "https://example.test/reference",
      "applicability": "fixture provenance coverage",
      "limitation": "not a runtime calibration",
      "derived_report": {
        "path": "reference.json",
        "sha256": "218589323cbe80b7ed077e3ee36f1663e7cb5f8f4e4ad02c938ad8a5c2c5a6b9"
      }
    }]
  }, {
    "id": "walking_speed",
    "target": "agent_speed_mps",
    "subject": "passengers",
    "values": [1.0, 1.5],
    "basis": "best_guess",
    "rationale": "exercise a planned agent declaration change"
  }, {
    "id": "passenger_count",
    "target": "agent_count",
    "subject": "passengers",
    "values": [1.0, 2.0],
    "basis": "best_guess",
    "rationale": "exercise a planned demand change"
  }],
  "claim_boundary": "structural fixture only"
}"#,
        )
        .expect("writing sensitivity manifest");
        let output = directory.0.join("output");
        let plan_path = directory.0.join("plan.json");

        super::sensitivity_plan(&manifest_path, Some(&plan_path))
            .expect("sensitivity plan succeeds");

        let plan: serde_json::Value = super::read_json(&plan_path).expect("plan parses");
        assert_eq!(plan["conditions"].as_array().map(Vec::len), Some(3));
        assert_eq!(
            plan["reference_report_snapshots"][0]["snapshot_path"],
            "reference-reports/street_capacity/fixture_reference.json"
        );
        assert!(
            !output.exists(),
            "planning must not create the execution output directory"
        );

        super::run_sensitivity(&manifest_path, &output).expect("sensitivity study succeeds");

        let report: serde_json::Value =
            super::read_json(&output.join("report.json")).expect("report parses");
        assert_eq!(report["conditions"].as_array().map(Vec::len), Some(3));
        assert_eq!(
            report["conditions"][0]["factor_values"]["street_capacity"],
            2.0
        );
        assert_eq!(
            report["one_at_a_time_responses"].as_array().map(Vec::len),
            Some(3)
        );
        assert_eq!(
            report["reference_report_snapshots"][0]["source_path"],
            "reference.json"
        );
        let reference_snapshot_path =
            output.join("reference-reports/street_capacity/fixture_reference.json");
        assert_eq!(
            fs::read(&reference_snapshot_path).expect("reference snapshot reads"),
            reference_report
        );
        assert_eq!(
            report["one_at_a_time_responses"][0]["alternatives"][0]["value"],
            2.0
        );
        let comparison_path = output.join("comparisons/case-0001.json");
        let comparison: serde_json::Value =
            super::read_json(&comparison_path).expect("comparison parses");
        assert_eq!(comparison["paired_runs"].as_array().map(Vec::len), Some(1));
        assert!(comparison["aggregate"]["candidate_minus_baseline"]["queue_telemetry"].is_object());
        assert!(
            comparison["aggregate"]["candidate_minus_baseline"]["queue_telemetry"]
                ["by_resource"]["exits"]["street"]
                .is_object(),
            "current comparisons retain resource-attributed queue deltas"
        );
        assert!(report["conditions"][0]["outcome"]["queue_telemetry_delta"].is_object());
        let agent_comparison: serde_json::Value =
            super::read_json(&output.join("comparisons/case-0002.json"))
                .expect("agent comparison parses");
        assert_eq!(
            agent_comparison["pairing"]["agent_declarations_matched"],
            false
        );
        assert_eq!(
            agent_comparison["paired_runs"][0]["baseline_total_agents"],
            1
        );
        assert_eq!(
            agent_comparison["paired_runs"][0]["candidate_total_agents"],
            1
        );
        let demand_comparison: serde_json::Value =
            super::read_json(&output.join("comparisons/case-0003.json"))
                .expect("demand comparison parses");
        assert_eq!(
            demand_comparison["pairing"]["agent_declarations_matched"],
            false
        );
        assert_eq!(
            demand_comparison["paired_runs"][0]["baseline_total_agents"],
            1
        );
        assert_eq!(
            demand_comparison["paired_runs"][0]["candidate_total_agents"],
            2
        );
        let baseline_summary_path = output.join("baseline/summary.json");
        let baseline_summary: serde_json::Value =
            super::read_json(&baseline_summary_path).expect("baseline summary parses");
        assert!(
            baseline_summary["runs"][0]["queue_experience"].is_object(),
            "current sweeps persist queue experience"
        );
        assert!(
            baseline_summary["runs"][0]["queue_metrics"].is_object(),
            "current sweeps persist detailed queue telemetry"
        );
        assert!(
            baseline_summary["runs"][0]["queue_metrics"]["by_resource"]["exits"]["street"]
                .is_object(),
            "current sweeps attribute queue telemetry to constrained resources"
        );
        assert!(
            baseline_summary["runs"][0]["movement_metrics"].is_object(),
            "current sweeps persist local-clearance telemetry"
        );
        assert!(
            baseline_summary["runs"][0]["movement_metrics"]["on_surface_clearance_audit"]
                .is_object(),
            "current sweeps persist the integration-boundary reference-disc audit"
        );
        assert!(
            baseline_summary["runs"][0]["movement_metrics"]["swept_on_surface_clearance_audit"]
                .is_object(),
            "current sweeps persist the analytic interval reference-disc audit"
        );
        assert!(
            comparison["aggregate"]["candidate_minus_baseline"]["movement_telemetry"].is_object(),
            "current comparisons retain local-clearance telemetry deltas"
        );
        assert!(
            comparison["aggregate"]["candidate_minus_baseline"]["movement_telemetry"]
                ["swept_on_surface_clearance_audit"]
                .is_object(),
            "current comparisons retain analytic interval clearance-audit deltas"
        );
        assert!(
            report["conditions"][0]["outcome"]["movement_telemetry_delta"].is_object(),
            "current sensitivity reports retain local-clearance telemetry deltas"
        );
        assert!(
            report["conditions"][0]["outcome"]["movement_telemetry_delta"]
                ["swept_on_surface_clearance_audit"]
                .is_object(),
            "current sensitivity reports retain analytic interval clearance-audit deltas"
        );
        let baseline_summary_for_analysis: SweepSummary =
            super::read_json(&baseline_summary_path).expect("baseline summary deserializes");
        let baseline_analysis = describe_sweep(&baseline_summary_for_analysis);
        assert_eq!(baseline_analysis.queue_telemetry.observed_runs, 1);
        assert_eq!(baseline_analysis.queue_telemetry.unobserved_legacy_runs, 0);
        assert_eq!(
            baseline_analysis
                .movement_telemetry
                .swept_on_surface_clearance_audit_observed_runs,
            1
        );
        assert_eq!(
            baseline_analysis
                .movement_telemetry
                .swept_on_surface_clearance_audit_unobserved_legacy_runs,
            0
        );
        assert_eq!(
            baseline_analysis.queue_telemetry.by_resource.observed_runs,
            1
        );
        assert_eq!(
            baseline_analysis
                .queue_telemetry
                .by_resource
                .unobserved_legacy_runs,
            0
        );
        assert!(
            baseline_analysis
                .queue_telemetry
                .by_resource
                .exits
                .contains_key("street")
        );
        super::verify_sweep(&output.join("baseline")).expect("baseline verifies");
        super::verify_sweep(&output.join("conditions/case-0001")).expect("condition verifies");
        super::verify_sensitivity(&output).expect("sensitivity study verifies");

        let original_summary = fs::read(&baseline_summary_path).expect("reading baseline summary");
        let mut altered_summary = baseline_summary.clone();
        altered_summary["runs"][0]["queue_experience"]["queued_for_exit_agents"] =
            serde_json::Value::from(99);
        super::write_json(&baseline_summary_path, &altered_summary)
            .expect("altering queue-experience summary");
        let queue_error = super::verify_sweep(&output.join("baseline"))
            .expect_err("altered queue experience must not verify");
        assert!(
            format!("{queue_error:#}").contains("summary queue experience"),
            "unexpected queue-experience verification error: {queue_error:#}"
        );
        fs::write(&baseline_summary_path, original_summary).expect("restoring baseline summary");

        let original_summary = fs::read(&baseline_summary_path).expect("reading baseline summary");
        let mut altered_summary: serde_json::Value =
            super::read_json(&baseline_summary_path).expect("baseline summary parses");
        altered_summary["runs"][0]["movement_metrics"]["local_clearance_adjustment_steps"] =
            serde_json::Value::from(99);
        super::write_json(&baseline_summary_path, &altered_summary)
            .expect("altering local-clearance telemetry summary");
        let movement_telemetry_error = super::verify_sweep(&output.join("baseline"))
            .expect_err("altered local-clearance telemetry must not verify");
        assert!(
            format!("{movement_telemetry_error:#}").contains("summary local-clearance telemetry"),
            "unexpected local-clearance telemetry verification error: {movement_telemetry_error:#}"
        );
        fs::write(&baseline_summary_path, original_summary).expect("restoring baseline summary");

        let original_summary = fs::read(&baseline_summary_path).expect("reading baseline summary");
        let mut altered_summary: serde_json::Value =
            super::read_json(&baseline_summary_path).expect("baseline summary parses");
        altered_summary["runs"][0]["queue_metrics"]["gate"]["cumulative_wait_agent_seconds"] =
            serde_json::Value::from(99.0);
        super::write_json(&baseline_summary_path, &altered_summary)
            .expect("altering queue telemetry summary");
        let queue_telemetry_error = super::verify_sweep(&output.join("baseline"))
            .expect_err("altered queue telemetry must not verify");
        assert!(
            format!("{queue_telemetry_error:#}").contains("summary queue telemetry"),
            "unexpected queue-telemetry verification error: {queue_telemetry_error:#}"
        );
        fs::write(&baseline_summary_path, original_summary).expect("restoring baseline summary");

        let original_summary = fs::read(&baseline_summary_path).expect("reading baseline summary");
        let mut altered_summary: serde_json::Value =
            super::read_json(&baseline_summary_path).expect("baseline summary parses");
        altered_summary["runs"][0]["queue_metrics"]["by_resource"]["exits"]["street"]["peak_waiting_agents"] =
            serde_json::Value::from(99);
        super::write_json(&baseline_summary_path, &altered_summary)
            .expect("altering resource queue telemetry summary");
        let resource_queue_telemetry_error = super::verify_sweep(&output.join("baseline"))
            .expect_err("altered resource queue telemetry must not verify");
        assert!(
            format!("{resource_queue_telemetry_error:#}").contains("summary queue telemetry"),
            "unexpected resource queue-telemetry verification error: {resource_queue_telemetry_error:#}"
        );
        fs::write(&baseline_summary_path, original_summary).expect("restoring baseline summary");

        let baseline_run_path = output.join("baseline/seed-10/run.json");
        let original_bundle = fs::read(&baseline_run_path).expect("reading baseline bundle");
        let mut fabricated_bundle: RunBundle =
            super::read_json(&baseline_run_path).expect("parsing baseline bundle");
        fabricated_bundle.metrics.queued_for_exit_agents = 99;
        fabricated_bundle.bundle_hash = chiyoda_core::bundle::bundle_hash(&fabricated_bundle);
        super::write_json(&baseline_run_path, &fabricated_bundle)
            .expect("writing self-hashed fabricated baseline bundle");
        let reconstruction_error = super::verify_sweep(&output.join("baseline"))
            .expect_err("self-hashed fabricated baseline bundle must not verify");
        assert!(
            format!("{reconstruction_error:#}").contains("deterministic reconstruction"),
            "unexpected reconstruction-verification error: {reconstruction_error:#}"
        );
        fs::write(&baseline_run_path, original_bundle).expect("restoring baseline bundle");

        let mut altered_comparison = comparison.clone();
        altered_comparison["paired_runs"] = serde_json::Value::Array(Vec::new());
        super::write_json(&comparison_path, &altered_comparison).expect("altering comparison");
        let comparison_error =
            super::verify_sensitivity(&output).expect_err("altered comparison must not verify");
        assert!(
            format!("{comparison_error:#}").contains("persisted sensitivity comparison"),
            "unexpected comparison-verification error: {comparison_error:#}"
        );
        super::write_json(&comparison_path, &comparison).expect("restoring comparison");

        fs::write(&reference_snapshot_path, b"{\"fixture\":false}\n")
            .expect("altering reference snapshot");
        let reference_error = super::verify_sensitivity(&output)
            .expect_err("altered reference snapshot must not verify");
        assert!(
            format!("{reference_error:#}").contains("reference snapshot hash"),
            "unexpected reference-snapshot verification error: {reference_error:#}"
        );
        fs::write(&reference_snapshot_path, reference_report)
            .expect("restoring reference snapshot");

        let mut altered_report = report.clone();
        altered_report["study_name"] = serde_json::Value::String("tampered".to_owned());
        super::write_json(&output.join("report.json"), &altered_report).expect("altering report");
        let report_error =
            super::verify_sensitivity(&output).expect_err("altered report must not verify");
        assert!(
            format!("{report_error:#}").contains("persisted sensitivity report"),
            "unexpected report-verification error: {report_error:#}"
        );
    }

    #[test]
    fn sweep_analysis_discloses_legacy_bundles_without_exit_attribution() {
        let summary = SweepSummary {
            schema_version: "0.1".to_owned(),
            generator_version: "0.14".to_owned(),
            source: SweepSource::Generated,
            first_seed: 73,
            count: 1,
            trace_every_steps: 10,
            runs: vec![SweepRun {
                seed: 73,
                scenario_name: "legacy".to_owned(),
                bundle_hash: "a".repeat(64),
                bundle_version: None,
                runtime_version: None,
                total_agents: 4,
                evacuated_agents: 3,
                evacuated_by_exit: BTreeMap::new(),
                remaining_by_state: BTreeMap::new(),
                information_delivery: BTreeMap::new(),
                queue_experience: None,
                queue_metrics: None,
                movement_metrics: None,
                clearance_time_s: Some(8.0),
                last_exit_time_s: None,
            }],
        };

        let analysis = describe_sweep(&summary);

        assert_eq!(analysis.evacuated_by_exit, BTreeMap::new());
        assert_eq!(analysis.unattributed_evacuations, 3);
        assert_eq!(analysis.remaining_by_state, BTreeMap::new());
        assert_eq!(analysis.unattributed_remaining_agents, 1);
    }

    #[test]
    #[allow(clippy::too_many_lines)] // the fixture documents two complete comparison arms
    fn sweep_comparison_preserves_seed_paired_deltas_and_completion_boundaries() {
        let baseline = SweepSummary {
            schema_version: "0.1".to_owned(),
            generator_version: "authored-template".to_owned(),
            source: SweepSource::Authored {
                template_scenario_hash: "a".repeat(64),
            },
            first_seed: 100,
            count: 2,
            trace_every_steps: 10,
            runs: vec![
                SweepRun {
                    seed: 100,
                    scenario_name: "baseline".to_owned(),
                    bundle_hash: "b".repeat(64),
                    bundle_version: Some("0.19".to_owned()),
                    runtime_version: Some("deterministic-euler-0.19".to_owned()),
                    total_agents: 10,
                    evacuated_agents: 10,
                    evacuated_by_exit: BTreeMap::from([("east".to_owned(), 10)]),
                    remaining_by_state: BTreeMap::new(),
                    information_delivery: BTreeMap::from([(
                        "notice".to_owned(),
                        InformationDeliveryMetrics {
                            kind: InformationInterventionKind::Message,
                            received_agents: 4,
                            accepted_agents: 2,
                        },
                    )]),
                    queue_experience: Some(QueueExperience {
                        queued_for_lift_agents: 1,
                        queued_for_connector_agents: 0,
                        queued_for_gate_agents: 0,
                        queued_for_exit_agents: 0,
                    }),
                    queue_metrics: Some(test_queue_metrics(
                        (1, 2.0, 1),
                        (0, 0.0, 0),
                        (0, 0.0, 0),
                        (0, 0.0, 0),
                    )),
                    movement_metrics: Some(test_movement_metrics(1, 2, Some(2), 0.8, 0.5)),
                    clearance_time_s: Some(10.0),
                    last_exit_time_s: Some(10.0),
                },
                SweepRun {
                    seed: 101,
                    scenario_name: "baseline".to_owned(),
                    bundle_hash: "c".repeat(64),
                    bundle_version: Some("0.19".to_owned()),
                    runtime_version: Some("deterministic-euler-0.19".to_owned()),
                    total_agents: 10,
                    evacuated_agents: 10,
                    evacuated_by_exit: BTreeMap::from([("west".to_owned(), 10)]),
                    remaining_by_state: BTreeMap::new(),
                    information_delivery: BTreeMap::from([(
                        "notice".to_owned(),
                        InformationDeliveryMetrics {
                            kind: InformationInterventionKind::Message,
                            received_agents: 1,
                            accepted_agents: 0,
                        },
                    )]),
                    queue_experience: Some(QueueExperience {
                        queued_for_lift_agents: 0,
                        queued_for_connector_agents: 0,
                        queued_for_gate_agents: 1,
                        queued_for_exit_agents: 0,
                    }),
                    queue_metrics: Some(test_queue_metrics(
                        (0, 0.0, 0),
                        (0, 0.0, 0),
                        (1, 1.0, 1),
                        (0, 0.0, 0),
                    )),
                    movement_metrics: Some(test_movement_metrics(0, 0, Some(0), 0.0, 0.0)),
                    clearance_time_s: Some(20.0),
                    last_exit_time_s: Some(20.0),
                },
            ],
        };
        let candidate = SweepSummary {
            schema_version: "0.1".to_owned(),
            generator_version: "authored-template".to_owned(),
            source: SweepSource::Authored {
                template_scenario_hash: "d".repeat(64),
            },
            first_seed: 100,
            count: 2,
            trace_every_steps: 10,
            runs: vec![
                SweepRun {
                    seed: 100,
                    scenario_name: "candidate".to_owned(),
                    bundle_hash: "e".repeat(64),
                    bundle_version: Some("0.19".to_owned()),
                    runtime_version: Some("deterministic-euler-0.19".to_owned()),
                    total_agents: 10,
                    evacuated_agents: 10,
                    evacuated_by_exit: BTreeMap::from([
                        ("east".to_owned(), 6),
                        ("west".to_owned(), 4),
                    ]),
                    remaining_by_state: BTreeMap::new(),
                    information_delivery: BTreeMap::from([(
                        "notice".to_owned(),
                        InformationDeliveryMetrics {
                            kind: InformationInterventionKind::Message,
                            received_agents: 4,
                            accepted_agents: 3,
                        },
                    )]),
                    queue_experience: Some(QueueExperience {
                        queued_for_lift_agents: 2,
                        queued_for_connector_agents: 0,
                        queued_for_gate_agents: 0,
                        queued_for_exit_agents: 0,
                    }),
                    queue_metrics: Some(test_queue_metrics(
                        (2, 5.0, 2),
                        (0, 0.0, 0),
                        (0, 0.0, 0),
                        (0, 0.0, 0),
                    )),
                    movement_metrics: Some(test_movement_metrics(2, 3, Some(3), 1.5, 0.8)),
                    clearance_time_s: Some(9.0),
                    last_exit_time_s: Some(9.0),
                },
                SweepRun {
                    seed: 101,
                    scenario_name: "candidate".to_owned(),
                    bundle_hash: "f".repeat(64),
                    bundle_version: Some("0.19".to_owned()),
                    runtime_version: Some("deterministic-euler-0.19".to_owned()),
                    total_agents: 10,
                    evacuated_agents: 7,
                    evacuated_by_exit: BTreeMap::from([("west".to_owned(), 7)]),
                    remaining_by_state: BTreeMap::from([("waiting_for_exit".to_owned(), 3)]),
                    information_delivery: BTreeMap::from([(
                        "notice".to_owned(),
                        InformationDeliveryMetrics {
                            kind: InformationInterventionKind::Message,
                            received_agents: 2,
                            accepted_agents: 1,
                        },
                    )]),
                    queue_experience: Some(QueueExperience {
                        queued_for_lift_agents: 0,
                        queued_for_connector_agents: 0,
                        queued_for_gate_agents: 3,
                        queued_for_exit_agents: 0,
                    }),
                    queue_metrics: Some(test_queue_metrics(
                        (0, 0.0, 0),
                        (0, 0.0, 0),
                        (3, 4.0, 3),
                        (0, 0.0, 0),
                    )),
                    movement_metrics: Some(test_movement_metrics(1, 1, Some(1), 0.3, 0.3)),
                    clearance_time_s: None,
                    last_exit_time_s: Some(14.0),
                },
            ],
        };

        let comparison = compare_sweep_summaries(
            &baseline,
            &candidate,
            vec!["messages".to_owned(), "countermeasures".to_owned()],
            InformationSamplingAlignment {
                shared: BTreeMap::new(),
                baseline_only: BTreeMap::new(),
                candidate_only: BTreeMap::new(),
            },
        )
        .expect("compatible authored sweeps compare");

        assert_eq!(comparison.pairing.run_count, 2);
        assert_eq!(comparison.pairing.execution_contract.bundle_version, "0.19");
        assert_eq!(
            comparison.pairing.execution_contract.runtime_version,
            "deterministic-euler-0.19"
        );
        assert_eq!(
            comparison
                .baseline
                .movement_telemetry
                .constraint_fallback_observed_runs,
            2
        );
        assert_eq!(
            comparison
                .baseline
                .movement_telemetry
                .local_avoidance_constraint_fallback_steps,
            2
        );
        assert_eq!(
            comparison
                .candidate
                .movement_telemetry
                .local_avoidance_constraint_fallback_steps,
            4
        );
        assert_eq!(
            comparison.paired_runs[0]
                .candidate_minus_baseline
                .evacuated_agents,
            0
        );
        assert_eq!(
            comparison.paired_runs[1]
                .candidate_minus_baseline
                .evacuated_agents,
            -3
        );
        assert_eq!(
            comparison
                .aggregate
                .candidate_minus_baseline
                .evacuated_agents,
            -3
        );
        assert_eq!(
            comparison
                .aggregate
                .candidate_minus_baseline
                .un_evacuated_agents,
            3
        );
        assert_eq!(
            comparison
                .aggregate
                .candidate_minus_baseline
                .evacuated_by_exit
                .get("east"),
            Some(&-4)
        );
        assert_eq!(
            comparison
                .aggregate
                .candidate_minus_baseline
                .evacuated_by_exit
                .get("west"),
            Some(&1)
        );
        assert_eq!(
            comparison
                .aggregate
                .candidate_minus_baseline
                .remaining_by_state
                .get("waiting_for_exit"),
            Some(&3)
        );
        let delivery_delta = comparison
            .aggregate
            .candidate_minus_baseline
            .information_delivery
            .get("notice")
            .expect("information-delivery difference is reported");
        assert_eq!(delivery_delta.received_agents, 1);
        assert_eq!(delivery_delta.accepted_agents, 2);
        let queue_delta = comparison
            .aggregate
            .candidate_minus_baseline
            .queue_experience
            .as_ref()
            .expect("current queue-experience difference is reported");
        assert_eq!(queue_delta.queued_for_lift_agents, 1);
        assert_eq!(queue_delta.queued_for_connector_agents, 0);
        assert_eq!(queue_delta.queued_for_gate_agents, 2);
        assert_eq!(queue_delta.queued_for_exit_agents, 0);
        let queue_telemetry_delta = comparison
            .aggregate
            .candidate_minus_baseline
            .queue_telemetry
            .as_ref()
            .expect("current queue telemetry difference is reported");
        assert_eq!(queue_telemetry_delta.lift.ever_queued_agents, 1);
        assert!(
            (queue_telemetry_delta.lift.cumulative_wait_agent_seconds - 3.0).abs() < f64::EPSILON
        );
        assert_eq!(queue_telemetry_delta.lift.maximum_peak_waiting_agents, 1);
        assert_eq!(queue_telemetry_delta.gate.ever_queued_agents, 2);
        assert!(
            (queue_telemetry_delta.gate.cumulative_wait_agent_seconds - 3.0).abs() < f64::EPSILON
        );
        let movement_telemetry_delta = comparison
            .aggregate
            .candidate_minus_baseline
            .movement_telemetry
            .as_ref()
            .expect("current local-clearance telemetry difference is reported");
        assert_eq!(
            movement_telemetry_delta.agents_with_local_clearance_adjustments,
            2
        );
        assert_eq!(movement_telemetry_delta.local_clearance_adjustment_steps, 2);
        assert_eq!(
            movement_telemetry_delta.local_avoidance_constraint_fallback_steps,
            Some(2)
        );
        assert!(
            (movement_telemetry_delta.cumulative_local_clearance_adjustment_m - 1.0).abs()
                < f64::EPSILON
        );
        assert!(
            (movement_telemetry_delta.maximum_local_clearance_adjustment_m - 0.3).abs()
                < f64::EPSILON
        );
        assert_eq!(queue_telemetry_delta.gate.maximum_peak_waiting_agents, 2);
        let attributed_queue_delta = queue_telemetry_delta
            .by_resource
            .as_ref()
            .expect("resource-attributed queue difference is reported when every run supports it");
        let lift_delta = attributed_queue_delta
            .lifts
            .get("fixture_lift")
            .expect("shared lift attribution is reported");
        assert!(lift_delta.baseline_resource_declared);
        assert!(lift_delta.candidate_resource_declared);
        assert_eq!(lift_delta.ever_queued_agents, 1);
        assert!((lift_delta.cumulative_wait_agent_seconds - 3.0).abs() < f64::EPSILON);
        assert_eq!(lift_delta.maximum_peak_waiting_agents, 1);
        let gate_delta = attributed_queue_delta
            .gates
            .get("fixture_gate")
            .expect("shared gate attribution is reported");
        assert_eq!(gate_delta.ever_queued_agents, 2);
        assert!((gate_delta.cumulative_wait_agent_seconds - 3.0).abs() < f64::EPSILON);
        assert_eq!(gate_delta.maximum_peak_waiting_agents, 2);
        assert_eq!(
            comparison.baseline.information_delivery["notice"].received_agents,
            5
        );
        assert_eq!(
            comparison.candidate.information_delivery["notice"].accepted_agents,
            4
        );
        assert_eq!(comparison.baseline.queue_telemetry.observed_runs, 2);
        assert_eq!(
            comparison
                .baseline
                .queue_telemetry
                .by_resource
                .observed_runs,
            2
        );
        assert_eq!(
            comparison.baseline.queue_telemetry.lift.ever_queued_agents,
            1
        );
        assert!(
            (comparison
                .candidate
                .queue_telemetry
                .gate
                .cumulative_wait_agent_seconds
                - 4.0)
                .abs()
                < f64::EPSILON
        );
        assert_eq!(
            comparison
                .candidate
                .queue_telemetry
                .gate
                .maximum_peak_waiting_agents,
            3
        );
        assert_eq!(comparison.aggregate.runs_with_more_candidate_evacuations, 0);
        assert_eq!(
            comparison.aggregate.runs_with_fewer_candidate_evacuations,
            1
        );
        assert_eq!(comparison.aggregate.runs_with_unchanged_evacuations, 1);
        assert_eq!(comparison.aggregate.clearance_time_s.both_recorded_runs, 1);
        assert_eq!(
            comparison
                .aggregate
                .clearance_time_s
                .baseline_only_recorded_runs,
            1
        );
        assert_eq!(
            comparison.aggregate.clearance_time_s.candidate_earlier_runs,
            1
        );
        assert!(
            (comparison
                .aggregate
                .clearance_time_s
                .candidate_minus_baseline_s
                .expect("one pair completed in both arms")
                .mean_s
                + 1.0)
                .abs()
                < f64::EPSILON
        );
        assert_eq!(comparison.aggregate.last_exit_time_s.both_recorded_runs, 2);
        assert_eq!(
            comparison.aggregate.last_exit_time_s.candidate_earlier_runs,
            2
        );
        assert!(
            (comparison
                .aggregate
                .last_exit_time_s
                .candidate_minus_baseline_s
                .expect("both pairs have observed exits")
                .mean_s
                + 3.5)
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn sweep_comparison_rejects_a_mismatched_seed_range() {
        let summary = SweepSummary {
            schema_version: "0.1".to_owned(),
            generator_version: "authored-template".to_owned(),
            source: SweepSource::Authored {
                template_scenario_hash: "a".repeat(64),
            },
            first_seed: 100,
            count: 1,
            trace_every_steps: 10,
            runs: vec![SweepRun {
                seed: 100,
                scenario_name: "control".to_owned(),
                bundle_hash: "b".repeat(64),
                bundle_version: None,
                runtime_version: None,
                total_agents: 1,
                evacuated_agents: 1,
                evacuated_by_exit: BTreeMap::from([("street".to_owned(), 1)]),
                remaining_by_state: BTreeMap::new(),
                information_delivery: BTreeMap::new(),
                queue_experience: None,
                queue_metrics: None,
                movement_metrics: None,
                clearance_time_s: Some(1.0),
                last_exit_time_s: Some(1.0),
            }],
        };
        let candidate = SweepSummary {
            first_seed: 101,
            ..summary.clone()
        };

        let error = compare_sweep_summaries(
            &summary,
            &candidate,
            Vec::new(),
            InformationSamplingAlignment {
                shared: BTreeMap::new(),
                baseline_only: BTreeMap::new(),
                candidate_only: BTreeMap::new(),
            },
        )
        .expect_err("different seed ranges cannot be paired");

        assert!(
            error
                .to_string()
                .contains("matching contiguous seed ranges")
        );
    }

    #[test]
    fn sweep_comparison_rejects_mismatched_execution_contracts() {
        let baseline = SweepSummary {
            schema_version: "0.1".to_owned(),
            generator_version: "authored-template".to_owned(),
            source: SweepSource::Authored {
                template_scenario_hash: "a".repeat(64),
            },
            first_seed: 100,
            count: 1,
            trace_every_steps: 10,
            runs: vec![SweepRun {
                seed: 100,
                scenario_name: "control".to_owned(),
                bundle_hash: "b".repeat(64),
                bundle_version: Some("0.19".to_owned()),
                runtime_version: Some("deterministic-euler-0.19".to_owned()),
                total_agents: 1,
                evacuated_agents: 1,
                evacuated_by_exit: BTreeMap::from([("street".to_owned(), 1)]),
                remaining_by_state: BTreeMap::new(),
                information_delivery: BTreeMap::new(),
                queue_experience: None,
                queue_metrics: None,
                movement_metrics: None,
                clearance_time_s: Some(1.0),
                last_exit_time_s: Some(1.0),
            }],
        };
        let mut candidate = baseline.clone();
        candidate.runs[0].runtime_version = Some("deterministic-euler-0.20".to_owned());

        let error = compare_sweep_summaries(
            &baseline,
            &candidate,
            Vec::new(),
            InformationSamplingAlignment {
                shared: BTreeMap::new(),
                baseline_only: BTreeMap::new(),
                candidate_only: BTreeMap::new(),
            },
        )
        .expect_err("different execution contracts cannot be paired");

        assert!(
            error
                .to_string()
                .contains("identical bundle and runtime versions")
        );
    }

    #[test]
    fn sweep_verification_rejects_inconsistent_remaining_state_counts() {
        let source = r#"
scenario "invalid-final-state-metrics"
seed 1
duration 1s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (9m, 1m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
        let scenario = parse(source).expect("source parses");
        let mut bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
        bundle
            .metrics
            .remaining_by_state
            .insert("moving".to_owned(), 2);

        let error = validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect_err("inconsistent terminal metrics must be rejected");

        assert!(
            error
                .to_string()
                .contains("remaining-state count does not match")
        );
    }

    #[test]
    fn sweep_verification_rejects_partial_run_labeled_as_clearance() {
        let source = r#"
scenario "partial-clearance"
seed 1
duration 3s
timestep 1s
surface concourse at (0m, 0m, 0m) size (14m, 10m)
exit street on concourse at (4m, 1m, 0m) width 2m
agents quick count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
agents slow count 1 on concourse at (12m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
        let scenario = parse(source).expect("source parses");
        let mut bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
        bundle.metrics.clearance_time_s = bundle.metrics.last_exit_time_s;

        let error = validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect_err("a partial run cannot be labeled as cleared");

        assert!(
            error
                .to_string()
                .contains("clearance time does not match full evacuation")
        );
    }

    #[test]
    fn sweep_verification_rejects_impossible_information_acceptance() {
        let scenario = generator::scenario(73).expect("generated scenario is valid");
        let mut bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
        let delivery = bundle
            .metrics
            .information_delivery
            .get_mut("false_platform_exit")
            .expect("generated message has delivery metrics");
        delivery.accepted_agents = delivery.received_agents + 1;

        let error = validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect_err("acceptance cannot exceed delivery");

        assert!(error.to_string().contains("acceptance exceeds delivery"));
    }

    #[test]
    fn sweep_verification_rejects_queue_telemetry_without_its_entry_audit_event() {
        let source = r#"
scenario "queue-entry-audit"
seed 1
duration 4s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street:west on concourse at (1m, 1m, 0m) width 2m capacity 0.5/s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street:west speed 1m/s radius 0.3m height 1.7m
"#;
        let scenario = parse(source).expect("source parses");
        let mut bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
        validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect("current queue-entry audit verifies");
        bundle
            .events
            .retain(|event| event.kind != "queue_entered_exit");

        let error = validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect_err("missing queue-entry audit event must be rejected");

        assert!(
            error
                .to_string()
                .contains("queue-entry events disagree with `street:west` telemetry")
        );
    }

    #[test]
    fn sweep_verification_rejects_unauthored_queue_service_reservations() {
        let source = r#"
scenario "queue-reservation-audit"
seed 1
duration 45s
timestep 1s
surface concourse at (0m, 0m, 0m) size (25m, 10m)
exit street on concourse at (22m, 5m, 0m) width 2m capacity 0.05/s
queue-footprint street_queue exit street on concourse from (18m, 5m, 0m) to (14m, 5m, 0m) slots 2
agents passengers count 2 on concourse at (20m, 5m, 0m) to street speed 10m/s radius 0.3m height 1.7m
"#;
        let scenario = parse(source).expect("source parses");
        let mut bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
        validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect("current queue-service reservation audit verifies");
        bundle
            .events
            .iter_mut()
            .find(|event| event.kind == "queue_service_reserved")
            .expect("fixture produces a reservation")
            .detail = "exit:forged".to_owned();

        let error = validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect_err("unauthored queue-service reservation must be rejected");

        assert!(error.to_string().contains("unauthored footprint"));
    }

    #[test]
    fn sweep_verification_requires_lift_reservations_to_follow_lift_entries() {
        let source = r#"
scenario "lift-queue-reservation-audit"
seed 1
duration 40s
timestep 0.5s
surface platform at (0m, 0m, 0m) size (25m, 10m)
surface concourse at (0m, 0m, 3m) size (25m, 10m)
exit street on concourse at (15m, 5m, 3m) width 2m
lift lift_a from platform at (10m, 5m, 0m) to concourse at (10m, 5m, 3m) cabin 2m 2m capacity 1 cycle 8s
queue-footprint lift_queue connector lift_a on platform from (12m, 5m, 0m) to (16m, 5m, 0m) slots 3
agents early count 1 on platform at (11m, 5m, 0m) to street speed 1m/s radius 0.3m height 1.7m
agents late count 1 on platform at (14m, 5m, 0m) to street speed 1m/s radius 0.3m height 1.7m release 1s
agents later count 1 on platform at (16m, 5m, 0m) to street speed 1m/s radius 0.3m height 1.7m release 1s
"#;
        let scenario = parse(source).expect("source parses");
        let mut bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
        validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect("lift reservation audit verifies");
        bundle
            .events
            .iter_mut()
            .find(|event| event.kind == "queue_entered_lift")
            .expect("fixture produces a lift queue entry")
            .kind = "queue_entered_connector".to_owned();

        let error = validate_queue_service_reservation_events(&bundle, Path::new("fixture"))
            .expect_err("lift reservation must not accept a connector queue-entry event");

        assert!(
            error
                .to_string()
                .contains("reservation lacks a prior matching queue entry")
        );
    }

    #[test]
    fn sweep_verification_cross_checks_orca_fallback_events() {
        let source = r#"
scenario "orca-fallback-audit"
seed 1
duration 1s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (9m, 1m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
        let scenario = parse(source).expect("source parses");
        let mut bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
        validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect("current bundle with no fallback verifies");

        let agent_id = bundle.trace[0].agents[0].id.clone();
        bundle
            .metrics
            .movement_metrics
            .as_mut()
            .expect("current bundle has local-motion telemetry")
            .local_avoidance_constraint_fallback_steps = Some(1);
        bundle.events.push(chiyoda_core::bundle::RunEvent {
            time_s: 1.0,
            kind: "local_avoidance_constraint_fallback".to_owned(),
            subject: agent_id,
            detail: "the speed-bounded reciprocal constraints were infeasible".to_owned(),
        });
        validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect("matching local-motion fallback event verifies");

        bundle.events.last_mut().expect("event exists").subject = "unknown-agent".to_owned();
        let error = validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect_err("fallback event for an unknown agent must be rejected");
        assert!(error.to_string().contains("names an unknown agent"));
    }

    #[test]
    fn sweep_verification_rejects_inconsistent_on_surface_clearance_audits() {
        let source = r#"
scenario "on-surface-clearance-audit"
seed 1
duration 1s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (9m, 1m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
        let scenario = parse(source).expect("source parses");
        let mut bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
        validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect("current bundle audit verifies");

        bundle
            .metrics
            .movement_metrics
            .as_mut()
            .expect("current bundle has local-motion telemetry")
            .on_surface_clearance_audit = Some(OnSurfaceClearanceMetrics {
            agents_with_disc_overlaps: 1,
            disc_overlap_pair_steps: 1,
            maximum_disc_overlap_m: 0.1,
        });

        let error = validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect_err("one-agent overlap audit must be rejected");
        assert!(
            error
                .to_string()
                .contains("invalid on-surface reference-disc audit telemetry")
        );

        bundle
            .metrics
            .movement_metrics
            .as_mut()
            .expect("current bundle has local-motion telemetry")
            .on_surface_clearance_audit = Some(OnSurfaceClearanceMetrics {
            agents_with_disc_overlaps: 0,
            disc_overlap_pair_steps: 0,
            maximum_disc_overlap_m: 0.0,
        });
        bundle
            .metrics
            .movement_metrics
            .as_mut()
            .expect("current bundle has local-motion telemetry")
            .swept_on_surface_clearance_audit = Some(SweptOnSurfaceClearanceMetrics {
            agents_with_swept_disc_overlaps: 1,
            swept_disc_overlap_pair_steps: 1,
            maximum_swept_disc_overlap_m: 0.1,
        });

        let error = validate_bundle_metrics(&bundle, Path::new("fixture"))
            .expect_err("one-agent swept overlap audit must be rejected");
        assert!(
            error
                .to_string()
                .contains("invalid swept on-surface reference-disc audit telemetry")
        );
    }

    #[test]
    fn reference_clearance_acceptance_requires_zero_audits() {
        let source = r#"
scenario "reference-clearance-acceptance"
seed 1
duration 1s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (9m, 1m, 0m) width 2m
agents passenger count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
        let scenario = parse(source).expect("source parses");
        let mut bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
        require_zero_reference_clearance(&bundle).expect("single-agent run has zero audits");

        bundle
            .metrics
            .movement_metrics
            .as_mut()
            .expect("current bundle has movement telemetry")
            .swept_on_surface_clearance_audit = Some(SweptOnSurfaceClearanceMetrics {
            agents_with_swept_disc_overlaps: 2,
            swept_disc_overlap_pair_steps: 1,
            maximum_swept_disc_overlap_m: 0.1,
        });
        let error = require_zero_reference_clearance(&bundle)
            .expect_err("nonzero reference audit is not accepted");
        assert!(
            error
                .to_string()
                .contains("reference-clearance audits are nonzero")
        );
    }
}
