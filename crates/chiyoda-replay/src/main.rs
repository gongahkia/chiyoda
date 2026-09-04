use anyhow::{Context, Result, bail};
use chiyoda_core::{AgentState, RunBundle};
use clap::Parser;
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::{fs, path::PathBuf, thread, time::Duration};

const WIDTH: usize = 1_200;
const HEIGHT: usize = 800;
const BACKGROUND: u32 = 0x0010_141a;
const MOVING: u32 = 0x0065_d1ff;
const IN_TRANSIT: u32 = 0x00ff_c857;
const EVACUATED: u32 = 0x004a_de80;

#[derive(Debug, Parser)]
#[command(
    name = "chiyoda-replay",
    version,
    about = "Native deterministic Chiyoda trace replay"
)]
struct Cli {
    /// A `run.json` emitted by `chiyoda run`.
    bundle: PathBuf,
    /// Start paused; space toggles playback and arrow keys advance frames.
    #[arg(long)]
    paused: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let source = fs::read_to_string(&cli.bundle)
        .with_context(|| format!("reading {}", cli.bundle.display()))?;
    let bundle: RunBundle = serde_json::from_str(&source)
        .with_context(|| format!("parsing {}", cli.bundle.display()))?;
    if !bundle.verifies_hash() {
        bail!("bundle integrity check failed");
    }
    if bundle.trace.is_empty() {
        bail!("bundle contains no trace frames");
    }
    replay(&bundle, cli.paused)
}

fn replay(bundle: &RunBundle, mut paused: bool) -> Result<()> {
    let mut window = Window::new(
        "Chiyoda replay — space: pause, arrows: step, escape: quit",
        WIDTH,
        HEIGHT,
        WindowOptions::default(),
    )?;
    window.set_target_fps(30);
    let mut buffer = vec![BACKGROUND; WIDTH * HEIGHT];
    let extent = extent(bundle);
    let mut index = 0usize;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            paused = !paused;
        }
        if window.is_key_pressed(Key::Right, KeyRepeat::Yes) {
            index = (index + 1).min(bundle.trace.len() - 1);
        }
        if window.is_key_pressed(Key::Left, KeyRepeat::Yes) {
            index = index.saturating_sub(1);
        }
        if !paused && index < bundle.trace.len() - 1 {
            index += 1;
        }
        buffer.fill(BACKGROUND);
        draw_frame(&mut buffer, bundle, index, extent);
        window.update_with_buffer(&buffer, WIDTH, HEIGHT)?;
        if paused {
            thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}

fn extent(bundle: &RunBundle) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for frame in &bundle.trace {
        for agent in &frame.agents {
            min_x = min_x.min(agent.x_m);
            max_x = max_x.max(agent.x_m);
            min_y = min_y.min(agent.y_m);
            max_y = max_y.max(agent.y_m);
        }
    }
    let padding = 1.0;
    (
        min_x - padding,
        (max_x - min_x).max(1.0) + padding * 2.0,
        min_y - padding,
        (max_y - min_y).max(1.0) + padding * 2.0,
    )
}

fn draw_frame(buffer: &mut [u32], bundle: &RunBundle, index: usize, extent: (f64, f64, f64, f64)) {
    let frame = &bundle.trace[index];
    for agent in &frame.agents {
        let x = project(agent.x_m, extent.0, extent.1, WIDTH);
        let y = project(agent.y_m, extent.2, extent.3, HEIGHT);
        let color = match agent.state {
            AgentState::Moving => MOVING,
            AgentState::WaitingForLift | AgentState::InTransit => IN_TRANSIT,
            AgentState::Evacuated => EVACUATED,
        };
        for offset_y in -2..=2 {
            for offset_x in -2..=2 {
                let pixel_x = x + offset_x;
                let pixel_y = y + offset_y;
                if pixel_x >= 0
                    && pixel_x < WIDTH.cast_signed()
                    && pixel_y >= 0
                    && pixel_y < HEIGHT.cast_signed()
                {
                    let location = pixel_y.cast_unsigned() * WIDTH + pixel_x.cast_unsigned();
                    buffer[location] = color;
                }
            }
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // values are clamped to the small, fixed native framebuffer before conversion
fn project(value: f64, origin: f64, span: f64, pixels: usize) -> isize {
    let upper = (pixels - 1) as f64;
    (((value - origin) / span * upper).round().clamp(0.0, upper)) as isize
}
