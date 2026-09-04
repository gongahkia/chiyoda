use anyhow::{Context, Result, bail};
use chiyoda_core::{
    BenchmarkManifest, CanonicalScenario, EvidenceCatalog, RunBundle, RunOptions, bundle_hash,
    calibrate_eindhoven_platform, format_scenario, generator, parse, run, validate,
    validate_catalog, validate_manifest, verify_catalog_files,
};
use clap::{Parser, Subcommand, ValueEnum};
use std::{
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
