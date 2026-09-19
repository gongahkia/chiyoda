use anyhow::{Context, Result, bail};
use chiyoda_core::{
    BundleVerification, CanonicalScenario, RunBundle, RunOptions, format_scenario, generator,
    parse, run, validate, verify_run_bundle,
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
    about = "Author, run, and inspect deterministic pedestrian-flow scenarios"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse and validate a scenario without executing it.
    Check { source: PathBuf },
    /// Compile a scenario to canonical, versioned JSON IR.
    Compile {
        source: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Render a scenario in canonical Chiyoda source style.
    Format {
        source: PathBuf,
        /// Write formatted source to a distinct file instead of standard output.
        #[arg(short, long, conflicts_with = "check")]
        output: Option<PathBuf>,
        /// Exit non-zero when the source is not already canonical.
        #[arg(long)]
        check: bool,
    },
    /// Execute one deterministic scenario and write its reproducible run bundle.
    Run {
        source: PathBuf,
        #[arg(short, long, default_value = "out")]
        output: PathBuf,
        #[arg(long, default_value_t = 10)]
        trace_every: u32,
    },
    /// Generate a deterministic structural example from a seed.
    Generate {
        #[arg(long)]
        seed: u64,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Reconstruct and summarize a current run bundle.
    Replay { bundle: PathBuf },
}

fn main() -> Result<()> {
    match Cli::parse().command {
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
                if read_text(&source)? != formatted {
                    bail!(
                        "{} is not canonically formatted; run `chiyoda format {}`",
                        source.display(),
                        source.display()
                    );
                }
                println!("formatted: {}", source.display());
            } else if let Some(output) = output {
                write_text(&output, &formatted)?;
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
            write_text(&output.join("scenario.chy"), &text)?;
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
            write_text(&output, &source)?;
            println!("generated: {} ({})", output.display(), scenario.name);
        }
        Command::Replay {
            bundle: bundle_path,
        } => {
            let bundle: RunBundle = read_json(&bundle_path)?;
            if verify_run_bundle(&bundle)? != BundleVerification::Reconstructed {
                bail!("bundle uses an incompatible runtime contract and cannot be reconstructed");
            }
            println!("verified reconstruction: {}", bundle.bundle_hash);
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

fn read_scenario(path: &Path) -> Result<chiyoda_core::Scenario> {
    let source = read_text(path)?;
    let scenario = parse(&source).map_err(|error| anyhow::anyhow!(error))?;
    validate(&scenario).map_err(|errors| validation_error(&errors))?;
    Ok(scenario)
}

fn read_text(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_str(&read_text(path)?)
        .with_context(|| format!("parsing JSON {}", path.display()))
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    ensure_parent(path)?;
    let mut bytes = serde_json::to_vec_pretty(value).context("serializing canonical JSON")?;
    bytes.push(b'\n');
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

fn write_text(path: &Path, text: &str) -> Result<()> {
    ensure_parent(path)?;
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    Ok(())
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
