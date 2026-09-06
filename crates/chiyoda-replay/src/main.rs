use anyhow::{Context, Result, bail};
use chiyoda_core::{
    AgentState, BundleVerification, RunBundle, RunOptions,
    bundle::TraceFrame,
    model::{ConnectorKind, Point3, PortalLanes, PortalResource, Scenario, Surface},
    parse, run, validate, verify_run_bundle,
};
use clap::Parser;
use flate2::{Compression, write::ZlibEncoder};
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
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
const PORTAL_LANE: u32 = 0x00f8_f9fa;
const QUEUE_FOOTPRINT: u32 = 0x00b5_6bff;
const STAIR: u32 = 0x00ff_9f1c;
const RAMP: u32 = 0x005c_d5f5;
const ESCALATOR: u32 = 0x00e8_79f9;
const LIFT: u32 = 0x00fb_7185;
const MOVING: u32 = 0x0065_d1ff;
const WAITING_TO_DEPART: u32 = 0x008a_94a6;
const IN_TRANSIT: u32 = 0x00ff_c857;
const EVACUATED: u32 = 0x004a_de80;
const WATCH_STATUS_PANEL: u32 = 0x001d_2835;
const WATCH_STATUS_BORDER: u32 = 0x0045_6078;
const WATCH_STATUS_TEXT: u32 = 0x00f8_f9fa;
const WATCH_ERROR: u32 = 0x00ff_6b6b;
const WATCH_INFO: u32 = 0x0065_d1ff;
const WATCH_POLL_INTERVAL: Duration = Duration::from_millis(100);
const WATCH_DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Debug, Parser)]
#[command(
    name = "chiyoda-replay",
    version,
    about = "Native deterministic Chiyoda trace replay"
)]
struct Cli {
    /// A `run.json` emitted by `chiyoda run`.
    bundle: Option<PathBuf>,
    /// Compile, validate, and rerun this Chiyoda source after every saved edit.
    #[arg(long, value_name = "SOURCE", conflicts_with = "bundle")]
    watch: Option<PathBuf>,
    /// Start paused; space toggles playback and arrow keys advance frames.
    #[arg(long)]
    paused: bool,
    /// Initial authored surface to display. Tab cycles declared surfaces.
    #[arg(long)]
    surface: Option<String>,
    /// Simulation seconds shown per wall-clock second.
    #[arg(long, default_value_t = 1.0)]
    speed: f64,
    /// Trace cadence for in-memory runs made by `--watch`.
    #[arg(long, default_value_t = 1)]
    trace_every: u32,
    /// Directory in which pressing P writes a PNG snapshot of the current frame.
    #[arg(long, default_value = "out/chiyoda-replay")]
    snapshot_dir: PathBuf,
    /// Permit a hash-only display of an incompatible legacy runtime artifact.
    #[arg(long)]
    allow_legacy_hash_only: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if !cli.speed.is_finite() || cli.speed <= 0.0 {
        bail!("--speed must be a finite value greater than zero");
    }
    if cli.trace_every == 0 {
        bail!("--trace-every must be greater than zero");
    }

    match (cli.bundle.clone(), cli.watch.clone()) {
        (Some(bundle_path), None) => replay_bundle(&bundle_path, &cli),
        (None, Some(source_path)) => {
            if cli.allow_legacy_hash_only {
                bail!("--allow-legacy-hash-only applies only to a persisted run bundle");
            }
            replay_watch(source_path, &cli)
        }
        (None, None) => bail!("provide a run bundle or --watch SOURCE.chy"),
        (Some(_), Some(_)) => unreachable!("clap rejects conflicting inputs"),
    }
}

fn replay_bundle(bundle_path: &Path, cli: &Cli) -> Result<()> {
    let bundle = load_bundle(bundle_path, cli.allow_legacy_hash_only)?;
    validate_playback_timing(&bundle.trace, cli.speed)?;
    let surface_index = surface_index(&bundle.scenario.scenario.surfaces, cli.surface.as_deref())?;
    replay(
        Some(bundle),
        cli.paused,
        surface_index,
        cli.surface.as_deref(),
        cli.speed,
        &cli.snapshot_dir,
        None,
    )
}

fn replay_watch(source_path: PathBuf, cli: &Cli) -> Result<()> {
    replay(
        None,
        cli.paused,
        0,
        cli.surface.as_deref(),
        cli.speed,
        &cli.snapshot_dir,
        Some(WatchController::new(source_path, cli.trace_every)),
    )
}

fn load_bundle(bundle_path: &Path, allow_legacy_hash_only: bool) -> Result<RunBundle> {
    let source = fs::read_to_string(bundle_path)
        .with_context(|| format!("reading {}", bundle_path.display()))?;
    let bundle: RunBundle = serde_json::from_str(&source)
        .with_context(|| format!("parsing {}", bundle_path.display()))?;
    match verify_run_bundle(&bundle)? {
        BundleVerification::Reconstructed => {}
        BundleVerification::HashOnlyLegacy if allow_legacy_hash_only => {
            eprintln!(
                "warning: bundle uses an incompatible runtime contract; only its hash was verified"
            );
        }
        BundleVerification::HashOnlyLegacy => {
            bail!(
                "bundle uses an incompatible runtime contract and cannot be reconstructed; pass --allow-legacy-hash-only only to display its hash"
            );
        }
    }
    if bundle.trace.is_empty() {
        bail!("bundle contains no trace frames");
    }
    Ok(bundle)
}

#[allow(clippy::too_many_lines)] // UI input, hot-reload state, and framebuffer presentation share one event loop
fn replay(
    mut bundle: Option<RunBundle>,
    mut paused: bool,
    mut surface_index: usize,
    initial_surface: Option<&str>,
    speed: f64,
    snapshot_dir: &Path,
    mut watch: Option<WatchController>,
) -> Result<()> {
    let mut window = Window::new(
        "Chiyoda replay — loading",
        WIDTH,
        HEIGHT,
        WindowOptions::default(),
    )?;
    window.set_target_fps(30);
    let mut buffer = vec![BACKGROUND; WIDTH * HEIGHT];
    let mut index = 0usize;
    let mut last_advance = Instant::now();
    let mut view_mode = ViewMode::Surface;
    let mut snapshot_number = 0_u64;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        if let Some(controller) = &mut watch
            && let Some(update) = controller.tick()
        {
            match update.result {
                Ok(next_bundle) => {
                    if let Err(error) = validate_playback_timing(&next_bundle.trace, speed) {
                        let message = error.to_string();
                        controller.mark_failed(update.revision, &message);
                        eprintln!("watch revision {} failed: {message}", update.revision);
                    } else {
                        let previously_selected_surface = bundle.as_ref().and_then(|current| {
                            current
                                .scenario
                                .scenario
                                .surfaces
                                .get(surface_index)
                                .map(|surface| surface.id.as_str())
                        });
                        match select_reloaded_surface(
                            &next_bundle.scenario.scenario.surfaces,
                            previously_selected_surface,
                            initial_surface,
                        ) {
                            Ok(next_surface_index) => {
                                surface_index = next_surface_index;
                                bundle = Some(next_bundle);
                                index = 0;
                                last_advance = Instant::now();
                                controller.mark_loaded(update.revision);
                                eprintln!("watch revision {} loaded", update.revision);
                            }
                            Err(error) => {
                                let message = format!("view configuration error: {error}");
                                controller.mark_failed(update.revision, &message);
                                eprintln!("watch revision {} failed: {message}", update.revision);
                            }
                        }
                    }
                }
                Err(message) => {
                    controller.mark_failed(update.revision, &message);
                    eprintln!("watch revision {} failed: {message}", update.revision);
                }
            }
        }

        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            paused = !paused;
            last_advance = Instant::now();
        }
        let mut manually_advanced = false;
        if let Some(current_bundle) = &bundle {
            if window.is_key_pressed(Key::Right, KeyRepeat::Yes) {
                index = (index + 1).min(current_bundle.trace.len() - 1);
                manually_advanced = true;
            }
            if window.is_key_pressed(Key::Left, KeyRepeat::Yes) {
                index = index.saturating_sub(1);
                manually_advanced = true;
            }
            if window.is_key_pressed(Key::Tab, KeyRepeat::No) {
                surface_index =
                    (surface_index + 1) % current_bundle.scenario.scenario.surfaces.len();
            }
        }
        if window.is_key_pressed(Key::V, KeyRepeat::No) {
            view_mode = view_mode.toggle();
        }
        if manually_advanced {
            last_advance = Instant::now();
        }
        if let Some(current_bundle) = &bundle
            && !paused
            && index < current_bundle.trace.len() - 1
            && last_advance.elapsed() >= frame_delay(&current_bundle.trace, index, speed)
        {
            index += 1;
            last_advance = Instant::now();
        }
        buffer.fill(BACKGROUND);
        let save_snapshot = window.is_key_pressed(Key::P, KeyRepeat::No);
        if let Some(current_bundle) = &bundle {
            match view_mode {
                ViewMode::Surface => {
                    let surface = &current_bundle.scenario.scenario.surfaces[surface_index];
                    let extent = extent_for_surface(surface);
                    draw_scene(
                        &mut buffer,
                        &current_bundle.scenario.scenario,
                        surface,
                        extent,
                    );
                    draw_frame(&mut buffer, current_bundle, index, &surface.id, extent);
                }
                ViewMode::Overview => {
                    draw_overview(&mut buffer, current_bundle, index);
                }
            }
        }
        if let Some(status) = watch.as_ref().map(WatchController::status) {
            draw_watch_status(&mut buffer, status);
        }
        window.set_title(&window_title(
            bundle.as_ref(),
            surface_index,
            index,
            speed,
            view_mode,
            watch.as_ref().map(WatchController::status),
        ));
        if save_snapshot {
            let step = bundle
                .as_ref()
                .and_then(|current| current.trace.get(index))
                .map_or(0, |frame| frame.step);
            match next_snapshot_path(snapshot_dir, &mut snapshot_number, step) {
                Ok(path) => match write_png(&path, &buffer, WIDTH, HEIGHT) {
                    Ok(()) => eprintln!("wrote PNG snapshot: {}", path.display()),
                    Err(error) => {
                        eprintln!("could not write PNG snapshot {}: {error}", path.display());
                    }
                },
                Err(error) => eprintln!("could not allocate a PNG snapshot path: {error}"),
            }
        }
        window.update_with_buffer(&buffer, WIDTH, HEIGHT)?;
        if paused {
            thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Surface,
    Overview,
}

impl ViewMode {
    fn toggle(self) -> Self {
        match self {
            Self::Surface => Self::Overview,
            Self::Overview => Self::Surface,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Surface => "surface",
            Self::Overview => "overview",
        }
    }
}

fn window_title(
    bundle: Option<&RunBundle>,
    surface_index: usize,
    index: usize,
    speed: f64,
    view_mode: ViewMode,
    watch_status: Option<&str>,
) -> String {
    let controls = "space: pause, arrows: step, tab: surface, V: view, P: PNG, escape: quit";
    let status = watch_status.unwrap_or("verified bundle replay");
    match bundle {
        Some(bundle) => {
            let surface = &bundle.scenario.scenario.surfaces[surface_index];
            format!(
                "Chiyoda {} — {} — {:.3}s — {speed:.2}× — {status} — {controls}",
                view_mode.label(),
                surface.id,
                bundle.trace[index].time_s,
            )
        }
        None => format!("Chiyoda watch — no valid scene — {status} — {controls}"),
    }
}

fn select_reloaded_surface(
    surfaces: &[Surface],
    previous_surface: Option<&str>,
    initial_surface: Option<&str>,
) -> Result<usize> {
    match previous_surface {
        Some(previous_surface) => Ok(surfaces
            .iter()
            .position(|surface| surface.id == previous_surface)
            .unwrap_or(0)),
        None => surface_index(surfaces, initial_surface),
    }
}

#[derive(Debug)]
struct PendingRevision {
    revision: u64,
    source: String,
    ready_at: Instant,
}

#[derive(Debug)]
struct WatchUpdate {
    revision: u64,
    result: std::result::Result<RunBundle, String>,
}

#[derive(Debug)]
struct WatchController {
    source_path: PathBuf,
    trace_every_steps: u32,
    observed_source: Option<String>,
    latest_revision: u64,
    pending: Option<PendingRevision>,
    running_revision: Option<u64>,
    sender: Sender<WatchUpdate>,
    receiver: Receiver<WatchUpdate>,
    last_poll: Instant,
    last_read_error: Option<String>,
    status: String,
}

impl WatchController {
    fn new(source_path: PathBuf, trace_every_steps: u32) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            source_path,
            trace_every_steps,
            observed_source: None,
            latest_revision: 0,
            pending: None,
            running_revision: None,
            sender,
            receiver,
            last_poll: Instant::now()
                .checked_sub(WATCH_POLL_INTERVAL)
                .unwrap_or_else(Instant::now),
            last_read_error: None,
            status: "waiting for the first saved source revision".to_owned(),
        }
    }

    fn tick(&mut self) -> Option<WatchUpdate> {
        let now = Instant::now();
        if now.duration_since(self.last_poll) >= WATCH_POLL_INTERVAL {
            self.last_poll = now;
            self.observe_source(now);
        }

        let mut latest_update = None;
        while let Ok(update) = self.receiver.try_recv() {
            if self.running_revision == Some(update.revision) {
                self.running_revision = None;
            }
            if update.revision == self.latest_revision {
                latest_update = Some(update);
            }
        }
        self.start_ready_revision(now);
        latest_update
    }

    fn observe_source(&mut self, now: Instant) {
        match fs::read_to_string(&self.source_path) {
            Ok(source) => {
                let source_became_readable = self.last_read_error.take().is_some();
                if self.observed_source.as_ref() == Some(&source) {
                    if source_became_readable {
                        self.status = format!(
                            "watch revision {} unchanged after source recovery",
                            self.latest_revision
                        );
                    }
                    return;
                }
                self.observed_source = Some(source.clone());
                self.latest_revision = self.latest_revision.saturating_add(1);
                self.pending = Some(PendingRevision {
                    revision: self.latest_revision,
                    source,
                    ready_at: now + WATCH_DEBOUNCE,
                });
                self.status = format!(
                    "watch revision {} waiting for save debounce",
                    self.latest_revision
                );
            }
            Err(error) => {
                let message = format!("cannot read {}: {error}", self.source_path.display());
                if self.last_read_error.as_deref() != Some(&message) {
                    eprintln!("watch source error: {message}");
                    self.last_read_error = Some(message.clone());
                }
                self.status = message;
            }
        }
    }

    fn start_ready_revision(&mut self, now: Instant) {
        if self.running_revision.is_some() {
            return;
        }
        let Some(pending) = self.pending.take_if(|pending| pending.ready_at <= now) else {
            return;
        };
        let sender = self.sender.clone();
        let trace_every_steps = self.trace_every_steps;
        let revision = pending.revision;
        self.running_revision = Some(revision);
        self.status = format!("watch revision {revision} compiling and running");
        thread::spawn(move || {
            let result = compile_watch_source(&pending.source, trace_every_steps);
            let _ = sender.send(WatchUpdate { revision, result });
        });
    }

    fn mark_loaded(&mut self, revision: u64) {
        self.status = format!("watch revision {revision} loaded");
    }

    fn mark_failed(&mut self, revision: u64, message: &str) {
        self.status = format!(
            "watch revision {revision} failed: {}",
            concise_message(message)
        );
    }

    fn status(&self) -> &str {
        &self.status
    }
}

fn compile_watch_source(
    source: &str,
    trace_every_steps: u32,
) -> std::result::Result<RunBundle, String> {
    let scenario = parse(source).map_err(|error| format!("compile error: {error}"))?;
    validate(&scenario).map_err(|errors| {
        let details = errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        format!("validation error: {details}")
    })?;
    run(&scenario, RunOptions { trace_every_steps })
        .map_err(|error| format!("runtime error: {error}"))
}

fn concise_message(message: &str) -> String {
    const MAXIMUM_CHARS: usize = 120;
    let one_line = message.replace('\n', " ");
    if one_line.chars().count() <= MAXIMUM_CHARS {
        one_line
    } else {
        format!(
            "{}…",
            one_line.chars().take(MAXIMUM_CHARS - 1).collect::<String>()
        )
    }
}

fn draw_watch_status(buffer: &mut [u32], status: &str) {
    const PANEL_X: isize = 28;
    const PANEL_Y: isize = 28;
    const PANEL_WIDTH: isize = 1_144;
    const PANEL_HEIGHT: isize = 122;
    const TEXT_X: isize = PANEL_X + 18;
    const TEXT_Y: isize = PANEL_Y + 16;
    const TEXT_SCALE: isize = 2;
    const MAXIMUM_COLUMNS: usize = 88;
    const MAXIMUM_LINES: usize = 4;

    let error = watch_status_is_error(status);
    draw_pixel_rectangle(
        buffer,
        PANEL_X,
        PANEL_Y,
        PANEL_WIDTH,
        PANEL_HEIGHT,
        WATCH_STATUS_PANEL,
    );
    draw_pixel_rectangle_outline(
        buffer,
        PANEL_X,
        PANEL_Y,
        PANEL_WIDTH,
        PANEL_HEIGHT,
        if error {
            WATCH_ERROR
        } else {
            WATCH_STATUS_BORDER
        },
    );
    draw_bitmap_text(
        buffer,
        TEXT_X,
        TEXT_Y,
        if error { "WATCH ERROR" } else { "WATCH STATUS" },
        if error { WATCH_ERROR } else { WATCH_INFO },
        TEXT_SCALE,
    );
    draw_wrapped_bitmap_text(
        buffer,
        TEXT_X,
        TEXT_Y + 22,
        status,
        BitmapTextLayout {
            color: WATCH_STATUS_TEXT,
            scale: TEXT_SCALE,
            maximum_columns: MAXIMUM_COLUMNS,
            maximum_lines: MAXIMUM_LINES,
        },
    );
}

fn watch_status_is_error(status: &str) -> bool {
    status.starts_with("cannot read") || status.contains(" failed:")
}

fn draw_pixel_rectangle(
    buffer: &mut [u32],
    x: isize,
    y: isize,
    width: isize,
    height: isize,
    color: u32,
) {
    for row in y..y + height {
        for column in x..x + width {
            set_pixel(buffer, column, row, color);
        }
    }
}

fn draw_pixel_rectangle_outline(
    buffer: &mut [u32],
    x: isize,
    y: isize,
    width: isize,
    height: isize,
    color: u32,
) {
    for column in x..x + width {
        set_pixel(buffer, column, y, color);
        set_pixel(buffer, column, y + height - 1, color);
    }
    for row in y..y + height {
        set_pixel(buffer, x, row, color);
        set_pixel(buffer, x + width - 1, row, color);
    }
}

#[derive(Debug, Clone, Copy)]
struct BitmapTextLayout {
    color: u32,
    scale: isize,
    maximum_columns: usize,
    maximum_lines: usize,
}

fn draw_wrapped_bitmap_text(
    buffer: &mut [u32],
    x: isize,
    y: isize,
    text: &str,
    layout: BitmapTextLayout,
) {
    let mut column = 0_usize;
    let mut line = 0_usize;
    let advance_x = 6 * layout.scale;
    let advance_y = 9 * layout.scale;
    for character in text.chars() {
        if character == '\n' || column == layout.maximum_columns {
            line += 1;
            column = 0;
            if line == layout.maximum_lines {
                return;
            }
            if character == '\n' {
                continue;
            }
        }
        draw_bitmap_glyph(
            buffer,
            x + advance_x * column.cast_signed(),
            y + advance_y * line.cast_signed(),
            character,
            layout.color,
            layout.scale,
        );
        column += 1;
    }
}

fn draw_bitmap_text(buffer: &mut [u32], x: isize, y: isize, text: &str, color: u32, scale: isize) {
    for (index, character) in text.chars().enumerate() {
        draw_bitmap_glyph(
            buffer,
            x + 6 * scale * index.cast_signed(),
            y,
            character,
            color,
            scale,
        );
    }
}

fn draw_bitmap_glyph(
    buffer: &mut [u32],
    x: isize,
    y: isize,
    character: char,
    color: u32,
    scale: isize,
) {
    for (row, bits) in bitmap_glyph(character).iter().enumerate() {
        for column in 0..5 {
            if *bits & (1 << (4 - column)) != 0 {
                draw_pixel_rectangle(
                    buffer,
                    x + column * scale,
                    y + row.cast_signed() * scale,
                    scale,
                    scale,
                    color,
                );
            }
        }
    }
}

#[allow(clippy::too_many_lines)] // one inspectable table keeps the dependency-free overlay readable
fn bitmap_glyph(character: char) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00001, 0b00001, 0b00001, 0b00001, 0b10001, 0b10001, 0b01110,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b11100,
        ],
        ' ' => [0; 7],
        ':' => [0, 0b00100, 0b00100, 0, 0b00100, 0b00100, 0],
        '.' => [0, 0, 0, 0, 0, 0b00100, 0b00100],
        ',' => [0, 0, 0, 0, 0b00100, 0b00100, 0b01000],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111],
        '/' => [0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0, 0],
        '\\' => [0b10000, 0b01000, 0b00100, 0b00010, 0b00001, 0, 0],
        '(' => [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
        ')' => [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
        '[' => [
            0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110,
        ],
        ']' => [
            0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110,
        ],
        '=' => [0, 0b11111, 0, 0b11111, 0, 0, 0],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100],
        _ => [0b01110, 0b10001, 0b00010, 0b00100, 0b01000, 0, 0b01000],
    }
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
    draw_queue_footprints(buffer, scenario, surface, extent);
    for waypoint in &scenario.waypoints {
        if waypoint.surface == surface.id {
            draw_marker(buffer, waypoint.at.x_m, waypoint.at.y_m, WAYPOINT, extent);
        }
    }
    draw_portal_lanes(buffer, scenario, surface, extent);
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

fn draw_queue_footprints(
    buffer: &mut [u32],
    scenario: &Scenario,
    surface: &Surface,
    extent: (f64, f64, f64, f64),
) {
    for footprint in &scenario.queue_footprints {
        if footprint.surface != surface.id {
            continue;
        }
        if footprint.width_m.is_some() {
            for rank in 1..footprint.slots {
                draw_line(
                    buffer,
                    footprint.position(rank - 1),
                    footprint.position(rank),
                    QUEUE_FOOTPRINT,
                    extent,
                );
            }
        } else {
            draw_line(
                buffer,
                footprint.head,
                footprint.tail,
                QUEUE_FOOTPRINT,
                extent,
            );
        }
        for rank in 0..footprint.slots {
            let slot = footprint.position(rank);
            let x = project(slot.x_m, extent.0, extent.1, WIDTH);
            let y = project(slot.y_m, extent.2, extent.3, HEIGHT);
            draw_square(buffer, x, y, 1, QUEUE_FOOTPRINT);
        }
    }
}

fn draw_portal_lanes(
    buffer: &mut [u32],
    scenario: &Scenario,
    surface: &Surface,
    extent: (f64, f64, f64, f64),
) {
    for lanes in &scenario.portal_lanes {
        match &lanes.resource {
            chiyoda_core::model::PortalResource::Connector { id } => {
                let Some(connector) = scenario
                    .connectors
                    .iter()
                    .find(|connector| connector.id() == id)
                else {
                    continue;
                };
                if connector.from_surface() == surface.id {
                    draw_lane_positions(
                        buffer,
                        lanes,
                        connector.from(),
                        connector.width_m(),
                        extent,
                    );
                }
                if connector.to_surface() == surface.id {
                    draw_lane_positions(buffer, lanes, connector.to(), connector.width_m(), extent);
                }
            }
            chiyoda_core::model::PortalResource::Exit { id } => {
                let Some(exit) = scenario.exits.iter().find(|exit| &exit.id == id) else {
                    continue;
                };
                if exit.surface == surface.id {
                    draw_lane_positions(buffer, lanes, exit.at, exit.width_m, extent);
                }
            }
            chiyoda_core::model::PortalResource::Gate { id } => {
                let Some(gate) = scenario.gates.iter().find(|gate| &gate.id == id) else {
                    continue;
                };
                if gate.surface == surface.id {
                    draw_lane_positions(buffer, lanes, gate.at, gate.width_m, extent);
                }
            }
        }
    }
}

fn draw_lane_positions(
    buffer: &mut [u32],
    lanes: &PortalLanes,
    portal: chiyoda_core::model::Point3,
    width_m: f64,
    extent: (f64, f64, f64, f64),
) {
    for lane_index in 0..lanes.count {
        let lane = lanes.position(portal, width_m, lane_index);
        let x = project(lane.x_m, extent.0, extent.1, WIDTH);
        let y = project(lane.y_m, extent.2, extent.3, HEIGHT);
        draw_square(buffer, x, y, 1, PORTAL_LANE);
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
        let color = agent_color(&agent.state);
        draw_square(buffer, x, y, 2, color);
    }
}

fn agent_color(state: &AgentState) -> u32 {
    match state {
        AgentState::Moving => MOVING,
        AgentState::WaitingToDepart
        | AgentState::WaitingAtWaypoint
        | AgentState::WaitingForRoute => WAITING_TO_DEPART,
        AgentState::WaitingForLift
        | AgentState::WaitingForConnector
        | AgentState::WaitingForGate
        | AgentState::WaitingForExit
        | AgentState::InTransit => IN_TRANSIT,
        AgentState::Evacuated => EVACUATED,
    }
}

#[derive(Debug, Clone, Copy)]
struct OverviewExtent {
    min_u: f64,
    span_u: f64,
    min_v: f64,
    span_v: f64,
}

fn draw_overview(buffer: &mut [u32], bundle: &RunBundle, index: usize) {
    let scenario = &bundle.scenario.scenario;
    let extent = overview_extent(scenario);
    let mut surfaces = scenario.surfaces.iter().collect::<Vec<_>>();
    surfaces.sort_by(|left, right| left.origin.z_m.total_cmp(&right.origin.z_m));
    for surface in surfaces {
        draw_overview_surface(buffer, surface, extent);
    }
    for obstacle in &scenario.obstacles {
        draw_overview_rectangle(
            buffer,
            obstacle.at,
            obstacle.width_m,
            obstacle.depth_m,
            OBSTACLE,
            extent,
        );
    }
    for footprint in &scenario.queue_footprints {
        if footprint.width_m.is_some() {
            for rank in 1..footprint.slots {
                draw_overview_line(
                    buffer,
                    footprint.position(rank - 1),
                    footprint.position(rank),
                    QUEUE_FOOTPRINT,
                    extent,
                );
            }
        } else {
            draw_overview_line(
                buffer,
                footprint.head,
                footprint.tail,
                QUEUE_FOOTPRINT,
                extent,
            );
        }
        for rank in 0..footprint.slots {
            draw_overview_marker(buffer, footprint.position(rank), QUEUE_FOOTPRINT, extent);
        }
    }
    for waypoint in &scenario.waypoints {
        draw_overview_marker(buffer, waypoint.at, WAYPOINT, extent);
    }
    draw_overview_portal_lanes(buffer, scenario, extent);
    for exit in &scenario.exits {
        draw_overview_marker(buffer, exit.at, EXIT, extent);
    }
    for gate in &scenario.gates {
        draw_overview_marker(buffer, gate.at, GATE, extent);
    }
    for connector in &scenario.connectors {
        let color = match connector.kind() {
            ConnectorKind::Stair => STAIR,
            ConnectorKind::Ramp => RAMP,
            ConnectorKind::Escalator => ESCALATOR,
            ConnectorKind::Lift => LIFT,
        };
        draw_overview_line(buffer, connector.from(), connector.to(), color, extent);
        draw_overview_marker(buffer, connector.from(), color, extent);
        draw_overview_marker(buffer, connector.to(), color, extent);
    }
    for agent in &bundle.trace[index].agents {
        draw_overview_marker(
            buffer,
            Point3 {
                x_m: agent.x_m,
                y_m: agent.y_m,
                z_m: agent.z_m,
            },
            agent_color(&agent.state),
            extent,
        );
    }
}

fn overview_extent(scenario: &Scenario) -> OverviewExtent {
    let mut coordinates = Vec::new();
    for surface in &scenario.surfaces {
        coordinates.extend([
            surface.origin,
            Point3 {
                x_m: surface.origin.x_m + surface.width_m,
                y_m: surface.origin.y_m,
                z_m: surface.origin.z_m,
            },
            Point3 {
                x_m: surface.origin.x_m + surface.width_m,
                y_m: surface.origin.y_m + surface.depth_m,
                z_m: surface.origin.z_m,
            },
            Point3 {
                x_m: surface.origin.x_m,
                y_m: surface.origin.y_m + surface.depth_m,
                z_m: surface.origin.z_m,
            },
        ]);
    }
    for connector in &scenario.connectors {
        coordinates.extend([connector.from(), connector.to()]);
    }
    let (mut min_u, mut max_u, mut min_v, mut max_v) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for point in coordinates {
        let (u, v) = isometric_coordinates(point);
        min_u = min_u.min(u);
        max_u = max_u.max(u);
        min_v = min_v.min(v);
        max_v = max_v.max(v);
    }
    let span_u = (max_u - min_u).max(1.0);
    let span_v = (max_v - min_v).max(1.0);
    let padding = span_u.max(span_v) * 0.08;
    OverviewExtent {
        min_u: min_u - padding,
        span_u: span_u + padding * 2.0,
        min_v: min_v - padding,
        span_v: span_v + padding * 2.0,
    }
}

fn isometric_coordinates(point: Point3) -> (f64, f64) {
    (
        point.x_m - point.y_m,
        point.x_m.midpoint(point.y_m) - point.z_m,
    )
}

fn project_overview(point: Point3, extent: OverviewExtent) -> (isize, isize) {
    let (u, v) = isometric_coordinates(point);
    (
        project(u, extent.min_u, extent.span_u, WIDTH),
        project(v, extent.min_v, extent.span_v, HEIGHT),
    )
}

fn draw_overview_surface(buffer: &mut [u32], surface: &Surface, extent: OverviewExtent) {
    draw_overview_rectangle(
        buffer,
        surface.origin,
        surface.width_m,
        surface.depth_m,
        SURFACE,
        extent,
    );
    let corners = overview_rectangle_corners(surface.origin, surface.width_m, surface.depth_m);
    for (from, to) in corners
        .iter()
        .copied()
        .zip(corners.iter().copied().cycle().skip(1))
        .take(corners.len())
    {
        draw_overview_line(buffer, from, to, SURFACE_BORDER, extent);
    }
}

fn draw_overview_rectangle(
    buffer: &mut [u32],
    origin: Point3,
    width_m: f64,
    depth_m: f64,
    color: u32,
    extent: OverviewExtent,
) {
    let corners = overview_rectangle_corners(origin, width_m, depth_m).map(|point| {
        let (x, y) = project_overview(point, extent);
        (x, y)
    });
    draw_triangle(buffer, corners[0], corners[1], corners[2], color);
    draw_triangle(buffer, corners[0], corners[2], corners[3], color);
}

fn overview_rectangle_corners(origin: Point3, width_m: f64, depth_m: f64) -> [Point3; 4] {
    [
        origin,
        Point3 {
            x_m: origin.x_m + width_m,
            y_m: origin.y_m,
            z_m: origin.z_m,
        },
        Point3 {
            x_m: origin.x_m + width_m,
            y_m: origin.y_m + depth_m,
            z_m: origin.z_m,
        },
        Point3 {
            x_m: origin.x_m,
            y_m: origin.y_m + depth_m,
            z_m: origin.z_m,
        },
    ]
}

fn draw_overview_portal_lanes(buffer: &mut [u32], scenario: &Scenario, extent: OverviewExtent) {
    for lanes in &scenario.portal_lanes {
        match &lanes.resource {
            PortalResource::Connector { id } => {
                let Some(connector) = scenario
                    .connectors
                    .iter()
                    .find(|connector| connector.id() == id)
                else {
                    continue;
                };
                draw_overview_lane_positions(
                    buffer,
                    lanes,
                    connector.from(),
                    connector.width_m(),
                    extent,
                );
                draw_overview_lane_positions(
                    buffer,
                    lanes,
                    connector.to(),
                    connector.width_m(),
                    extent,
                );
            }
            PortalResource::Exit { id } => {
                let Some(exit) = scenario.exits.iter().find(|exit| &exit.id == id) else {
                    continue;
                };
                draw_overview_lane_positions(buffer, lanes, exit.at, exit.width_m, extent);
            }
            PortalResource::Gate { id } => {
                let Some(gate) = scenario.gates.iter().find(|gate| &gate.id == id) else {
                    continue;
                };
                draw_overview_lane_positions(buffer, lanes, gate.at, gate.width_m, extent);
            }
        }
    }
}

fn draw_overview_lane_positions(
    buffer: &mut [u32],
    lanes: &PortalLanes,
    portal: Point3,
    width_m: f64,
    extent: OverviewExtent,
) {
    for lane_index in 0..lanes.count {
        draw_overview_marker(
            buffer,
            lanes.position(portal, width_m, lane_index),
            PORTAL_LANE,
            extent,
        );
    }
}

fn draw_overview_marker(buffer: &mut [u32], point: Point3, color: u32, extent: OverviewExtent) {
    let (x, y) = project_overview(point, extent);
    draw_square(buffer, x, y, 3, color);
}

fn draw_overview_line(
    buffer: &mut [u32],
    from: Point3,
    to: Point3,
    color: u32,
    extent: OverviewExtent,
) {
    let (from_x, from_y) = project_overview(from, extent);
    let (to_x, to_y) = project_overview(to, extent);
    draw_pixel_line(buffer, from_x, from_y, to_x, to_y, color);
}

fn draw_triangle(
    buffer: &mut [u32],
    first: (isize, isize),
    second: (isize, isize),
    third: (isize, isize),
    color: u32,
) {
    let min_x = first.0.min(second.0).min(third.0).max(0);
    let max_x = first
        .0
        .max(second.0)
        .max(third.0)
        .min(WIDTH.cast_signed() - 1);
    let min_y = first.1.min(second.1).min(third.1).max(0);
    let max_y = first
        .1
        .max(second.1)
        .max(third.1)
        .min(HEIGHT.cast_signed() - 1);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let first_side = triangle_side(first, second, (x, y));
            let second_side = triangle_side(second, third, (x, y));
            let third_side = triangle_side(third, first, (x, y));
            if (first_side >= 0 && second_side >= 0 && third_side >= 0)
                || (first_side <= 0 && second_side <= 0 && third_side <= 0)
            {
                set_pixel(buffer, x, y, color);
            }
        }
    }
}

fn triangle_side(from: (isize, isize), to: (isize, isize), point: (isize, isize)) -> isize {
    (point.0 - from.0) * (to.1 - from.1) - (point.1 - from.1) * (to.0 - from.0)
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

fn draw_line(
    buffer: &mut [u32],
    from: chiyoda_core::model::Point3,
    to: chiyoda_core::model::Point3,
    color: u32,
    extent: (f64, f64, f64, f64),
) {
    let from_x = project(from.x_m, extent.0, extent.1, WIDTH);
    let from_y = project(from.y_m, extent.2, extent.3, HEIGHT);
    let to_x = project(to.x_m, extent.0, extent.1, WIDTH);
    let to_y = project(to.y_m, extent.2, extent.3, HEIGHT);
    draw_pixel_line(buffer, from_x, from_y, to_x, to_y, color);
}

fn draw_pixel_line(
    buffer: &mut [u32],
    mut x: isize,
    mut y: isize,
    end_x: isize,
    end_y: isize,
    color: u32,
) {
    let delta_x = (end_x - x).abs();
    let step_x = if x < end_x { 1 } else { -1 };
    let delta_y = -(end_y - y).abs();
    let step_y = if y < end_y { 1 } else { -1 };
    let mut error = delta_x + delta_y;
    loop {
        set_pixel(buffer, x, y, color);
        if x == end_x && y == end_y {
            break;
        }
        let double_error = 2 * error;
        if double_error >= delta_y {
            error += delta_y;
            x += step_x;
        }
        if double_error <= delta_x {
            error += delta_x;
            y += step_y;
        }
    }
}

fn snapshot_path(directory: &Path, snapshot_number: u64, step: u64) -> PathBuf {
    directory.join(format!("snapshot-{snapshot_number:04}-step-{step}.png"))
}

fn next_snapshot_path(directory: &Path, snapshot_number: &mut u64, step: u64) -> Result<PathBuf> {
    loop {
        *snapshot_number = snapshot_number
            .checked_add(1)
            .context("PNG snapshot number overflowed")?;
        let path = snapshot_path(directory, *snapshot_number, step);
        if !path.exists() {
            return Ok(path);
        }
    }
}

fn write_png(path: &Path, buffer: &[u32], width: usize, height: usize) -> Result<()> {
    if buffer.len() != width.saturating_mul(height) {
        bail!(
            "framebuffer contains {} pixels but {width}×{height} pixels were expected",
            buffer.len()
        );
    }
    let scanline_bytes = width
        .checked_mul(3)
        .context("PNG row byte count overflowed")?;
    let raw_capacity = scanline_bytes
        .checked_add(1)
        .and_then(|row| row.checked_mul(height))
        .context("PNG image byte count overflowed")?;
    let mut raw = Vec::with_capacity(raw_capacity);
    for row in buffer.chunks_exact(width) {
        raw.push(0);
        for pixel in row {
            let bytes = pixel.to_be_bytes();
            raw.extend_from_slice(&bytes[1..]);
        }
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&raw)?;
    let compressed = encoder.finish()?;
    let width = u32::try_from(width).context("PNG width does not fit in u32")?;
    let height = u32::try_from(height).context("PNG height does not fit in u32")?;
    let mut header = Vec::with_capacity(13);
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .with_context(|| format!("creating snapshot directory {}", directory.display()))?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("creating PNG snapshot {}", path.display()))?;
    file.write_all(b"\x89PNG\r\n\x1a\n")?;
    write_png_chunk(&mut file, *b"IHDR", &header)?;
    write_png_chunk(&mut file, *b"IDAT", &compressed)?;
    write_png_chunk(&mut file, *b"IEND", &[])?;
    Ok(())
}

fn write_png_chunk(file: &mut fs::File, kind: [u8; 4], data: &[u8]) -> Result<()> {
    let length = u32::try_from(data.len()).context("PNG chunk exceeds u32 length")?;
    file.write_all(&length.to_be_bytes())?;
    file.write_all(&kind)?;
    file.write_all(data)?;
    let mut crc_data = Vec::with_capacity(kind.len() + data.len());
    crc_data.extend(kind);
    crc_data.extend(data);
    file.write_all(&png_crc32(&crc_data).to_be_bytes())?;
    Ok(())
}

fn png_crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
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
        model::{
            Connector, Exit, Gate, Obstacle, Point3, PortalAxis, PortalLanes, PortalResource,
            QueueFootprint, Scenario, Waypoint,
        },
    };
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    const WATCH_SOURCE: &str = r#"
scenario "watch fixture"
seed 7
duration 2s
timestep 0.5s
surface concourse at (0m, 0m, 0m) size (12m, 8m)
exit street on concourse at (10m, 4m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 4m, 0m) to street speed 1.2m/s radius 0.3m height 1.7m
"#;

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
    #[allow(clippy::too_many_lines)] // one complete static-scene fixture makes the layer order explicit
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
            portal_lanes: vec![PortalLanes {
                id: "stairs_lanes".to_owned(),
                resource: PortalResource::Connector {
                    id: "stairs".to_owned(),
                },
                axis: PortalAxis::X,
                count: 2,
            }],
            queue_footprints: vec![QueueFootprint {
                id: "street_queue".to_owned(),
                resource: PortalResource::Exit {
                    id: "street".to_owned(),
                },
                surface: "concourse".to_owned(),
                head: Point3 {
                    x_m: 8.0,
                    y_m: 8.0,
                    z_m: 0.0,
                },
                tail: Point3 {
                    x_m: 9.0,
                    y_m: 8.0,
                    z_m: 0.0,
                },
                slots: 4,
                width_m: Some(1.0),
                lanes: Some(2),
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
        assert_eq!(buffer[location(5.75, 6.0)], PORTAL_LANE);
        assert_eq!(buffer[location(8.0, 8.0)], QUEUE_FOOTPRINT);
        assert_eq!(buffer[location(8.0, 9.0)], QUEUE_FOOTPRINT);
        assert_eq!(buffer[location(9.0, 8.5)], QUEUE_FOOTPRINT);
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

    #[test]
    fn watch_compilation_matches_an_explicit_in_memory_run() {
        let watched = compile_watch_source(WATCH_SOURCE, 1).expect("watch fixture succeeds");
        let scenario = parse(WATCH_SOURCE).expect("watch fixture parses");
        let expected = run(
            &scenario,
            RunOptions {
                trace_every_steps: 1,
            },
        )
        .expect("explicit run succeeds");

        assert_eq!(watched.bundle_hash, expected.bundle_hash);
        assert_eq!(watched.trace, expected.trace);
        assert_eq!(watched.metrics, expected.metrics);
    }

    #[test]
    fn watch_compilation_keeps_compile_and_validation_failures_explicit() {
        let missing_scenario = compile_watch_source("seed 1\n", 1).unwrap_err();
        assert!(missing_scenario.starts_with("compile error:"));

        let invalid = WATCH_SOURCE.replace("duration 2s", "duration 0s");
        let invalid_result = compile_watch_source(&invalid, 1).unwrap_err();
        assert!(invalid_result.starts_with("validation error:"));
    }

    #[test]
    fn stale_watch_worker_result_is_discarded() {
        let path = std::env::temp_dir().join(format!(
            "chiyoda-replay-watch-stale-{}-{}.chy",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let mut controller = WatchController::new(path, 1);
        controller.latest_revision = 2;
        controller.running_revision = Some(1);
        controller
            .sender
            .send(WatchUpdate {
                revision: 1,
                result: Err("stale failure".to_owned()),
            })
            .expect("test receiver is live");

        assert!(controller.tick().is_none());
        assert_eq!(controller.running_revision, None);
        assert_eq!(controller.latest_revision, 2);
    }

    #[test]
    fn reload_surface_selection_prefers_the_existing_surface_then_the_requested_one() {
        let surfaces = [
            surface("concourse", 0.0, 0.0, 10.0, 10.0),
            surface("platform", 0.0, 0.0, 10.0, 10.0),
        ];
        assert_eq!(
            select_reloaded_surface(&surfaces, Some("platform"), Some("concourse"))
                .expect("existing surface remains selected"),
            1,
        );
        assert_eq!(
            select_reloaded_surface(&surfaces, None, Some("concourse"))
                .expect("requested surface is selected"),
            0,
        );
        assert_eq!(
            select_reloaded_surface(&surfaces, Some("missing"), None)
                .expect("a deleted selected surface falls back to the first one"),
            0,
        );
        assert!(select_reloaded_surface(&surfaces, None, Some("missing")).is_err());
    }

    #[test]
    fn overview_renderer_draws_static_and_trace_layers_without_a_display_server() {
        let bundle = compile_watch_source(WATCH_SOURCE, 1).expect("watch fixture succeeds");
        let mut buffer = vec![BACKGROUND; WIDTH * HEIGHT];
        draw_overview(&mut buffer, &bundle, 0);

        assert!(buffer.contains(&SURFACE));
        assert!(buffer.contains(&SURFACE_BORDER));
        assert!(buffer.contains(&EXIT));
    }

    #[test]
    fn watch_error_is_visible_in_the_rendered_canvas() {
        let mut buffer = vec![BACKGROUND; WIDTH * HEIGHT];
        draw_watch_status(
            &mut buffer,
            "cannot read draft.chy: No such file or directory (os error 2)",
        );

        assert!(watch_status_is_error(
            "cannot read draft.chy: No such file or directory"
        ));
        assert!(buffer.contains(&WATCH_STATUS_PANEL));
        assert!(buffer.contains(&WATCH_ERROR));
        assert!(buffer.contains(&WATCH_STATUS_TEXT));
    }

    #[test]
    fn png_snapshot_encodes_rgb_frame_data() {
        let directory = std::env::temp_dir().join(format!(
            "chiyoda-replay-png-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let path = snapshot_path(&directory, 1, 4);
        write_png(&path, &[0x0011_2233, 0x0044_5566], 2, 1).expect("PNG writes");
        let encoded = fs::read(&path).expect("PNG reads");

        assert_eq!(&encoded[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&encoded[12..16], b"IHDR");
        assert_eq!(&encoded[16..20], &2_u32.to_be_bytes());
        assert_eq!(&encoded[20..24], &1_u32.to_be_bytes());
        let idat_offset = 8 + 4 + 4 + 13 + 4;
        let idat_length =
            u32::from_be_bytes(encoded[idat_offset..idat_offset + 4].try_into().unwrap());
        assert_eq!(&encoded[idat_offset + 4..idat_offset + 8], b"IDAT");
        let idat_start = idat_offset + 8;
        let idat_end = idat_start + usize::try_from(idat_length).expect("u32 fits in usize");
        let mut decoded = Vec::new();
        ZlibDecoder::new(&encoded[idat_start..idat_end])
            .read_to_end(&mut decoded)
            .expect("PNG image data decompresses");
        assert_eq!(decoded, [0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        fs::remove_dir_all(directory).expect("test snapshot directory is removable");
    }

    #[test]
    fn snapshot_paths_do_not_reuse_existing_exports() {
        let directory = std::env::temp_dir().join(format!(
            "chiyoda-replay-snapshot-path-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        fs::create_dir_all(&directory).expect("test snapshot directory is created");
        let existing = snapshot_path(&directory, 1, 4);
        fs::write(&existing, "existing snapshot").expect("existing snapshot is written");
        let mut snapshot_number = 0;

        let next = next_snapshot_path(&directory, &mut snapshot_number, 4)
            .expect("a fresh snapshot path is selected");
        assert_eq!(next, snapshot_path(&directory, 2, 4));
        assert_eq!(snapshot_number, 2);
        fs::remove_dir_all(directory).expect("test snapshot directory is removable");
    }

    #[test]
    fn png_writer_does_not_overwrite_an_existing_snapshot() {
        let directory = std::env::temp_dir().join(format!(
            "chiyoda-replay-no-overwrite-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let path = snapshot_path(&directory, 1, 4);
        write_png(&path, &[BACKGROUND], 1, 1).expect("initial PNG writes");
        assert!(write_png(&path, &[BACKGROUND], 1, 1).is_err());
        fs::remove_dir_all(directory).expect("test snapshot directory is removable");
    }

    #[test]
    fn png_snapshot_rejects_wrong_framebuffer_dimensions() {
        let error = write_png(Path::new("unused.png"), &[BACKGROUND], 2, 1).unwrap_err();
        assert!(error.to_string().contains("framebuffer contains"));
    }
}
