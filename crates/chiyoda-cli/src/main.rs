use anyhow::{Context, Result, bail};
use chiyoda_core::{
    BenchmarkManifest, CanonicalScenario, RunBundle, RunOptions, bundle_hash, generator, parse,
    run, validate, validate_manifest,
};
use clap::{Parser, Subcommand};
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
    /// Verify a run bundle's trace-integrity hash and print its summary.
    Replay { bundle: PathBuf },
}

#[derive(Debug, Subcommand)]
enum BenchmarkCommand {
    Verify { manifest: PathBuf },
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
        Command::Run {
            source,
            output,
            trace_every,
        } => {
            let text = read_text(&source)?;
            let scenario = parse(&text).map_err(|error| anyhow::anyhow!(error))?;
            validate(&scenario).map_err(validation_error)?;
            let bundle = run(
                scenario,
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
            BenchmarkCommand::Verify { manifest } => {
                let manifest: BenchmarkManifest = read_json(&manifest)?;
                validate_manifest(&manifest).map_err(benchmark_error)?;
                println!("valid empirical benchmark round: {}", manifest.round_id);
            }
        },
        Command::Replay { bundle } => {
            let bundle: RunBundle = read_json(&bundle)?;
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
            println!("open with: chiyoda-replay {}", bundle_path_hint(&bundle));
        }
    }
    Ok(())
}

fn read_scenario(path: &Path) -> Result<chiyoda_core::Scenario> {
    let text = read_text(path)?;
    let scenario = parse(&text).map_err(|error| anyhow::anyhow!(error))?;
    validate(&scenario).map_err(validation_error)?;
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
    let bytes = serde_json::to_vec_pretty(value).context("serializing canonical JSON")?;
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

fn validation_error(errors: Vec<chiyoda_core::ValidationError>) -> anyhow::Error {
    anyhow::anyhow!(
        "scenario is invalid:\n{}",
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn benchmark_error(errors: Vec<chiyoda_core::BenchmarkValidationError>) -> anyhow::Error {
    anyhow::anyhow!(
        "benchmark manifest is invalid:\n{}",
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn bundle_path_hint(bundle: &RunBundle) -> &str {
    let _ = bundle;
    "out/run.json"
}
