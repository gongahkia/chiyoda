use anyhow::{Context, Result, bail};
use chiyoda_core::{
    BenchmarkManifest, CanonicalScenario, EvidenceCatalog, RunBundle, RunOptions, bundle_hash,
    calibrate_eindhoven_platform, format_scenario, generator, parse, run, validate,
    validate_catalog, validate_manifest, verify_catalog_files,
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
    /// Verify every run bundle and source recorded by a generated sweep.
    VerifySweep { directory: PathBuf },
    /// Verify a sweep and emit exact descriptive aggregates; this is not a benchmark score.
    AnalyzeSweep {
        directory: PathBuf,
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

#[derive(Debug, Serialize, Deserialize)]
struct SweepSummary {
    schema_version: String,
    generator_version: String,
    first_seed: u64,
    count: u32,
    trace_every_steps: u32,
    runs: Vec<SweepRun>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SweepRun {
    seed: u64,
    scenario_name: String,
    bundle_hash: String,
    total_agents: u32,
    evacuated_agents: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    evacuated_by_exit: BTreeMap<String, u32>,
    clearance_time_s: Option<f64>,
}

#[derive(Debug, Serialize)]
struct SweepAnalysis {
    schema_version: String,
    input_sweep_schema_version: String,
    generator_version: String,
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
    clearance_time_s: Option<DescriptiveRange>,
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
        Command::VerifySweep { directory } => verify_sweep(&directory)?,
        Command::AnalyzeSweep { directory, output } => {
            analyze_sweep(&directory, output.as_deref())?;
        }
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
    if count == 0 {
        bail!("sweep count must be greater than zero");
    }
    if trace_every_steps == 0 {
        bail!("trace-every must be greater than zero");
    }
    ensure_empty_directory(output)?;

    let mut runs = Vec::with_capacity(usize::try_from(count).expect("u32 fits usize"));
    for offset in 0..count {
        let seed = first_seed
            .checked_add(u64::from(offset))
            .context("sweep seed range exceeds u64")?;
        let source = generator::source(seed);
        let scenario = generator::scenario(seed)?;
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
            clearance_time_s: bundle.metrics.clearance_time_s,
        });
    }
    let summary = SweepSummary {
        schema_version: "0.1".to_owned(),
        generator_version: chiyoda_core::LANGUAGE_VERSION.to_owned(),
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
        validate_bundle_exit_metrics(&bundle, &run_directory)?;
        let source = read_text(&run_directory.join("scenario.chy"))?;
        let scenario = parse(&source).map_err(|error| anyhow::anyhow!(error))?;
        validate(&scenario).map_err(|errors| validation_error(&errors))?;
        if CanonicalScenario::from(scenario) != bundle.scenario {
            bail!(
                "source and canonical scenario disagree: {}",
                run_directory.display()
            );
        }
        if record.scenario_name != bundle.scenario.scenario.name
            || record.bundle_hash != bundle.bundle_hash
            || record.total_agents != bundle.metrics.total_agents
            || record.evacuated_agents != bundle.metrics.evacuated_agents
            || record.evacuated_by_exit != bundle.metrics.evacuated_by_exit
            || record.clearance_time_s != bundle.metrics.clearance_time_s
        {
            bail!(
                "summary and run bundle disagree: {}",
                run_directory.display()
            );
        }
    }
    Ok(summary)
}

fn validate_bundle_exit_metrics(bundle: &RunBundle, directory: &Path) -> Result<()> {
    let metrics = &bundle.metrics;
    if metrics.evacuated_agents > metrics.total_agents {
        bail!(
            "bundle evacuation count exceeds total agents: {}",
            directory.display()
        );
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
    Ok(())
}

fn describe_sweep(summary: &SweepSummary) -> SweepAnalysis {
    let mut total_agents = 0_u64;
    let mut evacuated_agents = 0_u64;
    let mut runs_with_any_evacuation = 0_u32;
    let mut fully_evacuated_runs = 0_u32;
    let mut evacuated_by_exit = BTreeMap::new();
    let mut clearance_times = Vec::new();

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
        if let Some(clearance_time_s) = run.clearance_time_s {
            clearance_times.push(clearance_time_s);
        }
    }

    let attributed_evacuations = evacuated_by_exit.values().sum::<u64>();
    let clearance_time_range = (!clearance_times.is_empty()).then(|| {
        let measured_runs = u32::try_from(clearance_times.len()).expect("sweep count fits u32");
        let minimum_s = clearance_times
            .iter()
            .copied()
            .reduce(f64::min)
            .expect("a non-empty collection has a minimum");
        let maximum_s = clearance_times
            .iter()
            .copied()
            .reduce(f64::max)
            .expect("a non-empty collection has a maximum");
        DescriptiveRange {
            measured_runs,
            minimum_s,
            mean_s: clearance_times.iter().sum::<f64>() / f64::from(measured_runs),
            maximum_s,
        }
    });
    SweepAnalysis {
        schema_version: "0.1".to_owned(),
        input_sweep_schema_version: summary.schema_version.clone(),
        generator_version: summary.generator_version.clone(),
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
        clearance_time_s: clearance_time_range,
        claim_boundary: "This report aggregates deterministic structural runs. It is not a benchmark score, calibration result, uncertainty estimate, or predictive claim.".to_owned(),
    }
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
    use super::{SweepRun, SweepSummary, describe_sweep};
    use std::collections::BTreeMap;

    #[test]
    fn sweep_analysis_keeps_counts_exact_and_labels_legacy_attribution() {
        let summary = SweepSummary {
            schema_version: "0.1".to_owned(),
            generator_version: "0.15".to_owned(),
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
                    clearance_time_s: Some(2.0),
                },
                SweepRun {
                    seed: 11,
                    scenario_name: "two".to_owned(),
                    bundle_hash: "b".repeat(64),
                    total_agents: 5,
                    evacuated_agents: 0,
                    evacuated_by_exit: BTreeMap::new(),
                    clearance_time_s: None,
                },
                SweepRun {
                    seed: 12,
                    scenario_name: "three".to_owned(),
                    bundle_hash: "c".repeat(64),
                    total_agents: 5,
                    evacuated_agents: 5,
                    evacuated_by_exit: BTreeMap::from([("east".to_owned(), 5)]),
                    clearance_time_s: Some(4.0),
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
        let clearance_time = analysis
            .clearance_time_s
            .expect("two runs report clearance times");
        assert_eq!(clearance_time.measured_runs, 2);
        assert!((clearance_time.minimum_s - 2.0).abs() < f64::EPSILON);
        assert!((clearance_time.mean_s - 3.0).abs() < f64::EPSILON);
        assert!((clearance_time.maximum_s - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn sweep_analysis_discloses_legacy_bundles_without_exit_attribution() {
        let summary = SweepSummary {
            schema_version: "0.1".to_owned(),
            generator_version: "0.14".to_owned(),
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
                clearance_time_s: Some(8.0),
            }],
        };

        let analysis = describe_sweep(&summary);

        assert_eq!(analysis.evacuated_by_exit, BTreeMap::new());
        assert_eq!(analysis.unattributed_evacuations, 3);
    }
}
