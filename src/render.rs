use macroquad::models::{draw_cube_wires, draw_mesh, Mesh, Vertex};
use macroquad::prelude::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cli::RuntimeOptions;
use crate::config::GenerationConfig;
use crate::generation::{
    biome_rust_at, clampf, hash_noise, is_walkable_floor_cell, mix_color, simplex, CellType,
    MaterialType, MegaStructureGenerator, CAMERA_FOV_DEGREES, CHUNK_SIZE_X, CHUNK_SIZE_Y,
    CHUNK_SIZE_Z, DISTRICTS, MATERIALS,
};
use crate::rules::{CompiledRulePackSet, RulePackDocument};
use crate::scenario::{generate_scenario, ScenarioRecord};
use crate::seed::generate_seed;
use crate::structure::{self, SavedStructure, StructureMetadata};

const RULE_EDITOR_WEIGHT_COUNT: usize = 9;

struct SpatialChunk {
    mesh: Mesh,
    center: Vec3,
}

struct RenderWorld {
    opaque_chunks: Vec<SpatialChunk>,
    translucent_chunks: Vec<SpatialChunk>,
}

struct SemanticLabel {
    text: String,
    position: Vec3,
    color: Color,
    priority: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OverlayMode {
    None,
    Transit,
    Districts,
    Strata,
    Entities,
    Typology,
    Construction,
    Stress,
    Section,
    Scenario,
    Debug,
}

impl OverlayMode {
    fn name(self) -> &'static str {
        match self {
            Self::None => "off",
            Self::Transit => "transit",
            Self::Districts => "districts",
            Self::Strata => "strata",
            Self::Entities => "entities",
            Self::Typology => "typology_frame",
            Self::Construction => "construction",
            Self::Stress => "stress",
            Self::Section => "section_quality",
            Self::Scenario => "scenario",
            Self::Debug => "debug",
        }
    }

    fn next_v22(self) -> Self {
        match self {
            Self::Typology => Self::Construction,
            Self::Construction => Self::Stress,
            Self::Stress => Self::Section,
            Self::Section => Self::Scenario,
            Self::Scenario => Self::None,
            _ => Self::Typology,
        }
    }
}

fn cell_style(
    generator: &MegaStructureGenerator,
    x: usize,
    z: usize,
    y: usize,
    cell: CellType,
) -> (Color, bool) {
    let district = generator.district_at(x, z);
    let district_props = DISTRICTS[district as usize];
    let visual_material = generator.visual_material_at(x, z, y, cell);
    let mut style = MATERIALS[visual_material as usize];
    let base_tint = district_props.color_palette[(x + z + y) % district_props.color_palette.len()];
    let noise = simplex::noise3(x as f32 * 0.12, y as f32 * 0.12, z as f32 * 0.12) * 0.5 + 0.5;
    let patina = hash_noise(generator.seed_hash, x, z, y);
    let mut color = mix_color(style.base_color, base_tint, 0.16 + noise * 0.10);

    if visual_material == MaterialType::Neon {
        style = MATERIALS[MaterialType::Neon as usize];
        color = match ((x + y + z) % 3) as i32 {
            0 => (0.10, 0.92, 0.96),
            1 => (0.92, 0.20, 0.84),
            _ => (0.95, 0.92, 0.20),
        };
    } else {
        let decay = biome_rust_at(y) * (0.06 + patina * 0.08);
        color = (
            clampf(color.0 * (1.0 - decay), 0.0, 1.0),
            clampf(color.1 * (1.0 - decay * 0.9), 0.0, 1.0),
            clampf(color.2 * (1.0 - decay * 0.7), 0.0, 1.0),
        );
    }

    if matches!(cell, CellType::Pipe | CellType::Cable) {
        color = mix_color(
            color,
            MATERIALS[MaterialType::Rust as usize].base_color,
            0.30,
        );
    }

    let is_translucent = style.alpha < 0.99;
    (
        Color::new(
            clampf(color.0, 0.0, 1.0),
            clampf(color.1, 0.0, 1.0),
            clampf(color.2, 0.0, 1.0),
            style.alpha,
        ),
        is_translucent,
    )
}

fn face_vertex(position: Vec3, uv: Vec2, color: Color) -> Vertex {
    Vertex::new2(position, uv, color)
}

fn push_face(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    quad: [Vec3; 4],
    color: Color,
    brightness: f32,
) {
    let base = vertices.len() as u16;
    let shaded = Color::new(
        clampf(color.r * brightness, 0.0, 1.0),
        clampf(color.g * brightness, 0.0, 1.0),
        clampf(color.b * brightness, 0.0, 1.0),
        color.a,
    );
    vertices.push(face_vertex(quad[0], vec2(0.0, 0.0), shaded));
    vertices.push(face_vertex(quad[1], vec2(1.0, 0.0), shaded));
    vertices.push(face_vertex(quad[2], vec2(1.0, 1.0), shaded));
    vertices.push(face_vertex(quad[3], vec2(0.0, 1.0), shaded));
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn face_brightness(normal: Vec3) -> f32 {
    let sun = vec3(0.5, 1.0, 0.3).normalize();
    let fill = vec3(-0.3, 0.5, -0.7).normalize();
    let direct = normal.dot(sun).max(0.0);
    let secondary = normal.dot(fill).max(0.0) * 0.3;
    0.32 + direct * 0.55 + secondary
}

fn build_mesh_chunk(
    generator: &MegaStructureGenerator,
    ox: usize,
    oz: usize,
    oy: usize,
    translucent: bool,
) -> Option<SpatialChunk> {
    let max_x = (ox + CHUNK_SIZE_X).min(generator.size);
    let max_z = (oz + CHUNK_SIZE_Z).min(generator.size);
    let max_y = (oy + CHUNK_SIZE_Y).min(generator.layers);

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let directions = [
        (1isize, 0isize, 0isize, vec3(1.0, 0.0, 0.0)),
        (-1, 0, 0, vec3(-1.0, 0.0, 0.0)),
        (0, 1, 0, vec3(0.0, 1.0, 0.0)),
        (0, -1, 0, vec3(0.0, -1.0, 0.0)),
        (0, 0, 1, vec3(0.0, 0.0, 1.0)),
        (0, 0, -1, vec3(0.0, 0.0, -1.0)),
    ];

    for x in ox..max_x {
        for z in oz..max_z {
            for y in oy..max_y {
                let cell = generator.get(x, z, y);
                if cell == CellType::Empty {
                    continue;
                }
                let (color, is_translucent) = cell_style(generator, x, z, y, cell);
                if is_translucent != translucent {
                    continue;
                }
                let center = vec3(x as f32, y as f32, z as f32);
                let half = 0.46;
                for (dx, dy, dz, normal) in directions {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    let nz = z as isize + dz;
                    let neighbor = if nx < 0
                        || ny < 0
                        || nz < 0
                        || nx >= generator.size as isize
                        || ny >= generator.layers as isize
                        || nz >= generator.size as isize
                    {
                        CellType::Empty
                    } else {
                        generator.get(nx as usize, nz as usize, ny as usize)
                    };
                    if neighbor != CellType::Empty {
                        let (_, neighbor_translucent) = cell_style(
                            generator,
                            nx.max(0) as usize,
                            nz.max(0) as usize,
                            ny.max(0) as usize,
                            neighbor,
                        );
                        if !neighbor_translucent || is_translucent == neighbor_translucent {
                            continue;
                        }
                    }
                    let quad = match (dx, dy, dz) {
                        (1, 0, 0) => [
                            center + vec3(half, -half, -half),
                            center + vec3(half, -half, half),
                            center + vec3(half, half, half),
                            center + vec3(half, half, -half),
                        ],
                        (-1, 0, 0) => [
                            center + vec3(-half, -half, half),
                            center + vec3(-half, -half, -half),
                            center + vec3(-half, half, -half),
                            center + vec3(-half, half, half),
                        ],
                        (0, 1, 0) => [
                            center + vec3(-half, half, -half),
                            center + vec3(half, half, -half),
                            center + vec3(half, half, half),
                            center + vec3(-half, half, half),
                        ],
                        (0, -1, 0) => [
                            center + vec3(-half, -half, half),
                            center + vec3(half, -half, half),
                            center + vec3(half, -half, -half),
                            center + vec3(-half, -half, -half),
                        ],
                        (0, 0, 1) => [
                            center + vec3(-half, -half, half),
                            center + vec3(-half, half, half),
                            center + vec3(half, half, half),
                            center + vec3(half, -half, half),
                        ],
                        _ => [
                            center + vec3(half, -half, -half),
                            center + vec3(half, half, -half),
                            center + vec3(-half, half, -half),
                            center + vec3(-half, -half, -half),
                        ],
                    };
                    push_face(
                        &mut vertices,
                        &mut indices,
                        quad,
                        color,
                        face_brightness(normal),
                    );
                }
            }
        }
    }

    if vertices.is_empty() {
        return None;
    }

    Some(SpatialChunk {
        mesh: Mesh {
            vertices,
            indices,
            texture: None,
        },
        center: vec3(
            (ox + max_x) as f32 * 0.5,
            (oy + max_y) as f32 * 0.5,
            (oz + max_z) as f32 * 0.5,
        ),
    })
}

fn build_render_world(generator: &MegaStructureGenerator) -> RenderWorld {
    let mut opaque_chunks = Vec::new();
    let mut translucent_chunks = Vec::new();
    for ox in (0..generator.size).step_by(CHUNK_SIZE_X) {
        for oz in (0..generator.size).step_by(CHUNK_SIZE_Z) {
            for oy in (0..generator.layers).step_by(CHUNK_SIZE_Y) {
                if let Some(chunk) = build_mesh_chunk(generator, ox, oz, oy, false) {
                    opaque_chunks.push(chunk);
                }
                if let Some(chunk) = build_mesh_chunk(generator, ox, oz, oy, true) {
                    translucent_chunks.push(chunk);
                }
            }
        }
    }
    RenderWorld {
        opaque_chunks,
        translucent_chunks,
    }
}

struct OrbitalCamera {
    target: Vec3,
    distance: f32,
    angle: f32,
    pitch: f32,
    target_angle: f32,
    target_pitch: f32,
    target_distance: f32,
    angle_velocity: f32,
    pitch_velocity: f32,
    zoom_velocity: f32,
    damping: f32,
    min_distance: f32,
    max_distance: f32,
    position: Vec3,
}

impl OrbitalCamera {
    fn new(target: Vec3, distance: f32) -> Self {
        let mut camera = Self {
            target,
            distance,
            angle: 45.0,
            pitch: 30.0,
            target_angle: 45.0,
            target_pitch: 30.0,
            target_distance: distance,
            angle_velocity: 0.0,
            pitch_velocity: 0.0,
            zoom_velocity: 0.0,
            damping: 0.85,
            min_distance: distance * 0.3,
            max_distance: distance * 5.0,
            position: vec3(0.0, 0.0, 0.0),
        };
        camera.position = camera.calc_position();
        camera
    }

    fn calc_position(&self) -> Vec3 {
        let rad_angle = self.angle.to_radians();
        let rad_pitch = self.pitch.to_radians();
        let x = self.distance * rad_pitch.cos() * rad_angle.cos();
        let y = self.distance * rad_pitch.sin();
        let z = self.distance * rad_pitch.cos() * rad_angle.sin();
        self.target + vec3(x, y, z)
    }

    fn update(&mut self, dt: f32) {
        let ad = self.target_angle - self.angle;
        let pd = self.target_pitch - self.pitch;
        let zd = self.target_distance - self.distance;
        self.angle_velocity += ad * dt * 5.0;
        self.pitch_velocity += pd * dt * 5.0;
        self.zoom_velocity += zd * dt * 3.0;
        self.angle_velocity *= self.damping;
        self.pitch_velocity *= self.damping;
        self.zoom_velocity *= self.damping;
        self.angle += self.angle_velocity * dt;
        self.pitch += self.pitch_velocity * dt;
        self.distance += self.zoom_velocity * dt;
        self.pitch = clampf(self.pitch, -89.0, 89.0);
        self.distance = clampf(self.distance, self.min_distance, self.max_distance);
        self.target_distance = clampf(self.target_distance, self.min_distance, self.max_distance);
        self.position = self.calc_position();
    }

    fn rotate(&mut self, da: f32, dp: f32) {
        self.target_angle += da;
        self.target_pitch += dp;
    }

    fn zoom(&mut self, delta: f32) {
        let bound = self.target.max_element().max(10.0);
        self.min_distance = bound * 0.3;
        self.max_distance = bound * 4.0;
        self.target_distance = clampf(
            self.target_distance + delta,
            self.min_distance,
            self.max_distance,
        );
    }

    fn pan(&mut self, dx: f32, dy: f32) {
        let forward = (self.target - self.position).normalize();
        let right = forward.cross(vec3(0.0, 1.0, 0.0)).normalize();
        let up = right.cross(forward).normalize();
        self.target += right * (dx * 0.12) + up * (dy * 0.12);
    }

    fn set_preset(&mut self, preset: usize) {
        const PRESETS: [(f32, f32); 5] = [
            (0.0, 89.0),
            (0.0, 0.0),
            (90.0, 0.0),
            (45.0, 30.0),
            (45.0, 35.264),
        ];
        let index = preset.min(PRESETS.len() - 1);
        self.target_angle = PRESETS[index].0;
        self.target_pitch = PRESETS[index].1;
        self.angle_velocity = 0.0;
        self.pitch_velocity = 0.0;
    }

    fn view_camera(&self, render_target: Option<RenderTarget>) -> Camera3D {
        Camera3D {
            position: self.position,
            target: self.target,
            up: vec3(0.0, 1.0, 0.0),
            fovy: CAMERA_FOV_DEGREES.to_radians(),
            projection: Projection::Perspective,
            render_target,
            z_near: 0.1,
            z_far: 500.0,
            ..Default::default()
        }
    }
}

struct FpsCamera {
    position: Vec3,
    yaw: f32,
    pitch: f32,
    speed: f32,
    sensitivity: f32,
    velocity_y: f32,
    on_ground: bool,
}

impl FpsCamera {
    const EYE_HEIGHT: f32 = 1.6;
    const RADIUS: f32 = 0.3;
    const GRAVITY: f32 = 9.8;
    const JUMP_VELOCITY: f32 = 5.0;
    const MAX_DELTA: f32 = 0.5;

    fn new(position: Vec3) -> Self {
        Self {
            position,
            yaw: -90.0,
            pitch: 0.0,
            speed: 5.0,
            sensitivity: 0.1,
            velocity_y: 0.0,
            on_ground: false,
        }
    }

    fn front(&self) -> Vec3 {
        let yaw = self.yaw.to_radians();
        let pitch = self.pitch.to_radians();
        vec3(
            yaw.cos() * pitch.cos(),
            pitch.sin(),
            yaw.sin() * pitch.cos(),
        )
        .normalize()
    }

    fn look_delta(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * self.sensitivity;
        self.pitch -= dy * self.sensitivity;
        self.pitch = clampf(self.pitch, -89.0, 89.0);
    }

    fn jump(&mut self) {
        if self.on_ground {
            self.velocity_y = Self::JUMP_VELOCITY;
            self.on_ground = false;
        }
    }

    fn collides_at(&self, position: Vec3, generator: &MegaStructureGenerator) -> bool {
        let offsets = [
            vec2(-Self::RADIUS, -Self::RADIUS),
            vec2(Self::RADIUS, -Self::RADIUS),
            vec2(-Self::RADIUS, Self::RADIUS),
            vec2(Self::RADIUS, Self::RADIUS),
        ];
        for offset in offsets {
            for height in [0.0, Self::EYE_HEIGHT] {
                let gx = (position.x + offset.x).floor() as isize;
                let gy = (position.y + height).floor() as isize;
                let gz = (position.z + offset.y).floor() as isize;
                if gx < 0
                    || gy < 0
                    || gz < 0
                    || gx >= generator.size as isize
                    || gy >= generator.layers as isize
                    || gz >= generator.size as isize
                {
                    continue;
                }
                if generator.get(gx as usize, gz as usize, gy as usize) != CellType::Empty {
                    return true;
                }
            }
        }
        false
    }

    fn update(
        &mut self,
        dt: f32,
        move_forward: f32,
        move_right: f32,
        generator: &MegaStructureGenerator,
    ) {
        let front = self.front();
        let right = front.cross(vec3(0.0, 1.0, 0.0)).normalize();
        let mut movement = (vec3(front.x, 0.0, front.z) * move_forward
            + vec3(right.x, 0.0, right.z) * move_right)
            * self.speed
            * dt;
        let length = movement.length();
        if length > Self::MAX_DELTA {
            movement *= Self::MAX_DELTA / length;
        }

        let mut next = self.position;
        next.x += movement.x;
        if self.collides_at(next, generator) {
            next.x = self.position.x;
        }
        next.z += movement.z;
        if self.collides_at(next, generator) {
            next.z = self.position.z;
        }

        self.velocity_y -= Self::GRAVITY * dt;
        let mut delta_y = self.velocity_y * dt;
        delta_y = clampf(delta_y, -Self::MAX_DELTA, Self::MAX_DELTA);
        next.y += delta_y;
        if self.collides_at(next, generator) {
            if self.velocity_y < 0.0 {
                self.on_ground = true;
            }
            self.velocity_y = 0.0;
            next.y = self.position.y;
        } else {
            self.on_ground = false;
        }
        if next.y < 0.0 {
            next.y = 0.0;
            self.velocity_y = 0.0;
            self.on_ground = true;
        }
        self.position = next;
    }

    fn view_camera(&self, render_target: Option<RenderTarget>) -> Camera3D {
        let eye = self.position + vec3(0.0, Self::EYE_HEIGHT, 0.0);
        Camera3D {
            position: eye,
            target: eye + self.front(),
            up: vec3(0.0, 1.0, 0.0),
            fovy: CAMERA_FOV_DEGREES.to_radians(),
            projection: Projection::Perspective,
            render_target,
            z_near: 0.1,
            z_far: 500.0,
            ..Default::default()
        }
    }
}

fn is_valid_spawn_cell(generator: &MegaStructureGenerator, x: usize, z: usize, y: usize) -> bool {
    if x == 0 || z == 0 || x >= generator.size - 1 || z >= generator.size - 1 {
        return false;
    }
    if y == 0 || y + 1 >= generator.layers {
        return false;
    }
    is_walkable_floor_cell(generator.get(x, z, y - 1))
        && generator.get(x, z, y) == CellType::Empty
        && generator.get(x, z, y + 1) == CellType::Empty
}

fn find_fps_spawn(generator: &MegaStructureGenerator) -> Vec3 {
    let center_x = generator.size as isize / 2;
    let center_z = generator.size as isize / 2;
    let center_y = generator.layers as isize / 3;
    let directions = [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)];
    let mut best_score = i32::MIN;
    let mut best_position = vec3(
        generator.size as f32 / 2.0,
        1.0,
        generator.size as f32 / 2.0,
    );
    for y in 1..generator.layers - 1 {
        for x in 1..generator.size - 1 {
            for z in 1..generator.size - 1 {
                if !is_valid_spawn_cell(generator, x, z, y) {
                    continue;
                }
                let mut floor_links = 0;
                let mut open_links = 0;
                let mut sheltered_links = 0;
                for (dx, dz) in directions {
                    let nx = (x as isize + dx) as usize;
                    let nz = (z as isize + dz) as usize;
                    if is_walkable_floor_cell(generator.get(nx, nz, y - 1)) {
                        floor_links += 1;
                    }
                    if generator.get(nx, nz, y) == CellType::Empty
                        && generator.get(nx, nz, y + 1) == CellType::Empty
                    {
                        open_links += 1;
                    }
                    if generator.get(nx, nz, y - 1) != CellType::Empty
                        || generator.get(nx, nz, y + 1) != CellType::Empty
                    {
                        sheltered_links += 1;
                    }
                }
                let center_penalty = (x as isize - center_x).abs() as i32
                    + (z as isize - center_z).abs() as i32
                    + ((y as isize - center_y).abs() as i32 * 3);
                let score =
                    floor_links * 24 + open_links * 10 + sheltered_links * 4 - center_penalty;
                if score > best_score {
                    best_score = score;
                    best_position = vec3(x as f32 + 0.5, y as f32, z as f32 + 0.5);
                }
            }
        }
    }
    best_position
}

struct PostFxResources {
    scene_target: RenderTarget,
    material: Material,
    width: u32,
    height: u32,
}

impl PostFxResources {
    async fn new(width: u32, height: u32) -> Self {
        let scene_target = render_target_ex(
            width,
            height,
            RenderTargetParams {
                sample_count: 1,
                depth: true,
            },
        );
        scene_target.texture.set_filter(FilterMode::Linear);
        let material = load_material(
            ShaderSource::Glsl {
                vertex: POST_VERTEX,
                fragment: POST_FRAGMENT,
            },
            MaterialParams {
                uniforms: vec![
                    UniformDesc::new("FogDensity", UniformType::Float1),
                    UniformDesc::new("BloomIntensity", UniformType::Float1),
                    UniformDesc::new("Time", UniformType::Float1),
                    UniformDesc::new("ScreenSize", UniformType::Float2),
                ],
                ..Default::default()
            },
        )
        .expect("postfx material");
        Self {
            scene_target,
            material,
            width,
            height,
        }
    }

    async fn ensure_size(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        *self = Self::new(width, height).await;
    }
}

struct AppState {
    generator: MegaStructureGenerator,
    config: GenerationConfig,
    rule_packs: CompiledRulePackSet,
    rule_path: Option<PathBuf>,
    rule_browser: Vec<RuleBrowserEntry>,
    selected_rule_index: usize,
    show_rule_browser: bool,
    rule_status_message: String,
    rule_editor: Option<RulePackDocument>,
    selected_editor_pack: usize,
    selected_editor_weight: usize,
    rule_editor_message: String,
    export_path: PathBuf,
    saved_structure: SavedStructure,
    scenario: ScenarioRecord,
    metadata: StructureMetadata,
    render_world: RenderWorld,
    orbital: OrbitalCamera,
    fps: FpsCamera,
    fps_mode: bool,
    postfx: PostFxResources,
    fog_density: f32,
    bloom_intensity: f32,
    enable_postfx: bool,
    inspection_mode: bool,
    show_legend: bool,
    show_labels: bool,
    overlay_mode: OverlayMode,
    entity_animation_time: f32,
    entity_animation_paused: bool,
    entity_animation_speed_index: usize,
    entity_phase_filter: Option<usize>,
    selected_entity_kind_index: usize,
    hidden_entity_kinds: BTreeSet<String>,
    selected_cell: Option<(usize, usize, usize)>,
    mouse_dragging: bool,
    last_mouse: Option<Vec2>,
    last_fps_mouse: Option<Vec2>,
    screenshot_requested: bool,
}

#[derive(Clone, Debug)]
struct RuleBrowserEntry {
    path: PathBuf,
    name: String,
    valid: bool,
    pack_count: usize,
    grammar_preview: Vec<String>,
    status: String,
}

impl AppState {
    async fn new(
        seed: String,
        config: GenerationConfig,
        rule_packs: CompiledRulePackSet,
        rule_path: Option<PathBuf>,
        export_path: PathBuf,
    ) -> Self {
        let mut generator =
            MegaStructureGenerator::with_config_and_rules(seed, config.clone(), rule_packs.clone());
        generator.generate();
        let saved_structure = generator.saved_structure();
        let scenario = generate_scenario(&saved_structure);
        let metadata = saved_structure.metadata.clone();
        if let Err(error) = save_current_outputs(generator.seed(), &saved_structure, &export_path) {
            eprintln!("Failed to save generated structure: {error}");
        }

        let render_world = build_render_world(&generator);
        let center = vec3(
            generator.size as f32 / 2.0,
            generator.layers as f32 / 2.0,
            generator.size as f32 / 2.0,
        );
        let distance = generator.size.max(generator.layers) as f32 * 1.5;
        let orbital = OrbitalCamera::new(center, distance);
        let fps = FpsCamera::new(find_fps_spawn(&generator));
        let postfx = PostFxResources::new(
            screen_width().max(1.0) as u32,
            screen_height().max(1.0) as u32,
        )
        .await;
        let rule_browser = rule_browser_entries();
        let selected_rule_index = selected_rule_index(&rule_browser, rule_path.as_ref());

        Self {
            generator,
            config,
            rule_packs,
            rule_path,
            rule_browser,
            selected_rule_index,
            show_rule_browser: false,
            rule_status_message: "rule browser ready".to_owned(),
            rule_editor: None,
            selected_editor_pack: 0,
            selected_editor_weight: 0,
            rule_editor_message: "E: edit selected rule file".to_owned(),
            export_path,
            saved_structure,
            scenario,
            metadata,
            render_world,
            orbital,
            fps,
            fps_mode: false,
            postfx,
            fog_density: 0.0,
            bloom_intensity: 0.2,
            enable_postfx: false,
            inspection_mode: false,
            show_legend: true,
            show_labels: false,
            overlay_mode: OverlayMode::None,
            entity_animation_time: 0.0,
            entity_animation_paused: false,
            entity_animation_speed_index: 1,
            entity_phase_filter: None,
            selected_entity_kind_index: 0,
            hidden_entity_kinds: BTreeSet::new(),
            selected_cell: None,
            mouse_dragging: false,
            last_mouse: None,
            last_fps_mouse: None,
            screenshot_requested: false,
        }
    }

    async fn regenerate(&mut self) {
        let seed = generate_seed();
        self.regenerate_with_seed(seed).await;
    }

    async fn regenerate_with_seed(&mut self, seed: String) {
        self.generator = MegaStructureGenerator::with_config_and_rules(
            seed,
            self.config.clone(),
            self.rule_packs.clone(),
        );
        self.generator.generate();
        self.saved_structure = self.generator.saved_structure();
        self.scenario = generate_scenario(&self.saved_structure);
        self.metadata = self.saved_structure.metadata.clone();
        if let Err(error) = save_current_outputs(
            self.generator.seed(),
            &self.saved_structure,
            &self.export_path,
        ) {
            eprintln!("Failed to save generated structure: {error}");
        }
        self.render_world = build_render_world(&self.generator);
        let center = vec3(
            self.generator.size as f32 / 2.0,
            self.generator.layers as f32 / 2.0,
            self.generator.size as f32 / 2.0,
        );
        let distance = self.generator.size.max(self.generator.layers) as f32 * 1.5;
        self.orbital = OrbitalCamera::new(center, distance);
        self.fps = FpsCamera::new(find_fps_spawn(&self.generator));
        self.selected_cell = None;
        self.entity_animation_time = 0.0;
        self.entity_phase_filter = None;
        self.mouse_dragging = false;
        self.last_mouse = None;
        self.last_fps_mouse = None;
        self.screenshot_requested = false;
    }

    async fn select_rule_delta(&mut self, delta: isize) {
        if self.rule_browser.is_empty() {
            self.rule_status_message = "no rules/*.json files found".to_owned();
            return;
        }
        let len = self.rule_browser.len() as isize;
        self.selected_rule_index =
            (self.selected_rule_index as isize + delta).rem_euclid(len) as usize;
        self.rule_editor = None;
        self.apply_selected_rule_pack().await;
    }

    async fn apply_selected_rule_pack(&mut self) {
        let Some(entry) = self.rule_browser.get(self.selected_rule_index).cloned() else {
            return;
        };
        if !entry.valid {
            self.rule_status_message = format!("cannot apply invalid rule file: {}", entry.status);
            return;
        }
        match CompiledRulePackSet::from_json_file(&entry.path) {
            Ok(compiled) => {
                self.rule_packs = compiled;
                self.rule_path = Some(entry.path.clone());
                self.rule_status_message = format!("applied {}", entry.name);
                self.rule_editor = None;
                let seed = self.current_seed().to_owned();
                self.regenerate_with_seed(seed).await;
            }
            Err(error) => {
                self.rule_status_message = format!("failed to apply {}: {error}", entry.name);
            }
        }
    }

    async fn hot_reload_rules(&mut self) {
        self.rule_browser = rule_browser_entries();
        self.selected_rule_index = selected_rule_index(&self.rule_browser, self.rule_path.as_ref());
        let Some(path) = self.rule_path.clone() else {
            self.rule_status_message = "rescanned rules; no active external rule file".to_owned();
            return;
        };
        match CompiledRulePackSet::from_json_file(&path) {
            Ok(compiled) => {
                self.rule_packs = compiled;
                self.rule_status_message = format!("hot reloaded {}", path.display());
                self.rule_editor = None;
                let seed = self.current_seed().to_owned();
                self.regenerate_with_seed(seed).await;
            }
            Err(error) => {
                self.rule_status_message = format!("hot reload failed: {error}");
            }
        }
    }

    fn toggle_rule_editor(&mut self) {
        if self.rule_editor.is_some() {
            self.rule_editor = None;
            self.rule_editor_message = "editor closed".to_owned();
            return;
        }
        self.load_rule_editor_from_selection();
    }

    fn load_rule_editor_from_selection(&mut self) {
        let Some(entry) = self.rule_browser.get(self.selected_rule_index) else {
            self.rule_editor_message = "no selected rule file".to_owned();
            return;
        };
        match RulePackDocument::from_json_file(&entry.path) {
            Ok(document) => {
                self.rule_editor = Some(document);
                self.selected_editor_pack = 0;
                self.selected_editor_weight = 0;
                self.rule_editor_message = "editing selected rule file".to_owned();
            }
            Err(error) => {
                self.rule_editor_message = format!("cannot edit rule file: {error}");
            }
        }
    }

    fn select_editor_weight(&mut self, weight_index: usize) {
        self.selected_editor_weight = weight_index.min(RULE_EDITOR_WEIGHT_COUNT - 1);
    }

    fn adjust_editor_weight(&mut self, delta: f32) {
        let Some(document) = &mut self.rule_editor else {
            return;
        };
        if document.packs.is_empty() {
            return;
        }
        let pack_index = self.selected_editor_pack.min(document.packs.len() - 1);
        let pack = &mut document.packs[pack_index];
        let label = match self.selected_editor_weight {
            0 => {
                pack.density_weight = (pack.density_weight + delta).clamp(0.05, 4.0);
                "density"
            }
            1 => {
                pack.route_weight = (pack.route_weight + delta).clamp(0.05, 4.0);
                "route"
            }
            2 => {
                pack.decay_weight = (pack.decay_weight + delta).clamp(0.05, 4.0);
                "decay"
            }
            3 => {
                pack.detail_weight = (pack.detail_weight + delta).clamp(0.05, 1.5);
                "detail"
            }
            4 => {
                let weight = pack.entity_density_weight.get_or_insert(1.0);
                *weight = (*weight + delta).clamp(0.0, 4.0);
                "entity density"
            }
            5 => {
                let weight = pack.entity_layout_weight.get_or_insert(1.0);
                *weight = (*weight + delta).clamp(0.0, 4.0);
                "entity layout"
            }
            6 => {
                let weight = pack.patrol_weight.get_or_insert(1.0);
                *weight = (*weight + delta).clamp(0.0, 3.0);
                "patrol"
            }
            7 => {
                let weight = pack.crowd_weight.get_or_insert(1.0);
                *weight = (*weight + delta).clamp(0.0, 3.0);
                "crowd"
            }
            _ => {
                let weight = pack.builder_weight.get_or_insert(1.0);
                *weight = (*weight + delta).clamp(0.0, 3.0);
                "builder"
            }
        };
        self.rule_editor_message = format!("edited {} {}", pack.name, label);
    }

    async fn apply_and_export_rule_editor(&mut self) {
        let Some(document) = self.rule_editor.clone() else {
            self.rule_editor_message = "open editor with E first".to_owned();
            return;
        };
        match document.compile() {
            Ok(compiled) => {
                let path = edited_rule_path(&document.name);
                match fs::write(
                    &path,
                    serde_json::to_string_pretty(&document).unwrap_or_default(),
                ) {
                    Ok(()) => {
                        self.rule_packs = compiled;
                        self.rule_path = Some(path.clone());
                        self.rule_browser = rule_browser_entries();
                        self.selected_rule_index =
                            selected_rule_index(&self.rule_browser, self.rule_path.as_ref());
                        self.rule_status_message = format!("exported + applied {}", path.display());
                        self.rule_editor_message = "editor export applied".to_owned();
                        let seed = self.current_seed().to_owned();
                        self.regenerate_with_seed(seed).await;
                    }
                    Err(error) => {
                        self.rule_editor_message = format!("export failed: {error}");
                    }
                }
            }
            Err(error) => {
                self.rule_editor_message = format!("edited rules invalid: {error}");
            }
        }
    }

    fn current_seed(&self) -> &str {
        self.generator.seed()
    }

    fn profile_name(&self) -> &str {
        self.generator.config().profile.as_str()
    }

    fn typology_name(&self) -> &str {
        self.generator.config().typology.as_str()
    }

    fn current_rule_label(&self) -> String {
        self.rule_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| "built-in".to_owned())
    }

    fn entity_animation_speed(&self) -> f32 {
        [0.5, 1.0, 2.0, 4.0][self.entity_animation_speed_index.min(3)]
    }

    fn cycle_entity_speed(&mut self) {
        self.entity_animation_speed_index = (self.entity_animation_speed_index + 1) % 4;
    }

    fn step_entity_phase(&mut self) {
        let phase_count = self.saved_structure.temporal_state.phases.len();
        if phase_count == 0 {
            self.entity_phase_filter = None;
            return;
        }
        self.entity_phase_filter = Some(match self.entity_phase_filter {
            Some(index) if index + 1 < phase_count => index + 1,
            Some(_) => 0,
            None => 0,
        });
    }

    fn cycle_entity_kind_selection(&mut self) {
        let kinds = entity_kind_order();
        self.selected_entity_kind_index = (self.selected_entity_kind_index + 1) % kinds.len();
    }

    fn toggle_selected_entity_kind(&mut self) {
        let kind = entity_kind_order()[self.selected_entity_kind_index].to_owned();
        if !self.hidden_entity_kinds.remove(&kind) {
            self.hidden_entity_kinds.insert(kind);
        }
    }

    fn entity_kind_visible(&self, kind: &str) -> bool {
        !self.hidden_entity_kinds.contains(kind)
    }
}

fn save_current_outputs(
    seed: &str,
    saved: &SavedStructure,
    export_path: &Path,
) -> structure::StructureResult<()> {
    fs::write(structure::CURRENT_SEED_FILE, seed)?;
    structure::save_structure(export_path, saved)
}

fn rule_browser_entries() -> Vec<RuleBrowserEntry> {
    let mut entries = Vec::new();
    let Ok(read_dir) = fs::read_dir("rules") else {
        return entries;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        entries.push(rule_browser_entry(path));
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

fn rule_browser_entry(path: PathBuf) -> RuleBrowserEntry {
    match CompiledRulePackSet::from_json_file(&path) {
        Ok(compiled) => RuleBrowserEntry {
            name: compiled.source_name.clone(),
            pack_count: compiled.packs().len(),
            grammar_preview: compiled
                .packs()
                .iter()
                .flat_map(|pack| pack.grammar.iter().cloned())
                .take(5)
                .collect(),
            status: "valid".to_owned(),
            valid: true,
            path,
        },
        Err(error) => RuleBrowserEntry {
            name: path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
                .to_owned(),
            pack_count: 0,
            grammar_preview: Vec::new(),
            status: error.to_string(),
            valid: false,
            path,
        },
    }
}

fn selected_rule_index(entries: &[RuleBrowserEntry], rule_path: Option<&PathBuf>) -> usize {
    let Some(rule_path) = rule_path else {
        return 0;
    };
    entries
        .iter()
        .position(|entry| entry.path == *rule_path)
        .unwrap_or(0)
}

fn edited_rule_path(name: &str) -> PathBuf {
    let safe_name: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    PathBuf::from("rules").join(format!("edited-{safe_name}.json"))
}

fn camera_ray(camera: &Camera3D, mouse_x: f32, mouse_y: f32) -> Vec3 {
    let aspect = (screen_width() / screen_height().max(1.0)).max(0.001);
    let tan_half = (CAMERA_FOV_DEGREES.to_radians() * 0.5).tan();
    let x = (2.0 * mouse_x / screen_width().max(1.0) - 1.0) * tan_half * aspect;
    let y = (1.0 - 2.0 * mouse_y / screen_height().max(1.0)) * tan_half;
    let forward = (camera.target - camera.position).normalize();
    let right = forward.cross(camera.up).normalize();
    let up = right.cross(forward).normalize();
    (forward + right * x + up * y).normalize()
}

fn ray_cast(
    generator: &MegaStructureGenerator,
    camera: &Camera3D,
    mouse_x: f32,
    mouse_y: f32,
) -> Option<(usize, usize, usize)> {
    let ray = camera_ray(camera, mouse_x, mouse_y);
    let mut position = camera.position;
    for _ in 0..220 {
        position += ray * 0.5;
        let gx = position.x.round() as isize;
        let gy = position.y.round() as isize;
        let gz = position.z.round() as isize;
        if gx < 0
            || gy < 0
            || gz < 0
            || gx >= generator.size as isize
            || gy >= generator.layers as isize
            || gz >= generator.size as isize
        {
            continue;
        }
        if generator.get(gx as usize, gz as usize, gy as usize) != CellType::Empty {
            return Some((gx as usize, gy as usize, gz as usize));
        }
    }
    None
}

fn draw_world(app: &AppState, camera_position: Vec3) {
    for chunk in &app.render_world.opaque_chunks {
        draw_mesh(&chunk.mesh);
    }
    let mut translucent: Vec<&SpatialChunk> = app.render_world.translucent_chunks.iter().collect();
    translucent.sort_by(|a, b| {
        b.center
            .distance_squared(camera_position)
            .partial_cmp(&a.center.distance_squared(camera_position))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for chunk in translucent {
        draw_mesh(&chunk.mesh);
    }
    if let Some((x, y, z)) = app.selected_cell {
        draw_cube_wires(
            vec3(x as f32, y as f32, z as f32),
            vec3(0.96, 0.96, 0.96),
            YELLOW,
        );
    }
}

fn draw_semantic_overlay(app: &AppState) {
    match app.overlay_mode {
        OverlayMode::None => {}
        OverlayMode::Transit => draw_transit_overlay(app),
        OverlayMode::Districts => draw_district_overlay(app),
        OverlayMode::Strata => draw_strata_overlay(app),
        OverlayMode::Entities => draw_entity_overlay(app),
        OverlayMode::Typology => draw_typology_overlay(app),
        OverlayMode::Construction => draw_construction_overlay(app),
        OverlayMode::Stress => draw_stress_overlay(app),
        OverlayMode::Section => draw_section_overlay(app),
        OverlayMode::Scenario => draw_scenario_overlay(app),
        OverlayMode::Debug => draw_debug_overlay(app),
    }
}

fn draw_typology_overlay(app: &AppState) {
    let frame = &app.saved_structure.typology_frame;
    for pair in frame.primary_spines.windows(2) {
        draw_line_3d(
            point_to_vec3(pair[0]),
            point_to_vec3(pair[1]),
            Color::new(0.05, 0.95, 1.0, 1.0),
        );
    }
    for point in &frame.primary_spines {
        draw_cube_wires(
            point_to_vec3(*point),
            vec3(1.2, 1.2, 1.2),
            Color::new(0.05, 0.95, 1.0, 1.0),
        );
    }
    for point in &frame.service_anchors {
        draw_cube_wires(
            point_to_vec3(*point),
            vec3(1.0, 1.0, 1.0),
            Color::new(1.0, 0.82, 0.24, 1.0),
        );
    }
    for band in &frame.void_bands {
        draw_band_wires(
            band.bounds_min,
            band.bounds_max,
            Color::new(1.0, 0.22, 0.28, 0.95),
        );
    }
    for band in &frame.habitat_bands {
        draw_band_wires(
            band.bounds_min,
            band.bounds_max,
            Color::new(0.24, 1.0, 0.52, 0.95),
        );
    }
}

fn draw_construction_overlay(app: &AppState) {
    for era in &app.saved_structure.construction_history {
        let color = construction_era_color(era.id);
        for route_id in &era.affected_route_ids {
            if let Some(edge) = app.saved_structure.transit_graph.edges.get(*route_id) {
                draw_route_points(edge, color);
            }
        }
        for room_id in era.affected_room_ids.iter().take(28) {
            if let Some(room) = app.saved_structure.rooms.get(*room_id) {
                draw_cube_wires(point_to_vec3(room.position), vec3(0.72, 0.72, 0.72), color);
            }
        }
    }
}

fn draw_stress_overlay(app: &AppState) {
    for field in &app.saved_structure.structural_system.stress_fields {
        let color = stress_color(field.stress);
        draw_band_wires(field.bounds_min, field.bounds_max, color);
        for route_id in &field.route_ids {
            if let Some(edge) = app.saved_structure.transit_graph.edges.get(*route_id) {
                draw_route_points(edge, color);
            }
        }
        for support in field.support_points.iter().take(6) {
            draw_cube_wires(
                point_to_vec3(*support),
                vec3(0.5, 0.5, 0.5),
                Color::new(0.30, 0.95, 0.72, 1.0),
            );
        }
    }
    for path in &app.saved_structure.structural_system.load_paths {
        draw_line_3d(
            point_to_vec3(path.from),
            point_to_vec3(path.to),
            stress_color(path.stress),
        );
    }
}

fn draw_section_overlay(app: &AppState) {
    let quality = &app.saved_structure.section_quality;
    let colors = [
        section_metric_color(quality.vertical_continuity),
        section_metric_color(quality.service_separation),
        section_metric_color(quality.evacuation_shaft_coverage),
        section_metric_color(quality.cross_section_route_density),
    ];
    for y in 0..app.generator.layers {
        let color = colors[y % colors.len()];
        draw_cube_wires(
            vec3(
                app.generator.size as f32 * 0.5,
                y as f32,
                app.generator.size as f32 * 0.5,
            ),
            vec3(app.generator.size as f32, 0.08, app.generator.size as f32),
            color,
        );
    }
    for point in &app.saved_structure.typology_frame.service_anchors {
        draw_line_3d(
            vec3(point[0] as f32, 0.0, point[2] as f32),
            vec3(
                point[0] as f32,
                app.generator.layers as f32,
                point[2] as f32,
            ),
            Color::new(0.95, 0.82, 0.24, 1.0),
        );
    }
}

fn draw_scenario_overlay(app: &AppState) {
    for consequence in &app.scenario.scenario_consequences {
        let color = scenario_consequence_color(&consequence.kind);
        for route_id in &consequence.route_ids {
            if let Some(edge) = app.saved_structure.transit_graph.edges.get(*route_id) {
                draw_route_points(edge, color);
            }
        }
        for room_id in consequence.room_ids.iter().take(24) {
            if let Some(room) = app.saved_structure.rooms.get(*room_id) {
                draw_cube_wires(point_to_vec3(room.position), vec3(0.86, 0.86, 0.86), color);
            }
        }
        for hazard_id in &consequence.hazard_ids {
            if let Some(hazard) = app.saved_structure.hazard_zones.get(*hazard_id) {
                draw_band_wires(hazard.bounds_min, hazard.bounds_max, color);
            }
        }
    }
}

fn draw_route_points(edge: &structure::TransitEdgeRecord, color: Color) {
    for pair in edge.points.windows(2) {
        draw_line_3d(point_to_vec3(pair[0]), point_to_vec3(pair[1]), color);
    }
}

fn draw_band_wires(bounds_min: [usize; 3], bounds_max: [usize; 3], color: Color) {
    let center = vec3(
        (bounds_min[0] + bounds_max[0]) as f32 * 0.5,
        (bounds_min[1] + bounds_max[1]) as f32 * 0.5,
        (bounds_min[2] + bounds_max[2]) as f32 * 0.5,
    );
    let size = vec3(
        (bounds_max[0].saturating_sub(bounds_min[0]) + 1) as f32,
        (bounds_max[1].saturating_sub(bounds_min[1]) + 1) as f32,
        (bounds_max[2].saturating_sub(bounds_min[2]) + 1) as f32,
    );
    draw_cube_wires(center, size, color);
}

fn draw_transit_overlay(app: &AppState) {
    let graph = app.generator.transit_graph();
    for edge in &graph.edges {
        let color = transit_role_color(&edge.role);
        for pair in edge.points.windows(2) {
            draw_line_3d(point_to_vec3(pair[0]), point_to_vec3(pair[1]), color);
        }
    }
    for attachment in &graph.attachments {
        draw_cube_wires(
            point_to_vec3(attachment.position),
            vec3(0.74, 0.74, 0.74),
            Color::new(1.0, 0.86, 0.28, 1.0),
        );
    }
    for node in &graph.nodes {
        draw_cube_wires(
            point_to_vec3(node.position),
            vec3(0.92, 0.92, 0.92),
            Color::new(0.20, 0.95, 1.0, 1.0),
        );
    }
}

fn draw_district_overlay(app: &AppState) {
    for x in 0..app.generator.size {
        for z in 0..app.generator.size {
            let district = app.generator.district_at(x, z);
            let rgb = DISTRICTS[district as usize].color_palette[0];
            draw_cube_wires(
                vec3(x as f32, 0.05, z as f32),
                vec3(0.92, 0.10, 0.92),
                Color::new(rgb.0, rgb.1, rgb.2, 0.88),
            );
        }
    }
    draw_border_overlay(app);
}

fn draw_strata_overlay(app: &AppState) {
    for y in 0..app.generator.layers {
        draw_cube_wires(
            vec3(
                app.generator.size as f32 * 0.5,
                y as f32,
                app.generator.size as f32 * 0.5,
            ),
            vec3(app.generator.size as f32, 0.06, app.generator.size as f32),
            stratum_overlay_color(y, app.generator.layers),
        );
    }
}

fn draw_debug_overlay(app: &AppState) {
    draw_transit_overlay(app);
    draw_border_overlay(app);
    draw_flow_overlay(app);
    draw_hazard_overlay(app);
    draw_entity_overlay(app);
    for cluster in app.generator.computed_room_clusters() {
        draw_cube_wires(
            point_to_vec3(cluster.anchor_position),
            vec3(1.2, 1.2, 1.2),
            cluster_color(&cluster.kind),
        );
    }
    for room in app.generator.rooms() {
        draw_cube_wires(
            point_to_vec3(room.position),
            vec3(0.58, 0.58, 0.58),
            room_color(&room.label),
        );
    }
}

fn draw_flow_overlay(app: &AppState) {
    for flow in app.generator.infrastructure_flows() {
        let color = flow_color(&flow.kind);
        for pair in flow.sample_points.windows(2) {
            draw_line_3d(point_to_vec3(pair[0]), point_to_vec3(pair[1]), color);
        }
    }
}

fn draw_hazard_overlay(app: &AppState) {
    for hazard in app.generator.hazard_zones() {
        let center = vec3(
            (hazard.bounds_min[0] + hazard.bounds_max[0]) as f32 * 0.5,
            (hazard.bounds_min[1] + hazard.bounds_max[1]) as f32 * 0.5,
            (hazard.bounds_min[2] + hazard.bounds_max[2]) as f32 * 0.5,
        );
        let size = vec3(
            (hazard.bounds_max[0].saturating_sub(hazard.bounds_min[0]) + 1) as f32,
            0.5,
            (hazard.bounds_max[2].saturating_sub(hazard.bounds_min[2]) + 1) as f32,
        );
        draw_cube_wires(center, size, hazard_color(&hazard.kind));
    }
}

fn draw_border_overlay(app: &AppState) {
    for border in app.generator.district_borders() {
        draw_cube_wires(
            vec3(
                (border.bounds_min[0] + border.bounds_max[0]) as f32 * 0.5,
                border.y as f32,
                (border.bounds_min[1] + border.bounds_max[1]) as f32 * 0.5,
            ),
            vec3(1.6, 0.18, 1.6),
            border_color(&border.feature),
        );
    }
}

fn draw_entity_overlay(app: &AppState) {
    for field in &app.saved_structure.entity_pressure_fields {
        if !pressure_field_visible_for_phase(app, field) {
            continue;
        }
        let center = vec3(
            (field.bounds_min[0] + field.bounds_max[0]) as f32 * 0.5,
            (field.bounds_min[1] + field.bounds_max[1]) as f32 * 0.5,
            (field.bounds_min[2] + field.bounds_max[2]) as f32 * 0.5,
        );
        let size = vec3(
            (field.bounds_max[0].saturating_sub(field.bounds_min[0]) + 1) as f32,
            (field.bounds_max[1].saturating_sub(field.bounds_min[1]) + 1) as f32 * 0.35,
            (field.bounds_max[2].saturating_sub(field.bounds_min[2]) + 1) as f32,
        );
        draw_cube_wires(center, size, entity_pressure_color(&field.kind));
    }
    for mutation in &app.saved_structure.layout_mutations {
        if let Some(phase_filter) = app.entity_phase_filter {
            if mutation.phase_id != Some(phase_filter) {
                continue;
            }
        }
        let center = vec3(
            (mutation.bounds_min[0] + mutation.bounds_max[0]) as f32 * 0.5,
            (mutation.bounds_min[1] + mutation.bounds_max[1]) as f32 * 0.5,
            (mutation.bounds_min[2] + mutation.bounds_max[2]) as f32 * 0.5,
        );
        draw_cube_wires(center, vec3(1.4, 0.55, 1.4), mutation_color(&mutation.kind));
    }
    for path in app.saved_structure.entity_paths.iter().take(96) {
        let Some(entity) = app.saved_structure.entities.get(path.entity_id) else {
            continue;
        };
        if !app.entity_kind_visible(&entity.kind) {
            continue;
        }
        if let Some(phase_filter) = app.entity_phase_filter {
            if !entity.active_phase_ids.contains(&phase_filter) {
                continue;
            }
        }
        for pair in path.sample_points.windows(2).take(10) {
            draw_line_3d(
                point_to_vec3(pair[0]) + vec3(0.0, 0.18, 0.0),
                point_to_vec3(pair[1]) + vec3(0.0, 0.18, 0.0),
                entity_color(&entity.kind),
            );
        }
        if path.sample_points.is_empty() {
            continue;
        }
        let index = ((app.entity_animation_time * 2.0 + path.id as f32 * 0.37) as usize)
            % path.sample_points.len();
        let position = point_to_vec3(path.sample_points[index]) + vec3(0.0, 0.62, 0.0);
        draw_cube_wires(position, vec3(0.36, 0.36, 0.36), entity_color(&entity.kind));
    }
}

fn pressure_field_visible_for_phase(
    app: &AppState,
    field: &structure::EntityPressureFieldRecord,
) -> bool {
    let Some(phase_filter) = app.entity_phase_filter else {
        return true;
    };
    app.saved_structure.layout_mutations.iter().any(|mutation| {
        mutation.source_pressure_field_id == field.id && mutation.phase_id == Some(phase_filter)
    }) || app.saved_structure.entities.iter().any(|entity| {
        field.source_entity_ids.contains(&entity.id)
            && entity.active_phase_ids.contains(&phase_filter)
    })
}

fn point_to_vec3(point: [usize; 3]) -> Vec3 {
    vec3(point[0] as f32, point[1] as f32, point[2] as f32)
}

fn transit_color(kind: &str) -> Color {
    match kind {
        "vertical_transit_core" => Color::new(0.35, 0.95, 1.0, 1.0),
        "service_tunnel" => Color::new(0.95, 0.58, 0.18, 1.0),
        "skybridge" => Color::new(0.48, 0.76, 1.0, 1.0),
        "express_spine" => Color::new(1.0, 0.25, 0.86, 1.0),
        _ => Color::new(1.0, 0.92, 0.35, 1.0),
    }
}

fn transit_role_color(role: &str) -> Color {
    match role {
        "primary_artery" => Color::new(1.0, 0.92, 0.35, 1.0),
        "service_loop" => Color::new(0.65, 0.78, 0.88, 1.0),
        "restricted_spine" => Color::new(1.0, 0.25, 0.86, 1.0),
        "evacuation_route" => Color::new(0.25, 1.0, 0.74, 1.0),
        "market_run" => Color::new(1.0, 0.62, 0.24, 1.0),
        "maintenance_backbone" => Color::new(0.95, 0.46, 0.16, 1.0),
        _ => transit_color(role),
    }
}

fn construction_era_color(index: usize) -> Color {
    match index % 5 {
        0 => Color::new(0.32, 0.82, 1.0, 1.0),
        1 => Color::new(1.0, 0.70, 0.24, 1.0),
        2 => Color::new(0.55, 1.0, 0.48, 1.0),
        3 => Color::new(1.0, 0.36, 0.62, 1.0),
        _ => Color::new(0.78, 0.62, 1.0, 1.0),
    }
}

fn stress_color(stress: f32) -> Color {
    if stress < 0.35 {
        Color::new(0.24, 0.90, 0.70, 1.0)
    } else if stress < 0.62 {
        Color::new(1.0, 0.76, 0.24, 1.0)
    } else {
        Color::new(1.0, 0.24, 0.24, 1.0)
    }
}

fn section_metric_color(score: f32) -> Color {
    if score >= 0.72 {
        Color::new(0.24, 0.82, 0.92, 0.88)
    } else if score >= 0.45 {
        Color::new(0.94, 0.76, 0.30, 0.88)
    } else {
        Color::new(1.0, 0.34, 0.40, 0.88)
    }
}

fn scenario_consequence_color(kind: &str) -> Color {
    match kind {
        "dynamic_layout_mutation" | "layout_shift" => Color::new(0.95, 0.48, 1.0, 1.0),
        "flood_isolation" | "sealed_breach_zone" | "hazard_escalation" => {
            Color::new(1.0, 0.28, 0.20, 1.0)
        }
        "temporary_bypass" | "evacuation_widening" | "route_pressure" => {
            Color::new(1.0, 0.78, 0.24, 1.0)
        }
        _ => Color::new(0.36, 0.88, 1.0, 1.0),
    }
}

fn border_color(feature: &str) -> Color {
    match feature {
        "BORDER_MARKET" | "SCRAP_MARKET" => Color::new(1.0, 0.72, 0.22, 1.0),
        "SCRAP_ZONE" => Color::new(0.86, 0.34, 0.14, 1.0),
        "SECURITY_THRESHOLD" => Color::new(0.42, 0.82, 1.0, 1.0),
        "SURFACE_COMMONS" => Color::new(0.62, 1.0, 0.52, 1.0),
        _ => Color::new(0.95, 0.95, 0.55, 1.0),
    }
}

fn cluster_color(kind: &str) -> Color {
    match kind {
        "habitation_block" => Color::new(0.82, 0.74, 0.56, 1.0),
        "market_strip" => Color::new(1.0, 0.58, 0.18, 1.0),
        "machine_complex" => Color::new(0.72, 0.72, 0.82, 1.0),
        "shrine_pocket" => Color::new(1.0, 0.76, 0.30, 1.0),
        "data_vault_compound" => Color::new(0.30, 0.82, 1.0, 1.0),
        "transit_cluster" => Color::new(0.20, 1.0, 0.78, 1.0),
        _ => Color::new(0.86, 0.86, 0.60, 1.0),
    }
}

fn flow_color(kind: &str) -> Color {
    match kind {
        "power_bus" => Color::new(1.0, 0.90, 0.22, 1.0),
        "data_spine" => Color::new(0.24, 0.88, 1.0, 1.0),
        "water_reclamation" => Color::new(0.24, 0.56, 1.0, 1.0),
        "waste_chute" => Color::new(0.72, 0.48, 0.24, 1.0),
        "ventilation_loop" => Color::new(0.70, 0.86, 0.88, 1.0),
        _ => Color::new(0.85, 0.85, 0.85, 1.0),
    }
}

fn hazard_color(kind: &str) -> Color {
    match kind {
        "flood_sump" => Color::new(0.20, 0.42, 1.0, 1.0),
        "unstable_span" => Color::new(1.0, 0.38, 0.14, 1.0),
        "security_sweep" => Color::new(1.0, 0.16, 0.62, 1.0),
        "blackout_pocket" => Color::new(0.42, 0.30, 0.58, 1.0),
        "vent_heat_plume" => Color::new(1.0, 0.68, 0.22, 1.0),
        _ => Color::new(1.0, 0.24, 0.24, 1.0),
    }
}

fn entity_color(kind: &str) -> Color {
    match kind {
        "corp_patrol" => Color::new(0.25, 0.55, 1.0, 1.0),
        "evacuee_flow" => Color::new(0.78, 1.0, 0.48, 1.0),
        "maintenance_crawler" => Color::new(0.95, 0.62, 0.24, 1.0),
        "builder_swarm" => Color::new(0.88, 0.86, 0.42, 1.0),
        "scavenger_drift" => Color::new(0.70, 0.55, 0.88, 1.0),
        _ => Color::new(0.10, 0.95, 0.92, 1.0),
    }
}

fn entity_pressure_color(kind: &str) -> Color {
    let base = entity_color(match kind {
        "patrol_lockdown" => "corp_patrol",
        "evacuation_flow" => "evacuee_flow",
        "maintenance_crawler" => "maintenance_crawler",
        "builder_swarm" => "builder_swarm",
        "scavenger_drift" => "scavenger_drift",
        _ => "market_crowd",
    });
    Color::new(base.r, base.g, base.b, 0.88)
}

fn mutation_color(kind: &str) -> Color {
    match kind {
        "entity_security_lockdown" => Color::new(0.38, 0.60, 1.0, 1.0),
        "entity_evacuation_bypass" => Color::new(0.68, 1.0, 0.48, 1.0),
        "entity_service_retrofit" => Color::new(0.95, 0.64, 0.30, 1.0),
        "entity_builder_expansion" => Color::new(0.90, 0.86, 0.35, 1.0),
        _ => Color::new(0.10, 0.95, 0.92, 1.0),
    }
}

fn entity_kind_order() -> [&'static str; 6] {
    [
        "market_crowd",
        "corp_patrol",
        "evacuee_flow",
        "maintenance_crawler",
        "builder_swarm",
        "scavenger_drift",
    ]
}

fn short_entity_kind(kind: &str) -> &'static str {
    match kind {
        "market_crowd" => "crowd",
        "corp_patrol" => "patrol",
        "evacuee_flow" => "evac",
        "maintenance_crawler" => "maint",
        "builder_swarm" => "build",
        "scavenger_drift" => "scav",
        _ => "other",
    }
}

fn room_color(label: &str) -> Color {
    match label {
        "DATA_VAULT" | "SKY_VAULT" => Color::new(0.35, 0.80, 1.0, 1.0),
        "SHRINE" => Color::new(1.0, 0.72, 0.30, 1.0),
        "MAINTENANCE_SHAFT" | "MACHINE_ROOM" => Color::new(0.95, 0.42, 0.18, 1.0),
        label if label.contains("TRANSIT") || label.contains("CHOKEPOINT") => {
            Color::new(0.20, 1.0, 0.78, 1.0)
        }
        _ => Color::new(0.86, 0.86, 0.60, 1.0),
    }
}

fn stratum_overlay_color(y: usize, layers: usize) -> Color {
    let normalized = y as f32 / layers.max(1) as f32;
    if normalized < 0.20 {
        Color::new(0.78, 0.34, 0.18, 0.9)
    } else if normalized < 0.45 {
        Color::new(0.92, 0.74, 0.36, 0.9)
    } else if normalized < 0.75 {
        Color::new(0.34, 0.72, 0.86, 0.9)
    } else {
        Color::new(0.76, 0.50, 1.0, 0.9)
    }
}

fn draw_overlay(app: &AppState) {
    let padding = 12.0;
    let line_height = 22.0;
    draw_rectangle(8.0, 8.0, 430.0, 72.0, Color::new(0.0, 0.0, 0.0, 0.65));
    draw_text(
        &format!("Seed: {}", app.current_seed()),
        padding,
        28.0,
        24.0,
        WHITE,
    );
    draw_text(
        &format!(
            "Profile: {} | Occupied: {:.1}% | Rooms: {} | Links: {}",
            app.profile_name(),
            app.metadata.occupied_cell_ratio * 100.0,
            app.metadata.room_count,
            app.metadata.connection_count
        ),
        padding,
        50.0,
        18.0,
        LIGHTGRAY,
    );
    draw_text(
        &format!("Typology: {}", app.typology_name()),
        padding,
        70.0,
        18.0,
        Color::new(0.78, 0.86, 0.95, 1.0),
    );

    let controls = [
        "Drag: Rotate | Wheel: Zoom | WASD: Pan",
        "1-5: Presets | TAB: FPS | Space: Jump",
        "R: Regenerate | H/Shift+R: Reload Rules",
        "G: Rule Browser | P: PostFX | [ ]: Fog",
        "S: Screenshot | I: Inspect | L/Y: Legend/Labels",
        "T/Z/X/V/B/C: Graph/Zone/Strata/Entities/v22/Debug | Q/Esc: Quit",
        "Entity V: U Pause | J Speed | N Phase | M/K Kind",
    ];
    let mut y = 92.0;
    for line in controls {
        draw_rectangle(8.0, y - 18.0, 420.0, 22.0, Color::new(0.0, 0.0, 0.0, 0.55));
        draw_text(line, padding, y, 20.0, Color::new(0.80, 0.80, 0.80, 1.0));
        y += line_height;
    }

    draw_rectangle(8.0, y - 18.0, 180.0, 22.0, Color::new(0.0, 0.0, 0.0, 0.65));
    draw_text(
        if app.fps_mode {
            "Mode: FPS"
        } else {
            "Mode: Orbital"
        },
        padding,
        y,
        20.0,
        Color::new(1.0, 1.0, 0.55, 1.0),
    );
    y += line_height;

    draw_rectangle(8.0, y - 18.0, 250.0, 22.0, Color::new(0.0, 0.0, 0.0, 0.55));
    draw_text(
        &format!("Overlay: {}", app.overlay_mode.name()),
        padding,
        y,
        20.0,
        Color::new(0.78, 0.78, 0.78, 1.0),
    );
    y += line_height;

    if app.overlay_mode == OverlayMode::Entities {
        draw_entity_legend(app, padding, y);
        y += 112.0;
    }
    if matches!(
        app.overlay_mode,
        OverlayMode::Typology
            | OverlayMode::Construction
            | OverlayMode::Stress
            | OverlayMode::Section
            | OverlayMode::Scenario
    ) {
        draw_v22_overlay_legend(app.overlay_mode, padding, y);
        y += 88.0;
    }

    if app.enable_postfx {
        draw_rectangle(8.0, y - 18.0, 250.0, 22.0, Color::new(0.0, 0.0, 0.0, 0.55));
        draw_text(
            &format!(
                "Fog: {:.2} | Bloom: {:.2}",
                app.fog_density, app.bloom_intensity
            ),
            padding,
            y,
            20.0,
            Color::new(0.78, 0.78, 0.78, 1.0),
        );
        y += line_height;
    }

    if app.inspection_mode {
        draw_rectangle(8.0, y - 18.0, 180.0, 22.0, Color::new(0.0, 0.0, 0.0, 0.65));
        draw_text(
            "Inspection Enabled",
            padding,
            y,
            20.0,
            Color::new(0.85, 0.85, 0.45, 1.0),
        );
        y += line_height;
    }

    if let Some((x, yv, z)) = app.selected_cell {
        let district = app.generator.district_at(x, z);
        let cell = app.generator.get(x, z, yv);
        let room_label = app.generator.nearest_room_label(x, yv, z);
        let route_label = app.generator.nearest_route_label(x, yv, z);
        let decay_label = app.generator.nearest_decay_feature(x, yv, z);
        let rule_influence = selected_rule_influence(app, x, yv, z);
        let material = app.generator.visual_material_at(x, z, yv, cell).name();
        let panel_height = 230.0
            + if room_label.is_some() { 20.0 } else { 0.0 }
            + if route_label.is_some() { 20.0 } else { 0.0 }
            + if decay_label.is_some() { 20.0 } else { 0.0 };
        draw_rectangle(
            8.0,
            y + 2.0,
            430.0,
            panel_height,
            Color::new(0.0, 0.0, 0.0, 0.72),
        );
        draw_text(
            "INSPECT",
            padding,
            y + 24.0,
            24.0,
            Color::new(1.0, 1.0, 0.55, 1.0),
        );
        draw_text(
            &format!("Pos: ({}, {}, {})", x, yv, z),
            padding,
            y + 46.0,
            20.0,
            LIGHTGRAY,
        );
        draw_text(
            &format!("Type: {}", cell.name()),
            padding,
            y + 66.0,
            20.0,
            LIGHTGRAY,
        );
        draw_text(
            &format!("Material: {}", material),
            padding,
            y + 86.0,
            20.0,
            LIGHTGRAY,
        );
        draw_text(
            &format!("Zone: {}", district.name()),
            padding,
            y + 106.0,
            20.0,
            LIGHTGRAY,
        );
        draw_text(
            &format!("Stratum: {}", app.generator.stratum_name_at(yv)),
            padding,
            y + 126.0,
            20.0,
            LIGHTGRAY,
        );
        draw_text(
            &format!(
                "Rule: {}",
                rule_influence
                    .map(|influence| influence.rule_pack_name.as_str())
                    .unwrap_or("built-in fallback")
            ),
            padding,
            y + 146.0,
            18.0,
            LIGHTGRAY,
        );
        draw_text(
            &format!(
                "Influence: {}",
                rule_influence
                    .map(|influence| {
                        format!("{} #{}", influence.target_type, influence.target_id)
                    })
                    .unwrap_or_else(|| "district fallback".to_owned())
            ),
            padding,
            y + 166.0,
            18.0,
            LIGHTGRAY,
        );
        draw_text(
            &format!(
                "Grammar: {}",
                rule_influence
                    .map(rule_grammar_label)
                    .unwrap_or_else(|| district_inspect_grammar(district.name()).to_owned())
            ),
            padding,
            y + 186.0,
            18.0,
            LIGHTGRAY,
        );
        let mut detail_y = y + 208.0;
        if let Some(label) = room_label {
            draw_text(
                &format!("Room: {}", label),
                padding,
                detail_y,
                20.0,
                LIGHTGRAY,
            );
            detail_y += 20.0;
        }
        if let Some(label) = route_label {
            draw_text(
                &format!("Route: {}", label),
                padding,
                detail_y,
                20.0,
                LIGHTGRAY,
            );
            detail_y += 20.0;
        }
        if let Some(label) = decay_label {
            draw_text(
                &format!("Decay: {}", label),
                padding,
                detail_y,
                20.0,
                LIGHTGRAY,
            );
        }
    }

    if app.show_legend {
        let legend_y = screen_height() - 206.0;
        draw_rectangle(
            8.0,
            legend_y - 88.0,
            310.0,
            282.0,
            Color::new(0.0, 0.0, 0.0, 0.65),
        );
        draw_text("Structure", padding, legend_y - 62.0, 24.0, WHITE);
        let mut sy = legend_y - 38.0;
        for label in top_counts(&app.metadata.cell_counts, 3) {
            draw_text(&label, padding, sy, 18.0, LIGHTGRAY);
            sy += 19.0;
        }
        for label in top_counts(&app.metadata.district_counts, 2) {
            draw_text(&label, padding + 140.0, sy - 57.0, 18.0, LIGHTGRAY);
            sy += 19.0;
        }
        draw_text("Materials", padding, legend_y + 24.0, 24.0, WHITE);
        let items = [
            (
                "Concrete",
                MATERIALS[MaterialType::Concrete as usize].base_color,
            ),
            ("Glass", MATERIALS[MaterialType::Glass as usize].base_color),
            ("Metal", MATERIALS[MaterialType::Metal as usize].base_color),
            ("Neon", MATERIALS[MaterialType::Neon as usize].base_color),
            ("Rust", MATERIALS[MaterialType::Rust as usize].base_color),
            ("Steel", MATERIALS[MaterialType::Steel as usize].base_color),
        ];
        let mut ly = legend_y + 38.0;
        for (label, rgb) in items {
            draw_rectangle(
                16.0,
                ly - 12.0,
                16.0,
                16.0,
                Color::new(rgb.0, rgb.1, rgb.2, 1.0),
            );
            draw_text(label, 42.0, ly, 20.0, LIGHTGRAY);
            ly += 22.0;
        }
        draw_text("Transit", padding + 142.0, legend_y + 24.0, 24.0, WHITE);
        for (index, (label, color)) in [
            ("Core", transit_color("vertical_transit_core")),
            ("Tunnel", transit_color("service_tunnel")),
            ("Sky", transit_color("skybridge")),
            ("Express", transit_color("express_spine")),
        ]
        .into_iter()
        .enumerate()
        {
            let y = legend_y + 44.0 + index as f32 * 22.0;
            draw_rectangle(158.0, y - 12.0, 16.0, 16.0, color);
            draw_text(label, 184.0, y, 20.0, LIGHTGRAY);
        }
    }

    if app.show_rule_browser {
        draw_rule_browser_overlay(app);
    }
}

fn draw_entity_legend(app: &AppState, x: f32, y: f32) {
    let width = 430.0;
    let height = 104.0;
    draw_rectangle(
        8.0,
        y - 18.0,
        width,
        height,
        Color::new(0.0, 0.0, 0.0, 0.68),
    );
    let phase = app
        .entity_phase_filter
        .and_then(|id| app.saved_structure.temporal_state.phases.get(id))
        .map(|phase| phase.name.as_str())
        .unwrap_or("all");
    let selected_kind = entity_kind_order()[app.selected_entity_kind_index];
    let selected_state = if app.entity_kind_visible(selected_kind) {
        "visible"
    } else {
        "hidden"
    };
    draw_text(
        &format!(
            "Entities: {} | speed {:.1}x | phase {} | {}",
            if app.entity_animation_paused {
                "paused"
            } else {
                "playing"
            },
            app.entity_animation_speed(),
            phase,
            selected_kind
        ),
        x,
        y,
        18.0,
        Color::new(0.86, 0.92, 1.0, 1.0),
    );
    draw_text(
        "U pause | J speed | N phase | M select kind | K toggle kind",
        x,
        y + 20.0,
        17.0,
        Color::new(0.78, 0.78, 0.78, 1.0),
    );
    draw_text(
        &format!("Selected kind is {selected_state}"),
        x,
        y + 40.0,
        17.0,
        Color::new(0.78, 0.78, 0.78, 1.0),
    );
    for (index, kind) in entity_kind_order().iter().enumerate() {
        let count = app
            .saved_structure
            .entities
            .iter()
            .filter(|entity| entity.kind == *kind && app.entity_kind_visible(&entity.kind))
            .count();
        let color = entity_color(kind);
        let col = index % 3;
        let row = index / 3;
        let item_x = x + col as f32 * 132.0;
        let item_y = y + 56.0 + row as f32 * 18.0;
        draw_rectangle(item_x, item_y, 10.0, 10.0, color);
        draw_text(
            &format!("{} {}", short_entity_kind(kind), count),
            item_x + 14.0,
            item_y + 11.0,
            15.0,
            Color::new(0.84, 0.84, 0.84, 1.0),
        );
    }
}

fn draw_typology_legend(x: f32, y: f32) {
    draw_rectangle(8.0, y - 18.0, 430.0, 80.0, Color::new(0.0, 0.0, 0.0, 0.68));
    let items = [
        ("spines", Color::new(0.05, 0.95, 1.0, 1.0)),
        ("service anchors", Color::new(1.0, 0.82, 0.24, 1.0)),
        ("void bands", Color::new(1.0, 0.22, 0.28, 1.0)),
        ("habitat bands", Color::new(0.24, 1.0, 0.52, 1.0)),
    ];
    draw_text(
        "Typology frame",
        x,
        y,
        18.0,
        Color::new(0.86, 0.92, 1.0, 1.0),
    );
    for (index, (label, color)) in items.iter().enumerate() {
        let item_x = x + (index % 2) as f32 * 170.0;
        let item_y = y + 22.0 + (index / 2) as f32 * 20.0;
        draw_rectangle(item_x, item_y, 10.0, 10.0, *color);
        draw_text(
            label,
            item_x + 14.0,
            item_y + 11.0,
            16.0,
            Color::new(0.84, 0.84, 0.84, 1.0),
        );
    }
}

fn draw_v22_overlay_legend(mode: OverlayMode, x: f32, y: f32) {
    match mode {
        OverlayMode::Typology => draw_typology_legend(x, y),
        OverlayMode::Construction => draw_simple_legend(
            "Construction eras",
            &[
                ("era anchors", construction_era_color(0)),
                ("scar routes", construction_era_color(1)),
                ("rooms", construction_era_color(2)),
            ],
            x,
            y,
        ),
        OverlayMode::Stress => draw_simple_legend(
            "Stress and load paths",
            &[
                ("low", stress_color(0.2)),
                ("medium", stress_color(0.5)),
                ("high", stress_color(0.8)),
                ("supports", Color::new(0.30, 0.95, 0.72, 1.0)),
            ],
            x,
            y,
        ),
        OverlayMode::Section => draw_simple_legend(
            "Section quality",
            &[
                ("good", section_metric_color(0.8)),
                ("thin", section_metric_color(0.55)),
                ("weak", section_metric_color(0.25)),
                ("service shafts", Color::new(0.95, 0.82, 0.24, 1.0)),
            ],
            x,
            y,
        ),
        OverlayMode::Scenario => draw_simple_legend(
            "Scenario consequences",
            &[
                ("layout", scenario_consequence_color("layout_shift")),
                ("hazard", scenario_consequence_color("hazard_escalation")),
                ("route", scenario_consequence_color("route_pressure")),
            ],
            x,
            y,
        ),
        _ => {}
    }
}

fn draw_simple_legend(title: &str, items: &[(&str, Color)], x: f32, y: f32) {
    draw_rectangle(8.0, y - 18.0, 430.0, 80.0, Color::new(0.0, 0.0, 0.0, 0.68));
    draw_text(title, x, y, 18.0, Color::new(0.86, 0.92, 1.0, 1.0));
    for (index, (label, color)) in items.iter().enumerate() {
        let item_x = x + (index % 2) as f32 * 170.0;
        let item_y = y + 22.0 + (index / 2) as f32 * 20.0;
        draw_rectangle(item_x, item_y, 10.0, 10.0, *color);
        draw_text(
            label,
            item_x + 14.0,
            item_y + 11.0,
            16.0,
            Color::new(0.84, 0.84, 0.84, 1.0),
        );
    }
}

fn draw_rule_browser_overlay(app: &AppState) {
    let width = 520.0;
    let height = 360.0;
    let x = screen_width() - width - 16.0;
    let y = 16.0;
    draw_rectangle(x, y, width, height, Color::new(0.0, 0.0, 0.0, 0.78));
    draw_rectangle_lines(x, y, width, height, 1.0, Color::new(0.45, 0.85, 1.0, 0.9));
    draw_text("RULE PACKS", x + 16.0, y + 30.0, 26.0, WHITE);
    draw_text(
        &format!("Active: {}", app.current_rule_label()),
        x + 16.0,
        y + 56.0,
        20.0,
        Color::new(0.86, 0.90, 0.96, 1.0),
    );
    draw_text(
        "G: close | [ ] / arrows: select + regenerate",
        x + 16.0,
        y + 80.0,
        18.0,
        Color::new(0.65, 0.72, 0.78, 1.0),
    );

    if app.rule_browser.is_empty() {
        draw_text(
            "No rules/*.json files found",
            x + 16.0,
            y + 118.0,
            20.0,
            Color::new(1.0, 0.55, 0.45, 1.0),
        );
        return;
    }

    let selected = app
        .rule_browser
        .get(app.selected_rule_index.min(app.rule_browser.len() - 1));
    let mut row_y = y + 112.0;
    for (index, entry) in app.rule_browser.iter().take(8).enumerate() {
        let selected_row = index == app.selected_rule_index;
        let color = if selected_row {
            Color::new(1.0, 0.94, 0.45, 1.0)
        } else if entry.valid {
            LIGHTGRAY
        } else {
            Color::new(1.0, 0.45, 0.38, 1.0)
        };
        if selected_row {
            draw_rectangle(
                x + 12.0,
                row_y - 18.0,
                width - 24.0,
                24.0,
                Color::new(0.18, 0.24, 0.30, 0.82),
            );
        }
        draw_text(
            &format!(
                "{} {} [{} packs]",
                if selected_row { ">" } else { " " },
                entry.name,
                entry.pack_count
            ),
            x + 18.0,
            row_y,
            19.0,
            color,
        );
        row_y += 25.0;
    }

    if let Some(entry) = selected {
        let detail_y = y + 246.0;
        draw_text(
            &format!(
                "Selected: {} | {}",
                entry.path.display(),
                if entry.valid { "valid" } else { "invalid" }
            ),
            x + 16.0,
            detail_y,
            18.0,
            Color::new(0.78, 0.86, 0.95, 1.0),
        );
        draw_text(
            &format!("Status: {}", truncate_text(&entry.status, 58)),
            x + 16.0,
            detail_y + 22.0,
            18.0,
            if entry.valid {
                Color::new(0.58, 1.0, 0.72, 1.0)
            } else {
                Color::new(1.0, 0.55, 0.45, 1.0)
            },
        );
        draw_text(
            &format!("Apply: {}", truncate_text(&app.rule_status_message, 58)),
            x + 16.0,
            detail_y + 66.0,
            18.0,
            Color::new(0.70, 0.82, 1.0, 1.0),
        );
        let grammar = if entry.grammar_preview.is_empty() {
            "no grammar preview".to_owned()
        } else {
            entry.grammar_preview.join(" / ")
        };
        draw_text(
            &format!("Grammar: {}", truncate_text(&grammar, 62)),
            x + 16.0,
            detail_y + 88.0,
            18.0,
            Color::new(0.86, 0.86, 0.78, 1.0),
        );
        draw_rule_editor_overlay(app, x, y + height + 10.0, width);
    }
}

fn draw_rule_editor_overlay(app: &AppState, x: f32, y: f32, width: f32) {
    let Some(document) = &app.rule_editor else {
        draw_text(
            "Editor: E opens selected file | O exports edited copy",
            x + 16.0,
            y + 18.0,
            18.0,
            Color::new(0.70, 0.74, 0.78, 1.0),
        );
        return;
    };
    let height = 234.0;
    draw_rectangle(x, y, width, height, Color::new(0.0, 0.0, 0.0, 0.78));
    draw_rectangle_lines(x, y, width, height, 1.0, Color::new(1.0, 0.76, 0.32, 0.9));
    draw_text("RULE EDITOR", x + 16.0, y + 28.0, 24.0, WHITE);
    draw_text(
        "1-9: weight | -/=: adjust | O: export+apply | E: close",
        x + 16.0,
        y + 52.0,
        17.0,
        Color::new(0.78, 0.82, 0.88, 1.0),
    );
    let Some(pack) = document.packs.get(
        app.selected_editor_pack
            .min(document.packs.len().saturating_sub(1)),
    ) else {
        return;
    };
    draw_text(
        &format!("Pack: {}", truncate_text(&pack.name, 48)),
        x + 16.0,
        y + 78.0,
        18.0,
        LIGHTGRAY,
    );
    let weights = [
        ("1 density", pack.density_weight),
        ("2 route", pack.route_weight),
        ("3 decay", pack.decay_weight),
        ("4 detail", pack.detail_weight),
        ("5 ent density", pack.entity_density_weight.unwrap_or(1.0)),
        ("6 ent layout", pack.entity_layout_weight.unwrap_or(1.0)),
        ("7 patrol", pack.patrol_weight.unwrap_or(1.0)),
        ("8 crowd", pack.crowd_weight.unwrap_or(1.0)),
        ("9 builder", pack.builder_weight.unwrap_or(1.0)),
    ];
    for (index, (label, value)) in weights.into_iter().enumerate() {
        let column = if index < 5 { 0.0 } else { 1.0 };
        let row = if index < 5 { index } else { index - 5 };
        let row_y = y + 104.0 + row as f32 * 20.0;
        let selected = index == app.selected_editor_weight;
        draw_text(
            &format!(
                "{}{}: {:.2}",
                if selected { ">" } else { " " },
                label,
                value
            ),
            x + 18.0 + column * 226.0,
            row_y,
            18.0,
            if selected {
                Color::new(1.0, 0.94, 0.45, 1.0)
            } else {
                LIGHTGRAY
            },
        );
    }
    draw_text(
        &truncate_text(&app.rule_editor_message, 56),
        x + 18.0,
        y + 214.0,
        18.0,
        Color::new(0.70, 0.88, 1.0, 1.0),
    );
}

fn draw_projected_semantic_labels(app: &AppState, camera: &Camera3D) {
    if !app.show_labels {
        return;
    }
    let saved = &app.saved_structure;
    if let Some(phase) = active_phase(saved) {
        draw_rectangle(
            screen_width() * 0.5 - 180.0,
            12.0,
            360.0,
            30.0,
            Color::new(0.0, 0.0, 0.0, 0.62),
        );
        draw_text(
            &format!("Phase: {} @{:02}:00", phase.name, phase.cycle_hour),
            screen_width() * 0.5 - 164.0,
            34.0,
            20.0,
            Color::new(0.55, 0.90, 1.0, 1.0),
        );
    }

    let mut labels = collect_semantic_labels(app);
    labels.sort_by(|a, b| b.priority.cmp(&a.priority));
    let max_distance = app.generator.size.max(app.generator.layers) as f32 * 2.8;
    let mut placed = Vec::new();
    for label in labels {
        let Some(screen) = project_world_label(camera, label.position, max_distance) else {
            continue;
        };
        if placed
            .iter()
            .any(|placed: &Vec2| placed.distance(screen) < 28.0)
        {
            continue;
        }
        draw_label_box(screen, &label.text, label.color);
        placed.push(screen);
        if placed.len() >= 18 {
            break;
        }
    }
}

fn collect_semantic_labels(app: &AppState) -> Vec<SemanticLabel> {
    let saved = &app.saved_structure;
    let mut labels = Vec::new();
    for landmark in saved.narrative_landmarks.iter().take(5) {
        labels.push(SemanticLabel {
            text: format!("{}: {}", landmark.kind, landmark.name),
            position: point_to_vec3(landmark.position),
            color: Color::new(1.0, 0.92, 0.55, 1.0),
            priority: 4,
        });
    }
    for route_id in saved.path_analysis.high_centrality_route_ids.iter().take(5) {
        if let Some(edge) = saved.transit_graph.edges.get(*route_id) {
            let position = edge
                .points
                .get(edge.points.len() / 2)
                .copied()
                .unwrap_or([0, 0, 0]);
            labels.push(SemanticLabel {
                text: format!("#{} {}", edge.id, edge.role),
                position: point_to_vec3(position),
                color: transit_role_color(&edge.role),
                priority: 3,
            });
        }
    }
    for hazard in saved.hazard_zones.iter().take(4) {
        labels.push(SemanticLabel {
            text: format!("hazard: {} {:.2}", hazard.kind, hazard.severity),
            position: vec3(
                (hazard.bounds_min[0] + hazard.bounds_max[0]) as f32 * 0.5,
                (hazard.bounds_min[1] + hazard.bounds_max[1]) as f32 * 0.5,
                (hazard.bounds_min[2] + hazard.bounds_max[2]) as f32 * 0.5,
            ),
            color: hazard_color(&hazard.kind),
            priority: 3,
        });
    }
    for faction in saved.factions.iter().take(4) {
        if let Some(route_id) = faction.controlled_route_ids.first() {
            if let Some(edge) = saved.transit_graph.edges.get(*route_id) {
                let position = edge
                    .points
                    .get(edge.points.len() / 2)
                    .copied()
                    .unwrap_or([0, 0, 0]);
                labels.push(SemanticLabel {
                    text: format!("{} {:.2}", faction.name, faction.influence),
                    position: point_to_vec3(position) + vec3(0.0, 1.4, 0.0),
                    color: Color::new(0.80, 0.82, 1.0, 1.0),
                    priority: 2,
                });
            }
        }
    }
    for field in saved.entity_pressure_fields.iter().take(4) {
        labels.push(SemanticLabel {
            text: format!("dynamic: {} {:.2}", field.kind, field.intensity),
            position: vec3(
                (field.bounds_min[0] + field.bounds_max[0]) as f32 * 0.5,
                (field.bounds_min[1] + field.bounds_max[1]) as f32 * 0.5,
                (field.bounds_min[2] + field.bounds_max[2]) as f32 * 0.5,
            ),
            color: entity_pressure_color(&field.kind),
            priority: 3,
        });
    }
    if let Some((x, y, z)) = app.selected_cell {
        let cell = app.generator.get(x, z, y);
        let room = app
            .generator
            .nearest_room_label(x, y, z)
            .unwrap_or("untyped");
        labels.push(SemanticLabel {
            text: format!("selected {} / {}", cell.name(), room),
            position: vec3(x as f32, y as f32 + 1.2, z as f32),
            color: YELLOW,
            priority: 8,
        });
    }
    labels
}

fn project_world_label(camera: &Camera3D, position: Vec3, max_distance: f32) -> Option<Vec2> {
    if camera.position.distance(position) > max_distance {
        return None;
    }
    let clip = camera.matrix() * vec4(position.x, position.y, position.z, 1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = vec3(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
    if ndc.z < -1.0 || ndc.z > 1.0 || ndc.x.abs() > 1.05 || ndc.y.abs() > 1.05 {
        return None;
    }
    Some(vec2(
        (ndc.x + 1.0) * 0.5 * screen_width(),
        (1.0 - ndc.y) * 0.5 * screen_height(),
    ))
}

fn draw_label_box(position: Vec2, text: &str, color: Color) {
    let metrics = measure_text(text, None, 18, 1.0);
    let x =
        (position.x - metrics.width * 0.5 - 8.0).clamp(8.0, screen_width() - metrics.width - 24.0);
    let y = (position.y - 30.0).clamp(48.0, screen_height() - 28.0);
    draw_line(position.x, position.y, x + 8.0, y + 14.0, 1.0, color);
    draw_rectangle(
        x,
        y,
        metrics.width + 16.0,
        24.0,
        Color::new(0.0, 0.0, 0.0, 0.66),
    );
    draw_rectangle_lines(x, y, metrics.width + 16.0, 24.0, 1.0, color);
    draw_text(text, x + 8.0, y + 17.0, 18.0, color);
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut truncated: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    truncated.push_str("...");
    truncated
}

fn shift_down() -> bool {
    is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)
}

fn active_phase(structure: &SavedStructure) -> Option<&structure::TemporalPhaseRecord> {
    let phases = &structure.temporal_state.phases;
    if phases.is_empty() {
        return None;
    }
    let index = ((get_time() / 8.0) as usize) % phases.len();
    phases.get(index)
}

fn selected_rule_influence(
    app: &AppState,
    x: usize,
    y: usize,
    z: usize,
) -> Option<&structure::RuleInfluenceRecord> {
    let point = [x, y, z];
    let saved = &app.saved_structure;

    if let Some(edge) = saved.transit_graph.edges.iter().min_by_key(|edge| {
        edge.points
            .iter()
            .map(|route_point| point_distance_manhattan(point, *route_point))
            .min()
            .unwrap_or(usize::MAX)
    }) {
        if edge
            .points
            .iter()
            .any(|route_point| point_distance_manhattan(point, *route_point) <= 3)
        {
            if let Some(influence) = find_rule_influence(saved, "route", edge.id.to_string()) {
                return Some(influence);
            }
        }
    }

    if let Some(cluster) = saved
        .room_clusters
        .iter()
        .find(|cluster| point_in_bounds(point, cluster.bounds_min, cluster.bounds_max))
    {
        if let Some(influence) = find_rule_influence(saved, "cluster", cluster.id.to_string()) {
            return Some(influence);
        }
    }

    if let Some(hazard) = saved
        .hazard_zones
        .iter()
        .find(|hazard| point_in_bounds(point, hazard.bounds_min, hazard.bounds_max))
    {
        if let Some(influence) = find_rule_influence(saved, "hazard", hazard.id.to_string()) {
            return Some(influence);
        }
    }

    if let Some(landmark) = saved
        .narrative_landmarks
        .iter()
        .min_by_key(|landmark| point_distance_manhattan(point, landmark.position))
    {
        if point_distance_manhattan(point, landmark.position) <= 4 {
            if let Some(influence) = find_rule_influence(saved, "landmark", landmark.id.to_string())
            {
                return Some(influence);
            }
        }
    }

    let district = app.generator.district_at(x, z).name();
    saved
        .districts
        .iter()
        .find(|record| record.kind == district)
        .and_then(|record| find_rule_influence(saved, "district", record.id.to_string()))
}

fn find_rule_influence<'a>(
    structure: &'a SavedStructure,
    target_type: &str,
    target_id: String,
) -> Option<&'a structure::RuleInfluenceRecord> {
    structure
        .rule_influences
        .iter()
        .find(|influence| influence.target_type == target_type && influence.target_id == target_id)
}

fn point_in_bounds(point: [usize; 3], min: [usize; 3], max: [usize; 3]) -> bool {
    point[0] >= min[0]
        && point[0] <= max[0]
        && point[1] >= min[1]
        && point[1] <= max[1]
        && point[2] >= min[2]
        && point[2] <= max[2]
}

fn point_distance_manhattan(a: [usize; 3], b: [usize; 3]) -> usize {
    a[0].abs_diff(b[0]) + a[1].abs_diff(b[1]) + a[2].abs_diff(b[2])
}

fn rule_grammar_label(influence: &structure::RuleInfluenceRecord) -> String {
    if influence.grammar.is_empty() {
        "built-in weights".to_owned()
    } else {
        truncate_text(&influence.grammar.join(" / "), 58)
    }
}

fn district_inspect_grammar(district: &str) -> &'static str {
    match district {
        "INDUSTRIAL" => "service trunks / machine blocks",
        "RESIDENTIAL" => "habitation / shared corridors",
        "COMMERCIAL" => "market arteries / neon fronts",
        "SLUM" => "patched density / cable walks",
        "ELITE" => "void courts / glass security",
        _ => "mixed megastructure grammar",
    }
}

fn take_screenshot(seed: &str, profile: &str) {
    let _ = fs::create_dir_all("screenshots");
    let image = get_screen_data();
    let profile_suffix = if profile == "balanced" {
        String::new()
    } else {
        format!("_{profile}")
    };
    let path = format!(
        "screenshots/gibson_{}{}_{}.png",
        seed,
        profile_suffix,
        timestamp_token()
    );
    image.export_png(&path);
}

fn top_counts(counts: &std::collections::BTreeMap<String, usize>, limit: usize) -> Vec<String> {
    let mut pairs: Vec<_> = counts.iter().filter(|(_, count)| **count > 0).collect();
    pairs.sort_by(|a, b| b.1.cmp(a.1));
    pairs
        .into_iter()
        .take(limit)
        .map(|(label, count)| format!("{label}: {count}"))
        .collect()
}

fn timestamp_token() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

pub fn window_conf() -> Conf {
    Conf {
        window_title: "Gibson Rust".to_owned(),
        window_width: 1600,
        window_height: 900,
        high_dpi: true,
        window_resizable: true,
        sample_count: 4,
        ..Default::default()
    }
}

const POST_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

varying lowp vec2 uv;
varying lowp vec4 color;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    color = color0 / 255.0;
    uv = texcoord;
}
"#;

const POST_FRAGMENT: &str = r#"#version 100
precision lowp float;

varying vec2 uv;
varying vec4 color;

uniform sampler2D Texture;
uniform float FogDensity;
uniform float BloomIntensity;
uniform float Time;
uniform vec2 ScreenSize;

void main() {
    vec3 scene = texture2D(Texture, uv).rgb * color.rgb;
    vec2 texel = 1.0 / max(ScreenSize, vec2(1.0, 1.0));
    vec3 blur =
        texture2D(Texture, uv + vec2(texel.x, 0.0)).rgb +
        texture2D(Texture, uv - vec2(texel.x, 0.0)).rgb +
        texture2D(Texture, uv + vec2(0.0, texel.y)).rgb +
        texture2D(Texture, uv - vec2(0.0, texel.y)).rgb;
    blur *= 0.25;

    vec3 composed = scene + blur * BloomIntensity;
    float fog = clamp(FogDensity * (1.1 - uv.y), 0.0, 1.0);
    vec3 fog_color = vec3(0.10, 0.10, 0.12);
    composed = mix(composed, fog_color, fog);

    vec2 vignette_uv = uv * (1.0 - uv.yx);
    float vignette = clamp(pow(16.0 * vignette_uv.x * vignette_uv.y, 0.22), 0.0, 1.0);
    composed *= vignette;

    float grain = fract(sin(dot(uv * (Time + 1.0), vec2(12.9898, 78.233))) * 43758.5453);
    composed += vec3((grain - 0.5) * 0.025);
    gl_FragColor = vec4(composed, 1.0);
}
"#;

pub async fn run(options: RuntimeOptions) {
    let mut app = AppState::new(
        options.seed,
        options.config,
        options.rule_packs,
        options.rules_path,
        options.export_path,
    )
    .await;

    loop {
        app.postfx
            .ensure_size(
                screen_width().max(1.0) as u32,
                screen_height().max(1.0) as u32,
            )
            .await;

        let dt = get_frame_time().max(1.0 / 120.0);
        if !app.entity_animation_paused {
            app.entity_animation_time += dt * app.entity_animation_speed();
        }

        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
            break;
        }
        if is_key_pressed(KeyCode::Tab) {
            app.fps_mode = !app.fps_mode;
            set_cursor_grab(app.fps_mode);
            show_mouse(!app.fps_mode);
            app.mouse_dragging = false;
            app.last_mouse = None;
            app.last_fps_mouse = None;
            if app.fps_mode {
                app.fps = FpsCamera::new(find_fps_spawn(&app.generator));
            }
        }
        if is_key_pressed(KeyCode::H) || (is_key_pressed(KeyCode::R) && shift_down()) {
            app.hot_reload_rules().await;
        } else if is_key_pressed(KeyCode::R) {
            app.regenerate().await;
        }
        if is_key_pressed(KeyCode::S) {
            app.screenshot_requested = true;
        }
        if is_key_pressed(KeyCode::P) {
            app.enable_postfx = !app.enable_postfx;
        }
        if is_key_pressed(KeyCode::I) {
            app.inspection_mode = !app.inspection_mode;
            app.mouse_dragging = false;
            app.last_mouse = None;
            if !app.inspection_mode {
                app.selected_cell = None;
            }
        }
        if is_key_pressed(KeyCode::L) {
            app.show_legend = !app.show_legend;
        }
        if is_key_pressed(KeyCode::Y) {
            app.show_labels = !app.show_labels;
        }
        if is_key_pressed(KeyCode::G) {
            app.show_rule_browser = !app.show_rule_browser;
        }
        if app.show_rule_browser {
            if is_key_pressed(KeyCode::E) {
                app.toggle_rule_editor();
            }
            if is_key_pressed(KeyCode::O) {
                app.apply_and_export_rule_editor().await;
            }
            if is_key_pressed(KeyCode::Key1) {
                app.select_editor_weight(0);
            }
            if is_key_pressed(KeyCode::Key2) {
                app.select_editor_weight(1);
            }
            if is_key_pressed(KeyCode::Key3) {
                app.select_editor_weight(2);
            }
            if is_key_pressed(KeyCode::Key4) {
                app.select_editor_weight(3);
            }
            if is_key_pressed(KeyCode::Key5) {
                app.select_editor_weight(4);
            }
            if is_key_pressed(KeyCode::Key6) {
                app.select_editor_weight(5);
            }
            if is_key_pressed(KeyCode::Key7) {
                app.select_editor_weight(6);
            }
            if is_key_pressed(KeyCode::Key8) {
                app.select_editor_weight(7);
            }
            if is_key_pressed(KeyCode::Key9) {
                app.select_editor_weight(8);
            }
            if is_key_pressed(KeyCode::Minus) {
                app.adjust_editor_weight(-0.05);
            }
            if is_key_pressed(KeyCode::Equal) {
                app.adjust_editor_weight(0.05);
            }
            if is_key_pressed(KeyCode::RightBracket) || is_key_pressed(KeyCode::Down) {
                app.select_rule_delta(1).await;
            }
            if is_key_pressed(KeyCode::LeftBracket) || is_key_pressed(KeyCode::Up) {
                app.select_rule_delta(-1).await;
            }
        }
        if is_key_pressed(KeyCode::T) {
            app.overlay_mode = if app.overlay_mode == OverlayMode::Transit {
                OverlayMode::None
            } else {
                OverlayMode::Transit
            };
        }
        if is_key_pressed(KeyCode::Z) {
            app.overlay_mode = if app.overlay_mode == OverlayMode::Districts {
                OverlayMode::None
            } else {
                OverlayMode::Districts
            };
        }
        if is_key_pressed(KeyCode::X) {
            app.overlay_mode = if app.overlay_mode == OverlayMode::Strata {
                OverlayMode::None
            } else {
                OverlayMode::Strata
            };
        }
        if is_key_pressed(KeyCode::V) {
            app.overlay_mode = if app.overlay_mode == OverlayMode::Entities {
                OverlayMode::None
            } else {
                OverlayMode::Entities
            };
        }
        if is_key_pressed(KeyCode::B) {
            app.overlay_mode = app.overlay_mode.next_v22();
        }
        if app.overlay_mode == OverlayMode::Entities && !app.show_rule_browser {
            if is_key_pressed(KeyCode::U) {
                app.entity_animation_paused = !app.entity_animation_paused;
            }
            if is_key_pressed(KeyCode::J) {
                app.cycle_entity_speed();
            }
            if is_key_pressed(KeyCode::N) {
                app.step_entity_phase();
            }
            if is_key_pressed(KeyCode::M) {
                app.cycle_entity_kind_selection();
            }
            if is_key_pressed(KeyCode::K) {
                app.toggle_selected_entity_kind();
            }
        }
        if is_key_pressed(KeyCode::C) {
            app.overlay_mode = if app.overlay_mode == OverlayMode::Debug {
                OverlayMode::None
            } else {
                OverlayMode::Debug
            };
        }
        if !app.show_rule_browser && is_key_down(KeyCode::LeftBracket) {
            app.fog_density = (app.fog_density - 0.01).max(0.0);
        }
        if !app.show_rule_browser && is_key_down(KeyCode::RightBracket) {
            app.fog_density = (app.fog_density + 0.01).min(2.0);
        }
        if !app.show_rule_browser && is_key_down(KeyCode::Minus) {
            app.bloom_intensity = (app.bloom_intensity - 0.01).max(0.0);
        }
        if !app.show_rule_browser && is_key_down(KeyCode::Equal) {
            app.bloom_intensity = (app.bloom_intensity + 0.01).min(2.0);
        }

        if !app.fps_mode {
            if is_key_down(KeyCode::W) {
                app.orbital.pan(0.0, 1.0);
            }
            if is_key_down(KeyCode::S) {
                app.orbital.pan(0.0, -1.0);
            }
            if is_key_down(KeyCode::A) {
                app.orbital.pan(-1.0, 0.0);
            }
            if is_key_down(KeyCode::D) {
                app.orbital.pan(1.0, 0.0);
            }

            if !app.show_rule_browser && is_key_pressed(KeyCode::Key1) {
                app.orbital.set_preset(0);
            }
            if !app.show_rule_browser && is_key_pressed(KeyCode::Key2) {
                app.orbital.set_preset(1);
            }
            if !app.show_rule_browser && is_key_pressed(KeyCode::Key3) {
                app.orbital.set_preset(2);
            }
            if !app.show_rule_browser && is_key_pressed(KeyCode::Key4) {
                app.orbital.set_preset(3);
            }
            if !app.show_rule_browser && is_key_pressed(KeyCode::Key5) {
                app.orbital.set_preset(4);
            }

            let wheel = mouse_wheel().1;
            if wheel.abs() > 0.01 {
                app.orbital.zoom(-wheel * 3.0);
            }

            if app.inspection_mode {
                if is_mouse_button_pressed(MouseButton::Left) {
                    let camera = app.orbital.view_camera(None);
                    let (mx, my) = mouse_position();
                    app.selected_cell = ray_cast(&app.generator, &camera, mx, my);
                }
            } else {
                if is_mouse_button_pressed(MouseButton::Left) {
                    app.mouse_dragging = true;
                    let (mx, my) = mouse_position();
                    app.last_mouse = Some(vec2(mx, my));
                }
                if is_mouse_button_released(MouseButton::Left) {
                    app.mouse_dragging = false;
                    app.last_mouse = None;
                }
                if app.mouse_dragging {
                    let (mx, my) = mouse_position();
                    let current = vec2(mx, my);
                    if let Some(previous) = app.last_mouse {
                        let delta = current - previous;
                        app.orbital.rotate(-delta.x * 0.3, -delta.y * 0.3);
                    }
                    app.last_mouse = Some(current);
                }
            }
            app.orbital.update(dt);
        } else {
            let mut move_forward = 0.0;
            let mut move_right = 0.0;
            if is_key_down(KeyCode::W) {
                move_forward += 1.0;
            }
            if is_key_down(KeyCode::S) {
                move_forward -= 1.0;
            }
            if is_key_down(KeyCode::D) {
                move_right += 1.0;
            }
            if is_key_down(KeyCode::A) {
                move_right -= 1.0;
            }
            if is_key_pressed(KeyCode::Space) {
                app.fps.jump();
            }

            let (mx, my) = mouse_position();
            let mouse = vec2(mx, my);
            if let Some(previous) = app.last_fps_mouse {
                let delta = mouse - previous;
                app.fps.look_delta(delta.x, delta.y);
            }
            app.last_fps_mouse = Some(mouse);
            app.fps.update(dt, move_forward, move_right, &app.generator);
        }

        let render_camera = if app.fps_mode {
            app.fps.view_camera(Some(app.postfx.scene_target.clone()))
        } else {
            app.orbital
                .view_camera(Some(app.postfx.scene_target.clone()))
        };
        set_camera(&render_camera);
        clear_background(Color::new(0.05, 0.05, 0.08, 1.0));
        let camera_position = render_camera.position;
        draw_world(&app, camera_position);
        draw_semantic_overlay(&app);

        set_default_camera();
        clear_background(Color::new(0.05, 0.05, 0.08, 1.0));

        if app.enable_postfx {
            app.postfx
                .material
                .set_uniform("FogDensity", app.fog_density);
            app.postfx
                .material
                .set_uniform("BloomIntensity", app.bloom_intensity);
            app.postfx.material.set_uniform("Time", get_time() as f32);
            app.postfx
                .material
                .set_uniform("ScreenSize", vec2(screen_width(), screen_height()));
            gl_use_material(&app.postfx.material);
            draw_texture_ex(
                &app.postfx.scene_target.texture,
                0.0,
                0.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(screen_width(), screen_height())),
                    flip_y: true,
                    ..Default::default()
                },
            );
            gl_use_default_material();
        } else {
            draw_texture_ex(
                &app.postfx.scene_target.texture,
                0.0,
                0.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(screen_width(), screen_height())),
                    flip_y: true,
                    ..Default::default()
                },
            );
        }

        draw_overlay(&app);
        draw_projected_semantic_labels(&app, &render_camera);
        if app.screenshot_requested {
            take_screenshot(app.current_seed(), app.profile_name());
            app.screenshot_requested = false;
        }
        next_frame().await;
    }
}
