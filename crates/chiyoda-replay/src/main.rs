use anyhow::{Context, Result, bail};
use chiyoda_core::{
    AgentState, RunBundle,
    bundle::TraceFrame,
    model::{ConnectorKind, Scenario, Surface},
};
use clap::Parser;
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

const WIDTH: usize = 1_200;
const HEIGHT: usize = 800;
const BACKGROUND: u32 = 0x0010_141a;
const SURFACE: u32 = 0x001d_2835;
const SURFACE_BORDER: u32 = 0x0045_6078;
const OBSTACLE: u32 = 0x003d_4a56;
const WAYPOINT: u32 = 0x00c6_8cff;
const EXIT: u32 = 0x004a_de80;
const GATE: u32 = 0x00ff_c857;
const STAIR: u32 = 0x00ff_9f1c;
const RAMP: u32 = 0x005c_d5f5;
const ESCALATOR: u32 = 0x00e8_79f9;
const LIFT: u32 = 0x00fb_7185;
const MOVING: u32 = 0x0065_d1ff;
const WAITING_TO_DEPART: u32 = 0x008a_94a6;
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
    /// Initial authored surface to display. Tab cycles declared surfaces.
    #[arg(long)]
    surface: Option<String>,
    /// Simulation seconds shown per wall-clock second.
    #[arg(long, default_value_t = 1.0)]
    speed: f64,
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
    validate_playback_timing(&bundle.trace, cli.speed)?;
    let surface_index = surface_index(&bundle.scenario.scenario.surfaces, cli.surface.as_deref())?;
    replay(&bundle, cli.paused, surface_index, cli.speed)
}

fn replay(
    bundle: &RunBundle,
    mut paused: bool,
    mut surface_index: usize,
    speed: f64,
) -> Result<()> {
    let mut window = Window::new(
        "Chiyoda replay — space: pause, arrows: step, tab: surface, escape: quit",
        WIDTH,
        HEIGHT,
        WindowOptions::default(),
    )?;
    window.set_target_fps(30);
    let mut buffer = vec![BACKGROUND; WIDTH * HEIGHT];
    let mut index = 0usize;
    let mut last_advance = Instant::now();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            paused = !paused;
            last_advance = Instant::now();
        }
        let mut manually_advanced = false;
        if window.is_key_pressed(Key::Right, KeyRepeat::Yes) {
            index = (index + 1).min(bundle.trace.len() - 1);
            manually_advanced = true;
        }
        if window.is_key_pressed(Key::Left, KeyRepeat::Yes) {
            index = index.saturating_sub(1);
            manually_advanced = true;
        }
        if window.is_key_pressed(Key::Tab, KeyRepeat::No) {
            surface_index = (surface_index + 1) % bundle.scenario.scenario.surfaces.len();
        }
        if manually_advanced {
            last_advance = Instant::now();
        }
        if !paused
            && index < bundle.trace.len() - 1
            && last_advance.elapsed() >= frame_delay(&bundle.trace, index, speed)
        {
            index += 1;
            last_advance = Instant::now();
        }
        buffer.fill(BACKGROUND);
        let surface = &bundle.scenario.scenario.surfaces[surface_index];
        let extent = extent_for_surface(surface);
        window.set_title(&format!(
            "Chiyoda replay — {} — {:.3}s — {speed:.2}× — space: pause, arrows: step, tab: surface, escape: quit",
            surface.id, bundle.trace[index].time_s,
        ));
        draw_scene(&mut buffer, &bundle.scenario.scenario, surface, extent);
        draw_frame(&mut buffer, bundle, index, &surface.id, extent);
        window.update_with_buffer(&buffer, WIDTH, HEIGHT)?;
        if paused {
            thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}

fn validate_playback_timing(trace: &[TraceFrame], speed: f64) -> Result<()> {
    if !speed.is_finite() || speed <= 0.0 {
        bail!("--speed must be a finite value greater than zero");
    }
    for (index, frame) in trace.iter().enumerate() {
        if !frame.time_s.is_finite() {
            bail!("trace frame {index} has a non-finite timestamp");
        }
        if index > 0 && frame.time_s < trace[index - 1].time_s {
            bail!("trace frame {index} precedes the prior frame in simulation time");
        }
        if index > 0 {
            let delay_s = (frame.time_s - trace[index - 1].time_s) / speed;
            if !delay_s.is_finite() || delay_s > Duration::MAX.as_secs_f64() {
                bail!("trace timestamps and --speed produce an unrepresentable playback delay");
            }
        }
    }
    Ok(())
}

fn frame_delay(trace: &[TraceFrame], index: usize, speed: f64) -> Duration {
    Duration::from_secs_f64((trace[index + 1].time_s - trace[index].time_s) / speed)
}

fn surface_index(surfaces: &[Surface], requested: Option<&str>) -> Result<usize> {
    if surfaces.is_empty() {
        bail!("bundle scenario has no authored surfaces");
    }
    match requested {
        Some(id) => surfaces
            .iter()
            .position(|surface| surface.id == id)
            .with_context(|| format!("surface `{id}` is not present in the bundle")),
        None => Ok(0),
    }
}

fn extent_for_surface(surface: &Surface) -> (f64, f64, f64, f64) {
    let padding = (surface.width_m.max(surface.depth_m) * 0.05).max(1.0);
    (
        surface.origin.x_m - padding,
        surface.width_m + padding * 2.0,
        surface.origin.y_m - padding,
        surface.depth_m + padding * 2.0,
    )
}

fn draw_scene(
    buffer: &mut [u32],
    scenario: &Scenario,
    surface: &Surface,
    extent: (f64, f64, f64, f64),
) {
    draw_rectangle(
        buffer,
        surface.origin.x_m,
        surface.origin.y_m,
        surface.width_m,
        surface.depth_m,
        SURFACE,
        extent,
    );
    draw_rectangle_outline(
        buffer,
        surface.origin.x_m,
        surface.origin.y_m,
        surface.width_m,
        surface.depth_m,
        SURFACE_BORDER,
        extent,
    );
    for obstacle in &scenario.obstacles {
        if obstacle.surface == surface.id {
            draw_rectangle(
                buffer,
                obstacle.at.x_m,
                obstacle.at.y_m,
                obstacle.width_m,
                obstacle.depth_m,
                OBSTACLE,
                extent,
            );
        }
    }
    for waypoint in &scenario.waypoints {
        if waypoint.surface == surface.id {
            draw_marker(buffer, waypoint.at.x_m, waypoint.at.y_m, WAYPOINT, extent);
        }
    }
    for exit in &scenario.exits {
        if exit.surface == surface.id {
            draw_marker(buffer, exit.at.x_m, exit.at.y_m, EXIT, extent);
        }
    }
    for gate in &scenario.gates {
        if gate.surface == surface.id {
            draw_marker(buffer, gate.at.x_m, gate.at.y_m, GATE, extent);
        }
    }
    for connector in &scenario.connectors {
        let color = match connector.kind() {
            ConnectorKind::Stair => STAIR,
            ConnectorKind::Ramp => RAMP,
            ConnectorKind::Escalator => ESCALATOR,
            ConnectorKind::Lift => LIFT,
        };
        if connector.from_surface() == surface.id {
            let point = connector.from();
            draw_marker(buffer, point.x_m, point.y_m, color, extent);
        }
        if connector.to_surface() == surface.id {
            let point = connector.to();
            draw_marker(buffer, point.x_m, point.y_m, color, extent);
        }
    }
}

fn draw_frame(
    buffer: &mut [u32],
    bundle: &RunBundle,
    index: usize,
    surface_id: &str,
    extent: (f64, f64, f64, f64),
) {
    let frame = &bundle.trace[index];
    for agent in &frame.agents {
        if agent.surface != surface_id {
            continue;
        }
        let x = project(agent.x_m, extent.0, extent.1, WIDTH);
        let y = project(agent.y_m, extent.2, extent.3, HEIGHT);
        let color = match agent.state {
            AgentState::Moving => MOVING,
            AgentState::WaitingToDepart
            | AgentState::WaitingAtWaypoint
            | AgentState::WaitingForRoute => WAITING_TO_DEPART,
            AgentState::WaitingForLift
            | AgentState::WaitingForConnector
            | AgentState::WaitingForExit
            | AgentState::InTransit => IN_TRANSIT,
            AgentState::Evacuated => EVACUATED,
        };
        draw_square(buffer, x, y, 2, color);
    }
}

fn draw_rectangle(
    buffer: &mut [u32],
    x_m: f64,
    y_m: f64,
    width_m: f64,
    depth_m: f64,
    color: u32,
    extent: (f64, f64, f64, f64),
) {
    let left = project(x_m, extent.0, extent.1, WIDTH);
    let right = project(x_m + width_m, extent.0, extent.1, WIDTH);
    let top = project(y_m, extent.2, extent.3, HEIGHT);
    let bottom = project(y_m + depth_m, extent.2, extent.3, HEIGHT);
    for y in top.min(bottom)..=top.max(bottom) {
        for x in left.min(right)..=left.max(right) {
            set_pixel(buffer, x, y, color);
        }
    }
}

fn draw_rectangle_outline(
    buffer: &mut [u32],
    x_m: f64,
    y_m: f64,
    width_m: f64,
    depth_m: f64,
    color: u32,
    extent: (f64, f64, f64, f64),
) {
    let left = project(x_m, extent.0, extent.1, WIDTH);
    let right = project(x_m + width_m, extent.0, extent.1, WIDTH);
    let top = project(y_m, extent.2, extent.3, HEIGHT);
    let bottom = project(y_m + depth_m, extent.2, extent.3, HEIGHT);
    for x in left.min(right)..=left.max(right) {
        set_pixel(buffer, x, top, color);
        set_pixel(buffer, x, bottom, color);
    }
    for y in top.min(bottom)..=top.max(bottom) {
        set_pixel(buffer, left, y, color);
        set_pixel(buffer, right, y, color);
    }
}

fn draw_marker(buffer: &mut [u32], x_m: f64, y_m: f64, color: u32, extent: (f64, f64, f64, f64)) {
    let x = project(x_m, extent.0, extent.1, WIDTH);
    let y = project(y_m, extent.2, extent.3, HEIGHT);
    draw_square(buffer, x, y, 3, color);
}

fn draw_square(buffer: &mut [u32], x: isize, y: isize, radius: isize, color: u32) {
    for offset_y in -radius..=radius {
        for offset_x in -radius..=radius {
            set_pixel(buffer, x + offset_x, y + offset_y, color);
        }
    }
}

fn set_pixel(buffer: &mut [u32], x: isize, y: isize, color: u32) {
    if x >= 0 && x < WIDTH.cast_signed() && y >= 0 && y < HEIGHT.cast_signed() {
        buffer[y.cast_unsigned() * WIDTH + x.cast_unsigned()] = color;
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

#[cfg(test)]
mod tests {
    use super::*;
    use chiyoda_core::{
        bundle::TraceFrame,
        model::{Connector, Exit, Gate, Obstacle, Point3, Scenario, Waypoint},
    };

    fn surface(id: &str, x_m: f64, y_m: f64, width_m: f64, depth_m: f64) -> Surface {
        Surface {
            id: id.to_owned(),
            origin: Point3 { x_m, y_m, z_m: 0.0 },
            width_m,
            depth_m,
        }
    }

    #[test]
    fn surface_extent_uses_authored_dimensions_not_trace_positions() {
        assert_eq!(
            extent_for_surface(&surface("concourse", 10.0, 20.0, 40.0, 10.0)),
            (8.0, 44.0, 18.0, 14.0)
        );
    }

    #[test]
    fn drawing_a_scene_preserves_static_geometry_beneath_agents() {
        let surface = surface("concourse", 0.0, 0.0, 10.0, 10.0);
        let scenario = Scenario {
            name: "view-test".to_owned(),
            seed: 0,
            duration_s: 1.0,
            timestep_s: 1.0,
            surfaces: vec![surface.clone()],
            obstacles: vec![Obstacle {
                id: "column".to_owned(),
                surface: "concourse".to_owned(),
                at: Point3 {
                    x_m: 1.0,
                    y_m: 1.0,
                    z_m: 0.0,
                },
                width_m: 2.0,
                depth_m: 2.0,
            }],
            waypoints: vec![Waypoint {
                id: "meeting".to_owned(),
                surface: "concourse".to_owned(),
                at: Point3 {
                    x_m: 4.0,
                    y_m: 4.0,
                    z_m: 0.0,
                },
                dwell_s: 0.0,
            }],
            exits: vec![Exit {
                id: "street".to_owned(),
                surface: "concourse".to_owned(),
                at: Point3 {
                    x_m: 5.0,
                    y_m: 5.0,
                    z_m: 0.0,
                },
                width_m: 1.0,
                capacity_per_s: None,
            }],
            connectors: vec![Connector::Stair {
                id: "stairs".to_owned(),
                from_surface: "concourse".to_owned(),
                from: Point3 {
                    x_m: 6.0,
                    y_m: 6.0,
                    z_m: 0.0,
                },
                to_surface: "upper".to_owned(),
                to: Point3 {
                    x_m: 6.0,
                    y_m: 6.0,
                    z_m: 3.0,
                },
                width_m: 1.0,
                capacity_per_s: None,
                clearance_height_m: None,
            }],
            connector_states: Vec::new(),
            exit_states: Vec::new(),
            connector_capacity_states: Vec::new(),
            exit_capacity_states: Vec::new(),
            gates: vec![Gate {
                id: "barrier".to_owned(),
                surface: "concourse".to_owned(),
                at: Point3 {
                    x_m: 7.0,
                    y_m: 7.0,
                    z_m: 0.0,
                },
                width_m: 1.0,
                service_rate_per_s: 1.0,
                destination: "street".to_owned(),
            }],
            gate_states: Vec::new(),
            gate_capacity_states: Vec::new(),
            agents: Vec::new(),
            messages: Vec::new(),
            countermeasures: Vec::new(),
        };
        let mut buffer = vec![BACKGROUND; WIDTH * HEIGHT];
        let extent = extent_for_surface(&surface);
        draw_scene(&mut buffer, &scenario, &surface, extent);

        let location = |x_m, y_m| {
            project(x_m, extent.0, extent.1, WIDTH).cast_unsigned()
                + project(y_m, extent.2, extent.3, HEIGHT).cast_unsigned() * WIDTH
        };
        let center = location(5.0, 5.0);
        let corner = project(0.0, extent.0, extent.1, WIDTH).cast_unsigned()
            + project(0.0, extent.2, extent.3, HEIGHT).cast_unsigned() * WIDTH;
        assert_eq!(buffer[center], EXIT);
        assert_eq!(buffer[corner], SURFACE_BORDER);
        assert_eq!(buffer[location(2.0, 2.0)], OBSTACLE);
        assert_eq!(buffer[location(4.0, 4.0)], WAYPOINT);
        assert_eq!(buffer[location(6.0, 6.0)], STAIR);
        assert_eq!(buffer[location(7.0, 7.0)], GATE);
    }

    #[test]
    fn named_surface_selection_follows_declaration_order_and_rejects_unknown_ids() {
        let surfaces = [
            surface("concourse", 0.0, 0.0, 10.0, 10.0),
            surface("platform", 0.0, 0.0, 10.0, 10.0),
        ];

        assert_eq!(surface_index(&surfaces, None).unwrap(), 0);
        assert_eq!(surface_index(&surfaces, Some("platform")).unwrap(), 1);
        assert!(surface_index(&surfaces, Some("roof")).is_err());
    }

    #[test]
    fn replay_timing_uses_simulation_timestamps_and_rejects_regressions() {
        let trace = [
            TraceFrame {
                step: 0,
                time_s: 0.0,
                agents: Vec::new(),
            },
            TraceFrame {
                step: 1,
                time_s: 0.5,
                agents: Vec::new(),
            },
        ];
        assert_eq!(frame_delay(&trace, 0, 2.0), Duration::from_millis(250));
        assert!(validate_playback_timing(&trace, 1.0).is_ok());
        let invalid = [trace[1].clone(), trace[0].clone()];
        assert!(validate_playback_timing(&invalid, 1.0).is_err());
        assert!(validate_playback_timing(&trace, 0.0).is_err());
        let unrepresentable = [
            trace[0].clone(),
            TraceFrame {
                step: 2,
                time_s: f64::MAX,
                agents: Vec::new(),
            },
        ];
        assert!(validate_playback_timing(&unrepresentable, 1.0).is_err());
    }
}
