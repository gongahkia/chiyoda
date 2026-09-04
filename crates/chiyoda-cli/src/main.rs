use anyhow::{Context, Result, bail};
use chiyoda_core::{
    BenchmarkManifest, CanonicalScenario, EvidenceCatalog, ExperimentManifest, GeographicPoint,
    InformationDeliveryMetrics, OpenStreetMapLayoutReport, OpenStreetMapLocalProjectionReport,
    OsmInspectionLimits, RunBundle, RunOptions, SensitivityFactor, SensitivityManifest,
    bundle_hash, calibrate_eindhoven_platform, format_scenario, generator,
    inspect_openstreetmap_layout, parse, plan_sensitivity, project_openstreetmap_layout_report,
    run, summarize_crowd_queue_reference, summarize_vru_trajectory_reference, validate,
    validate_catalog, validate_experiment_manifest, validate_manifest, verify_catalog_files,
    verify_openstreetmap_layout_report, verify_openstreetmap_local_projection_report,
};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
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
    /// Execute one provenance-bound, uncalibrated authored experiment.
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
    /// Verify a run bundle's trace-integrity hash and print its summary.
    Replay { bundle: PathBuf },
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
}

#[derive(Debug, Subcommand)]
enum ExperimentCommand {
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
    trace_every_steps: u32,
    reference_report_snapshots: Vec<SensitivityReferenceReportSnapshot>,
    factors: Vec<SensitivityFactorReport>,
    conditions: Vec<SensitivityPlanCondition>,
    author_claim_boundary: String,
    claim_boundary: String,
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

#[derive(Debug, Serialize)]
struct ExperimentReport {
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
    author_claim_boundary: String,
    claim_boundary: String,
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
        } => {
            let bundle: RunBundle = read_json(&bundle_path)?;
            if !bundle.verifies_hash() || bundle_hash(&bundle) != bundle.bundle_hash {
                bail!("bundle integrity check failed");
            }
            println!("verified: {}", bundle.bundle_hash);
            println!("scenario: {}", bundle.scenario.scenario.name);
            println!("frames: {}", bundle.trace.len());
            println!(
                "evacuated: {}/{}",
                bundle.metrics.evacuated_agents, bundle.metrics.total_agents
            );
            println!("open with: chiyoda-replay {}", bundle_path.display());
        }
    }
    Ok(())
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
    }
    Ok(())
}

fn handle_experiment(command: ExperimentCommand) -> Result<()> {
    match command {
        ExperimentCommand::Run { manifest, output } => run_experiment(&manifest, &output)?,
        ExperimentCommand::Verify { directory } => verify_experiment(&directory)?,
    }
    Ok(())
}

fn run_experiment(manifest_path: &Path, output: &Path) -> Result<()> {
    let (manifest, scenario_source, scenario) = load_experiment(manifest_path)?;
    let source_reports = capture_experiment_source_reports(&manifest, manifest_path)?;
    ensure_empty_directory(output)?;

    write_json(&output.join("manifest.json"), &manifest)?;
    fs::write(output.join("scenario.chy"), &scenario_source)
        .with_context(|| format!("writing scenario snapshot into {}", output.display()))?;
    write_experiment_source_reports(output, &source_reports)?;
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
    );
    write_json(&output.join("report.json"), &report)?;
    println!(
        "uncalibrated experiment: {}",
        output.join("report.json").display()
    );
    println!("bundle hash: {}", bundle.bundle_hash);
    Ok(())
}

fn verify_experiment(directory: &Path) -> Result<()> {
    let manifest: ExperimentManifest = read_json(&directory.join("manifest.json"))?;
    validate_experiment_manifest(&manifest).map_err(|errors| experiment_error(&errors))?;
    let source_reports = verify_experiment_source_reports(directory, &manifest)?;
    verify_experiment_layout(directory, !source_reports.is_empty())?;
    let scenario_source = read_text(&directory.join("scenario.chy"))?;
    let scenario = parse(&scenario_source).map_err(|error| anyhow::anyhow!(error))?;
    validate(&scenario).map_err(|errors| validation_error(&errors))?;
    let bundle: RunBundle = read_json(&directory.join("run.json"))?;
    if !bundle.verifies_hash() || bundle_hash(&bundle) != bundle.bundle_hash {
        bail!("experiment run bundle integrity check failed");
    }
    let canonical = CanonicalScenario::from(scenario);
    if bundle.scenario != canonical
        || bundle.scenario_hash != chiyoda_core::bundle::canonical_hash(&bundle.scenario)
    {
        bail!("experiment scenario snapshot does not match its run bundle");
    }
    if bundle.options.get("trace_every_steps") != Some(&manifest.trace_every_steps.to_string()) {
        bail!("experiment run bundle does not use the manifest trace_every_steps");
    }
    let report = experiment_report(
        &manifest,
        &fs::read(directory.join("manifest.json")).context("reading manifest snapshot")?,
        scenario_source.as_bytes(),
        &bundle,
        source_reports,
    );
    let persisted_report: serde_json::Value = read_json(&directory.join("report.json"))?;
    let expected_report =
        serde_json::to_value(&report).context("serializing reconstructed experiment report")?;
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

fn verify_experiment_layout(directory: &Path, has_source_reports: bool) -> Result<()> {
    let expected = ["manifest.json", "scenario.chy", "run.json", "report.json"]
        .into_iter()
        .chain(has_source_reports.then_some("source-reports"))
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

fn experiment_report(
    manifest: &ExperimentManifest,
    manifest_bytes: &[u8],
    scenario_source: &[u8],
    bundle: &RunBundle,
    source_report_snapshots: Vec<ExperimentSourceReportSnapshot>,
) -> ExperimentReport {
    ExperimentReport {
        schema_version: "0.1".to_owned(),
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
        if CanonicalScenario::from(persisted_template)
            != CanonicalScenario::from(condition.scenario.clone())
        {
            bail!(
                "sensitivity condition template does not match its manifest-derived scenario: {}",
                condition.id
            );
        }

        let comparison = build_sensitivity_comparison(&baseline_directory, &condition_directory)?;
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
    build_sweep_comparison_with_policy(
        baseline_directory,
        candidate_directory,
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
        let run_directory = directory.join(format!("seed-{}", record.seed));
        let bundle: RunBundle = read_json(&run_directory.join("run.json"))?;
        if !bundle.verifies_hash() || bundle_hash(&bundle) != bundle.bundle_hash {
            bail!("bundle integrity check failed: {}", run_directory.display());
        }
        validate_bundle_metrics(&bundle, &run_directory)?;
        let source = read_text(&run_directory.join("scenario.chy"))?;
        let scenario = parse(&source).map_err(|error| anyhow::anyhow!(error))?;
        validate(&scenario).map_err(|errors| validation_error(&errors))?;
        if CanonicalScenario::from(scenario) != bundle.scenario {
            bail!(
                "source and canonical scenario disagree: {}",
                run_directory.display()
            );
        }
        if let Some(template) = &authored_template {
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
    }
    Ok(summary)
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
        "0.17" | "0.18" | "0.19" | "0.20" | "0.21"
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
        "0.18" | "0.19" | "0.20" | "0.21"
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
    Ok(())
}

fn describe_sweep(summary: &SweepSummary) -> SweepAnalysis {
    let mut total_agents = 0_u64;
    let mut evacuated_agents = 0_u64;
    let mut runs_with_any_evacuation = 0_u32;
    let mut fully_evacuated_runs = 0_u32;
    let mut evacuated_by_exit = BTreeMap::new();
    let mut remaining_by_state = BTreeMap::new();
    let mut information_delivery = BTreeMap::new();
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
        ExperimentCommand, InformationSamplingAlignment, LayoutCommand, SweepRun, SweepSource,
        SweepSummary, compare_sweep_summaries, describe_sweep, handle_experiment, handle_layout,
        information_sampling_alignment, validate_bundle_metrics,
    };
    use chiyoda_core::{
        InformationDeliveryMetrics, InformationInterventionKind, RunOptions, generator, parse, run,
    };
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
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

    #[test]
    fn layout_osm_writes_a_content_locked_source_observation_report() {
        let directory = test_directory("layout-osm");
        let data_root = directory.0.join("raw");
        fs::create_dir_all(&data_root).expect("creating layout data root");
        let source = br#"<osm version="0.6"><node id="1" lat="1.3" lon="103.8"><tag k="entrance" v="yes"/></node></osm>"#;
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
            max_nodes: 1,
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
    }

    #[test]
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
        super::verify_sweep(&output.join("baseline")).expect("baseline verifies");
        super::verify_sweep(&output.join("conditions/case-0001")).expect("condition verifies");
        super::verify_sensitivity(&output).expect("sensitivity study verifies");

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
        assert_eq!(
            comparison.baseline.information_delivery["notice"].received_agents,
            5
        );
        assert_eq!(
            comparison.candidate.information_delivery["notice"].accepted_agents,
            4
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
}
