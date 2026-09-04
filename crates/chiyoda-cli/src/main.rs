use anyhow::{Context, Result, bail};
use chiyoda_core::{
    BenchmarkManifest, CanonicalScenario, EvidenceCatalog, InformationDeliveryMetrics, RunBundle,
    RunOptions, bundle_hash, calibrate_eindhoven_platform, format_scenario, generator, parse, run,
    validate, validate_catalog, validate_manifest, verify_catalog_files,
};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
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
    /// Verify an empirical benchmark round's evidence and seed-release contract.
    Benchmark {
        #[command(subcommand)]
        command: BenchmarkCommand,
    },
    /// Validate and content-lock research data before it can enter a round.
    Evidence {
        #[command(subcommand)]
        command: EvidenceCommand,
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
    /// Validate source/license/split metadata without reading acquired data.
    Verify { catalog: PathBuf },
    /// Validate a catalog and verify every acquired source's size and SHA-256.
    Lock {
        catalog: PathBuf,
        #[arg(long, default_value = "data/raw")]
        data_root: PathBuf,
    },
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

#[derive(Debug, Serialize)]
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
struct SweepPairing {
    first_seed: u64,
    run_count: u32,
    baseline_template_scenario_hash: String,
    candidate_template_scenario_hash: String,
    changed_scenario_sections: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PairedRun {
    seed: u64,
    total_agents: u32,
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
        Command::Benchmark { command } => match command {
            BenchmarkCommand::Verify { manifest } => verify_benchmark(&manifest)?,
        },
        Command::Evidence { command } => handle_evidence(command)?,
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
    let template_hash =
        chiyoda_core::bundle::canonical_hash(&CanonicalScenario::from(template.clone()));
    prepare_sweep_output(count, output, trace_every_steps)?;
    fs::write(output.join("template.chy"), format_scenario(&template))
        .with_context(|| format!("writing template into {}", output.display()))?;
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
    let baseline = load_and_verify_sweep(baseline_directory)?;
    let candidate = load_and_verify_sweep(candidate_directory)?;
    let baseline_template = authored_template(baseline_directory, &baseline, "baseline")?;
    let candidate_template = authored_template(candidate_directory, &candidate, "candidate")?;
    let changed_scenario_sections =
        compatible_comparison_sections(&baseline_template, &candidate_template)?;
    let comparison = compare_sweep_summaries(&baseline, &candidate, changed_scenario_sections)?;
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
) -> Result<Vec<String>> {
    if baseline.duration_s.total_cmp(&candidate.duration_s) != std::cmp::Ordering::Equal {
        bail!("comparison requires identical scenario durations");
    }
    if baseline.timestep_s.total_cmp(&candidate.timestep_s) != std::cmp::Ordering::Equal {
        bail!("comparison requires identical simulation timesteps");
    }
    if baseline.agents != candidate.agents {
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

fn load_and_verify_sweep(directory: &Path) -> Result<SweepSummary> {
    let summary: SweepSummary = read_json(&directory.join("summary.json"))?;
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
    for (offset, record) in summary.runs.iter().enumerate() {
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
    if matches!(bundle.bundle_version.as_str(), "0.17" | "0.18" | "0.19") {
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
    if matches!(bundle.bundle_version.as_str(), "0.18" | "0.19")
        && !expected_interventions.is_empty()
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
) -> Result<SweepComparison> {
    if baseline.first_seed != candidate.first_seed || baseline.count != candidate.count {
        bail!("comparison requires matching contiguous seed ranges");
    }
    let baseline_template_scenario_hash = template_hash_for_comparison(baseline, "baseline")?;
    let candidate_template_scenario_hash = template_hash_for_comparison(candidate, "candidate")?;
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
        if baseline_run.total_agents != candidate_run.total_agents {
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
            total_agents: baseline_run.total_agents,
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
            baseline_template_scenario_hash,
            candidate_template_scenario_hash,
            changed_scenario_sections,
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
        claim_boundary: "This report compares deterministic structural runs sharing authored demand and seed labels. It is not an empirical control group, a statistical uncertainty estimate, a causal-effect estimate, a benchmark score, calibration result, or predictive claim.".to_owned(),
    })
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

fn dataset_role(partition: EvidencePartition) -> chiyoda_core::benchmark::DatasetRole {
    match partition {
        EvidencePartition::Calibration => chiyoda_core::benchmark::DatasetRole::Calibration,
        EvidencePartition::HeldOut => chiyoda_core::benchmark::DatasetRole::HeldOut,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SweepRun, SweepSource, SweepSummary, compare_sweep_summaries, describe_sweep,
        validate_bundle_metrics,
    };
    use chiyoda_core::{
        InformationDeliveryMetrics, InformationInterventionKind, RunOptions, generator, parse, run,
    };
    use std::{collections::BTreeMap, path::Path};

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
        )
        .expect("compatible authored sweeps compare");

        assert_eq!(comparison.pairing.run_count, 2);
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

        let error = compare_sweep_summaries(&summary, &candidate, Vec::new())
            .expect_err("different seed ranges cannot be paired");

        assert!(
            error
                .to_string()
                .contains("matching contiguous seed ranges")
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
