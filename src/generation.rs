use macroquad::prelude::*;
use std::collections::BTreeMap;

use crate::config::GenerationConfig;
use crate::seed::{seed_hash, Rng32};
use crate::structure::{
    self, ConnectionRecord, DistrictRecord, RoomRecord, SavedStructure, StratumRecord,
    StructureMetadata, StructureResult, TransitAttachmentRecord, TransitEdgeRecord,
    TransitGraphRecord, TransitNodeRecord, STRUCTURE_SCHEMA_VERSION,
};

pub(crate) const CHUNK_SIZE_X: usize = 8;
pub(crate) const CHUNK_SIZE_Z: usize = 8;
pub(crate) const CHUNK_SIZE_Y: usize = 4;
pub(crate) const CAMERA_FOV_DEGREES: f32 = 45.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum CellType {
    Empty = 0,
    Vertical = 1,
    Horizontal = 2,
    Bridge = 3,
    Facade = 4,
    Stair = 5,
    Pipe = 6,
    Antenna = 7,
    Cable = 8,
    Vent = 9,
    Elevator = 10,
    Debris = 11,
}

impl CellType {
    const COUNT: usize = 12;

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Empty => "EMPTY",
            Self::Vertical => "VERTICAL",
            Self::Horizontal => "HORIZONTAL",
            Self::Bridge => "BRIDGE",
            Self::Facade => "FACADE",
            Self::Stair => "STAIR",
            Self::Pipe => "PIPE",
            Self::Antenna => "ANTENNA",
            Self::Cable => "CABLE",
            Self::Vent => "VENT",
            Self::Elevator => "ELEVATOR",
            Self::Debris => "DEBRIS",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum MaterialType {
    Concrete = 0,
    Glass = 1,
    Metal = 2,
    Neon = 3,
    Rust = 4,
    Steel = 5,
}

impl MaterialType {
    const COUNT: usize = 6;

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Concrete => "CONCRETE",
            Self::Glass => "GLASS",
            Self::Metal => "METAL",
            Self::Neon => "NEON",
            Self::Rust => "RUST",
            Self::Steel => "STEEL",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum DistrictType {
    Industrial = 0,
    Residential = 1,
    Commercial = 2,
    Slum = 3,
    Elite = 4,
}

impl DistrictType {
    const COUNT: usize = 5;

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Industrial => "INDUSTRIAL",
            Self::Residential => "RESIDENTIAL",
            Self::Commercial => "COMMERCIAL",
            Self::Slum => "SLUM",
            Self::Elite => "ELITE",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum BiomeStratum {
    Underground = 0,
    Surface = 1,
    Midrise = 2,
    Skyline = 3,
}

impl BiomeStratum {
    const COUNT: usize = 4;

    fn name(self) -> &'static str {
        match self {
            Self::Underground => "UNDERGROUND",
            Self::Surface => "SURFACE",
            Self::Midrise => "MIDRISE",
            Self::Skyline => "SKYLINE",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum WFCTile {
    Empty = 0,
    FloorSolid = 1,
    FloorHalfN = 2,
    FloorHalfE = 3,
    WallN = 4,
    WallE = 5,
    WallCornerNE = 6,
    WallCornerNW = 7,
    CorridorNS = 8,
    CorridorEW = 9,
    RoomCenter = 10,
    DoorN = 11,
    DoorE = 12,
    Stairwell = 13,
    ElevatorShaft = 14,
}

const WFC_TILE_COUNT: usize = 15;

#[derive(Clone, Copy)]
pub(crate) struct MaterialStyle {
    pub(crate) base_color: (f32, f32, f32),
    pub(crate) alpha: f32,
}

#[derive(Clone, Copy)]
pub(crate) struct DistrictProps {
    pub(crate) color_palette: [(f32, f32, f32); 3],
    pub(crate) core_density: f32,
    pub(crate) floor_thickness: i32,
    pub(crate) vertical_variation: f32,
    pub(crate) neon_probability: f32,
    pub(crate) pipe_probability: f32,
    pub(crate) elevator_probability: f32,
}

#[derive(Clone, Copy)]
struct BiomeParams {
    y_min: usize,
    y_max: usize,
    rust_mult: f32,
}

pub(crate) const MATERIALS: [MaterialStyle; 6] = [
    MaterialStyle {
        base_color: (0.50, 0.50, 0.60),
        alpha: 1.0,
    },
    MaterialStyle {
        base_color: (0.40, 0.70, 0.90),
        alpha: 0.42,
    },
    MaterialStyle {
        base_color: (0.60, 0.60, 0.70),
        alpha: 1.0,
    },
    MaterialStyle {
        base_color: (0.08, 0.92, 0.96),
        alpha: 1.0,
    },
    MaterialStyle {
        base_color: (0.80, 0.40, 0.20),
        alpha: 1.0,
    },
    MaterialStyle {
        base_color: (0.40, 0.50, 0.60),
        alpha: 1.0,
    },
];

pub(crate) const DISTRICTS: [DistrictProps; 5] = [
    DistrictProps {
        color_palette: [(0.30, 0.30, 0.40), (0.40, 0.50, 0.50), (0.20, 0.30, 0.35)],
        core_density: 1.2,
        floor_thickness: 2,
        vertical_variation: 0.30,
        neon_probability: 0.10,
        pipe_probability: 0.40,
        elevator_probability: 0.10,
    },
    DistrictProps {
        color_palette: [(0.60, 0.50, 0.40), (0.70, 0.60, 0.50), (0.50, 0.40, 0.30)],
        core_density: 0.8,
        floor_thickness: 1,
        vertical_variation: 0.50,
        neon_probability: 0.20,
        pipe_probability: 0.20,
        elevator_probability: 0.12,
    },
    DistrictProps {
        color_palette: [(0.20, 0.30, 0.40), (0.30, 0.40, 0.50), (0.10, 0.20, 0.30)],
        core_density: 0.6,
        floor_thickness: 3,
        vertical_variation: 0.80,
        neon_probability: 0.40,
        pipe_probability: 0.18,
        elevator_probability: 0.20,
    },
    DistrictProps {
        color_palette: [(0.40, 0.35, 0.30), (0.50, 0.40, 0.35), (0.45, 0.40, 0.35)],
        core_density: 1.5,
        floor_thickness: 1,
        vertical_variation: 0.20,
        neon_probability: 0.05,
        pipe_probability: 0.50,
        elevator_probability: 0.08,
    },
    DistrictProps {
        color_palette: [(0.80, 0.80, 0.85), (0.75, 0.75, 0.80), (0.70, 0.75, 0.80)],
        core_density: 0.4,
        floor_thickness: 3,
        vertical_variation: 0.90,
        neon_probability: 0.30,
        pipe_probability: 0.12,
        elevator_probability: 0.26,
    },
];

const BIOME_TABLE: [BiomeParams; 4] = [
    BiomeParams {
        y_min: 0,
        y_max: 2,
        rust_mult: 1.5,
    },
    BiomeParams {
        y_min: 3,
        y_max: 6,
        rust_mult: 1.0,
    },
    BiomeParams {
        y_min: 7,
        y_max: 11,
        rust_mult: 0.8,
    },
    BiomeParams {
        y_min: 12,
        y_max: 14,
        rust_mult: 0.5,
    },
];

#[derive(Clone, Copy)]
struct WfcCell {
    possible: u16,
    collapsed_tile: Option<usize>,
    entropy: f32,
}

pub(crate) fn clampf(v: f32, lo: f32, hi: f32) -> f32 {
    v.max(lo).min(hi)
}

fn lerpf(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub(crate) fn mix_color(a: (f32, f32, f32), b: (f32, f32, f32), t: f32) -> (f32, f32, f32) {
    (lerpf(a.0, b.0, t), lerpf(a.1, b.1, t), lerpf(a.2, b.2, t))
}

fn biome_for_y(y: usize) -> BiomeStratum {
    for (index, entry) in BIOME_TABLE.iter().enumerate() {
        if y >= entry.y_min && y <= entry.y_max {
            return match index {
                0 => BiomeStratum::Underground,
                1 => BiomeStratum::Surface,
                2 => BiomeStratum::Midrise,
                _ => BiomeStratum::Skyline,
            };
        }
    }
    BiomeStratum::Surface
}

fn configured_stratum_for_y(y: usize, layers: usize, separation: f32) -> BiomeStratum {
    let layer_count = layers.max(1) as f32;
    let normalized = y as f32 / layer_count;
    let underground_cut = (0.20 * separation).clamp(0.12, 0.35);
    let surface_cut = (0.45 * separation).clamp(underground_cut + 0.10, 0.68);
    let midrise_cut = (0.78 + (separation - 1.0) * 0.08).clamp(surface_cut + 0.10, 0.90);
    if normalized < underground_cut {
        BiomeStratum::Underground
    } else if normalized < surface_cut {
        BiomeStratum::Surface
    } else if normalized < midrise_cut {
        BiomeStratum::Midrise
    } else {
        BiomeStratum::Skyline
    }
}

pub(crate) fn biome_rust_at(y: usize) -> f32 {
    match biome_for_y(y) {
        BiomeStratum::Underground => BIOME_TABLE[0].rust_mult,
        BiomeStratum::Surface => BIOME_TABLE[1].rust_mult,
        BiomeStratum::Midrise => BIOME_TABLE[2].rust_mult,
        BiomeStratum::Skyline => BIOME_TABLE[3].rust_mult,
    }
}

pub(crate) fn cell_to_material(cell: CellType) -> MaterialType {
    match cell {
        CellType::Empty | CellType::Vertical | CellType::Horizontal => MaterialType::Concrete,
        CellType::Bridge | CellType::Cable => MaterialType::Steel,
        CellType::Facade | CellType::Elevator => MaterialType::Glass,
        CellType::Stair | CellType::Antenna | CellType::Vent => MaterialType::Metal,
        CellType::Pipe | CellType::Debris => MaterialType::Rust,
    }
}

pub(crate) fn hash_noise(seed_hash: u64, x: usize, z: usize, y: usize) -> f32 {
    let mut value = seed_hash
        ^ ((x as u64).wrapping_mul(0x9E37_79B1))
        ^ ((z as u64).wrapping_mul(0x85EB_CA77))
        ^ ((y as u64).wrapping_mul(0xC2B2_AE3D));
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51afd7ed558ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ceb9fe1a85ec53);
    value ^= value >> 33;
    (value as u32) as f32 / u32::MAX as f32
}

pub(crate) fn is_walkable_floor_cell(cell: CellType) -> bool {
    matches!(cell, CellType::Horizontal | CellType::Bridge)
}

fn is_traversal_carveable_cell(cell: CellType) -> bool {
    matches!(
        cell,
        CellType::Horizontal
            | CellType::Bridge
            | CellType::Facade
            | CellType::Stair
            | CellType::Pipe
            | CellType::Antenna
            | CellType::Cable
            | CellType::Vent
            | CellType::Elevator
            | CellType::Debris
    )
}

pub(crate) mod simplex {
    const PERM: [u8; 256] = [
        151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225, 140, 36, 103, 30,
        69, 142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148, 247, 120, 234, 75, 0, 26, 197, 62, 94,
        252, 219, 203, 117, 35, 11, 32, 57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171,
        168, 68, 175, 74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122, 60,
        211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54, 65, 25, 63, 161, 1,
        216, 80, 73, 209, 76, 132, 187, 208, 89, 18, 169, 200, 196, 135, 130, 116, 188, 159, 86,
        164, 100, 109, 198, 173, 186, 3, 64, 52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118,
        126, 255, 82, 85, 212, 207, 206, 59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170,
        213, 119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172, 9, 129, 22, 39,
        253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104, 218, 246, 97, 228, 251, 34,
        242, 193, 238, 210, 144, 12, 191, 179, 162, 241, 81, 51, 145, 235, 249, 14, 239, 107, 49,
        192, 214, 31, 181, 199, 106, 157, 184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254,
        138, 236, 205, 93, 222, 114, 67, 29, 24, 72, 243, 141, 128, 195, 78, 66, 215, 61, 156, 180,
    ];

    const GRAD3: [[f32; 3]; 12] = [
        [1.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
        [1.0, -1.0, 0.0],
        [-1.0, -1.0, 0.0],
        [1.0, 0.0, 1.0],
        [-1.0, 0.0, 1.0],
        [1.0, 0.0, -1.0],
        [-1.0, 0.0, -1.0],
        [0.0, 1.0, 1.0],
        [0.0, -1.0, 1.0],
        [0.0, 1.0, -1.0],
        [0.0, -1.0, -1.0],
    ];

    fn fastfloor(x: f32) -> i32 {
        let xi = x as i32;
        if x < xi as f32 {
            xi - 1
        } else {
            xi
        }
    }

    fn gdot3(g: [f32; 3], x: f32, y: f32, z: f32) -> f32 {
        g[0] * x + g[1] * y + g[2] * z
    }

    pub fn noise3(xin: f32, yin: f32, zin: f32) -> f32 {
        const F3: f32 = 1.0 / 3.0;
        const G3: f32 = 1.0 / 6.0;
        let s = (xin + yin + zin) * F3;
        let i = fastfloor(xin + s);
        let j = fastfloor(yin + s);
        let k = fastfloor(zin + s);
        let t = (i + j + k) as f32 * G3;
        let x0 = xin - (i as f32 - t);
        let y0 = yin - (j as f32 - t);
        let z0 = zin - (k as f32 - t);

        let (i1, j1, k1, i2, j2, k2) = if x0 >= y0 {
            if y0 >= z0 {
                (1, 0, 0, 1, 1, 0)
            } else if x0 >= z0 {
                (1, 0, 0, 1, 0, 1)
            } else {
                (0, 0, 1, 1, 0, 1)
            }
        } else if y0 < z0 {
            (0, 0, 1, 0, 1, 1)
        } else if x0 < z0 {
            (0, 1, 0, 0, 1, 1)
        } else {
            (0, 1, 0, 1, 1, 0)
        };

        let x1 = x0 - i1 as f32 + G3;
        let y1 = y0 - j1 as f32 + G3;
        let z1 = z0 - k1 as f32 + G3;
        let x2 = x0 - i2 as f32 + 2.0 * G3;
        let y2 = y0 - j2 as f32 + 2.0 * G3;
        let z2 = z0 - k2 as f32 + 2.0 * G3;
        let x3 = x0 - 1.0 + 3.0 * G3;
        let y3 = y0 - 1.0 + 3.0 * G3;
        let z3 = z0 - 1.0 + 3.0 * G3;

        let ii = (i & 255) as usize;
        let jj = (j & 255) as usize;
        let kk = (k & 255) as usize;

        let gi0 = PERM[(ii + PERM[(jj + PERM[kk] as usize) & 255] as usize) & 255] as usize % 12;
        let gi1 = PERM[(ii
            + i1 as usize
            + PERM[(jj + j1 as usize + PERM[(kk + k1 as usize) & 255] as usize) & 255] as usize)
            & 255] as usize
            % 12;
        let gi2 = PERM[(ii
            + i2 as usize
            + PERM[(jj + j2 as usize + PERM[(kk + k2 as usize) & 255] as usize) & 255] as usize)
            & 255] as usize
            % 12;
        let gi3 = PERM
            [(ii + 1 + PERM[(jj + 1 + PERM[(kk + 1) & 255] as usize) & 255] as usize) & 255]
            as usize
            % 12;

        let mut n0 = 0.0;
        let mut n1 = 0.0;
        let mut n2 = 0.0;
        let mut n3 = 0.0;

        let t0 = 0.6 - x0 * x0 - y0 * y0 - z0 * z0;
        if t0 >= 0.0 {
            let t0sq = t0 * t0;
            n0 = t0sq * t0sq * gdot3(GRAD3[gi0], x0, y0, z0);
        }
        let t1 = 0.6 - x1 * x1 - y1 * y1 - z1 * z1;
        if t1 >= 0.0 {
            let t1sq = t1 * t1;
            n1 = t1sq * t1sq * gdot3(GRAD3[gi1], x1, y1, z1);
        }
        let t2 = 0.6 - x2 * x2 - y2 * y2 - z2 * z2;
        if t2 >= 0.0 {
            let t2sq = t2 * t2;
            n2 = t2sq * t2sq * gdot3(GRAD3[gi2], x2, y2, z2);
        }
        let t3 = 0.6 - x3 * x3 - y3 * y3 - z3 * z3;
        if t3 >= 0.0 {
            let t3sq = t3 * t3;
            n3 = t3sq * t3sq * gdot3(GRAD3[gi3], x3, y3, z3);
        }

        32.0 * (n0 + n1 + n2 + n3)
    }
}

fn catmull_rom_point(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let t2 = t * t;
    let t3 = t2 * t;
    p0 * (-0.5 * t3 + t2 - 0.5 * t)
        + p1 * (1.5 * t3 - 2.5 * t2 + 1.0)
        + p2 * (-1.5 * t3 + 2.0 * t2 + 0.5 * t)
        + p3 * (0.5 * t3 - 0.5 * t2)
}

fn rasterize_spline(
    p0: Vec3,
    p1: Vec3,
    p2: Vec3,
    p3: Vec3,
    steps: usize,
) -> Vec<(usize, usize, usize)> {
    let mut points = Vec::new();
    let mut last = None;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let p = catmull_rom_point(p0, p1, p2, p3, t);
        let point = (
            p.x.round().max(0.0) as usize,
            p.z.round().max(0.0) as usize,
            p.y.round().max(0.0) as usize,
        );
        if Some(point) != last {
            points.push(point);
            last = Some(point);
        }
    }
    points
}

fn wfc_init_tables() -> ([[u16; 4]; WFC_TILE_COUNT], [f32; WFC_TILE_COUNT]) {
    let mut adjacency = [[0xFFFFu16; 4]; WFC_TILE_COUNT];
    let mut weights = [1.0f32; WFC_TILE_COUNT];
    weights[WFCTile::Empty as usize] = 3.0;
    weights[WFCTile::FloorSolid as usize] = 5.0;
    weights[WFCTile::CorridorNS as usize] = 2.0;
    weights[WFCTile::CorridorEW as usize] = 2.0;
    weights[WFCTile::RoomCenter as usize] = 3.0;
    weights[WFCTile::WallN as usize] = 1.5;
    weights[WFCTile::WallE as usize] = 1.5;
    weights[WFCTile::WallCornerNE as usize] = 0.8;
    weights[WFCTile::WallCornerNW as usize] = 0.8;
    weights[WFCTile::DoorN as usize] = 0.5;
    weights[WFCTile::DoorE as usize] = 0.5;
    weights[WFCTile::Stairwell as usize] = 0.3;
    weights[WFCTile::ElevatorShaft as usize] = 0.2;
    weights[WFCTile::FloorHalfN as usize] = 1.0;
    weights[WFCTile::FloorHalfE as usize] = 1.0;

    let no_wall_n = !(1u16 << WFCTile::WallN as usize);
    let no_wall_e = !(1u16 << WFCTile::WallE as usize);
    adjacency[WFCTile::WallN as usize][0] = no_wall_n;
    adjacency[WFCTile::WallN as usize][2] = no_wall_n;
    adjacency[WFCTile::WallE as usize][1] = no_wall_e;
    adjacency[WFCTile::WallE as usize][3] = no_wall_e;

    let corridor_ns_ew = (1u16 << WFCTile::WallN as usize)
        | (1u16 << WFCTile::WallE as usize)
        | (1u16 << WFCTile::WallCornerNE as usize)
        | (1u16 << WFCTile::WallCornerNW as usize)
        | (1u16 << WFCTile::Empty as usize);
    adjacency[WFCTile::CorridorNS as usize][1] = corridor_ns_ew;
    adjacency[WFCTile::CorridorNS as usize][3] = corridor_ns_ew;
    adjacency[WFCTile::CorridorEW as usize][0] = corridor_ns_ew;
    adjacency[WFCTile::CorridorEW as usize][2] = corridor_ns_ew;

    let struct_mask = (1u16 << WFCTile::FloorSolid as usize)
        | (1u16 << WFCTile::CorridorNS as usize)
        | (1u16 << WFCTile::CorridorEW as usize)
        | (1u16 << WFCTile::RoomCenter as usize)
        | (1u16 << WFCTile::DoorN as usize)
        | (1u16 << WFCTile::DoorE as usize)
        | (1u16 << WFCTile::Stairwell as usize)
        | (1u16 << WFCTile::ElevatorShaft as usize);
    for tile in [WFCTile::Stairwell as usize, WFCTile::ElevatorShaft as usize] {
        for direction_mask in adjacency[tile].iter_mut().take(4) {
            *direction_mask = struct_mask | (1u16 << WFCTile::Empty as usize);
        }
    }

    (adjacency, weights)
}

fn wfc_calc_entropy(possible: u16, weights: &[f32; WFC_TILE_COUNT]) -> f32 {
    let mut sum = 0.0;
    let mut sum_log = 0.0;
    for (index, weight) in weights.iter().enumerate() {
        if possible & (1u16 << index) != 0 {
            sum += *weight;
            sum_log += *weight * (*weight + 1e-6).ln();
        }
    }
    if sum < 1e-6 {
        0.0
    } else {
        sum.ln() - sum_log / sum
    }
}

fn wfc_count_options(possible: u16) -> usize {
    (0..WFC_TILE_COUNT)
        .filter(|index| possible & (1u16 << index) != 0)
        .count()
}

struct WfcSolver {
    size: usize,
    cells: Vec<WfcCell>,
    weights: [f32; WFC_TILE_COUNT],
    adjacency: [[u16; 4]; WFC_TILE_COUNT],
    rng: Rng32,
    backtrack_depth: usize,
}

impl WfcSolver {
    fn new(
        seed: u64,
        size: usize,
        district: DistrictType,
        stratum: BiomeStratum,
        room_density: f32,
    ) -> Self {
        let (adjacency, mut weights) = wfc_init_tables();
        weights[WFCTile::RoomCenter as usize] *= room_density;
        weights[WFCTile::FloorSolid as usize] *= room_density.clamp(0.5, 2.0);
        match district {
            DistrictType::Industrial => {
                weights[WFCTile::CorridorNS as usize] *= 1.5;
                weights[WFCTile::CorridorEW as usize] *= 1.5;
            }
            DistrictType::Elite => {
                weights[WFCTile::RoomCenter as usize] *= 2.0;
                weights[WFCTile::FloorSolid as usize] *= 1.5;
            }
            DistrictType::Slum => {
                weights[WFCTile::Empty as usize] *= 2.0;
                weights[WFCTile::FloorHalfN as usize] *= 2.0;
            }
            _ => {}
        }
        match stratum {
            BiomeStratum::Underground => {
                weights[WFCTile::CorridorNS as usize] *= 1.5;
                weights[WFCTile::CorridorEW as usize] *= 1.5;
            }
            BiomeStratum::Skyline => {
                weights[WFCTile::Empty as usize] *= 3.0;
            }
            _ => {}
        }
        let mut cells = vec![
            WfcCell {
                possible: (1u16 << WFC_TILE_COUNT) - 1,
                collapsed_tile: None,
                entropy: 0.0,
            };
            size * size
        ];
        for cell in &mut cells {
            cell.entropy = wfc_calc_entropy(cell.possible, &weights);
        }
        Self {
            size,
            cells,
            weights,
            adjacency,
            rng: Rng32::new(seed),
            backtrack_depth: 0,
        }
    }

    fn idx(&self, x: usize, z: usize) -> usize {
        x * self.size + z
    }

    fn constrain(&mut self, x: usize, z: usize, tile: WFCTile) {
        let index = self.idx(x, z);
        let cell = &mut self.cells[index];
        cell.possible = 1u16 << tile as usize;
        cell.collapsed_tile = Some(tile as usize);
        cell.entropy = 0.0;
    }

    fn propagate(&mut self, start_x: usize, start_z: usize) -> bool {
        let mut queue = vec![(start_x, start_z)];
        let directions = [(0isize, 1isize), (1, 0), (0, -1), (-1, 0)];
        let mut iterations = 0usize;
        while let Some((x, z)) = queue.pop() {
            if iterations > 1000 {
                break;
            }
            let current_tile = match self.cells[self.idx(x, z)].collapsed_tile {
                Some(tile) => tile,
                None => {
                    iterations += 1;
                    continue;
                }
            };
            for (direction, (dx, dz)) in directions.iter().enumerate() {
                let nx = x as isize + dx;
                let nz = z as isize + dz;
                if nx < 0 || nz < 0 || nx >= self.size as isize || nz >= self.size as isize {
                    continue;
                }
                let nx = nx as usize;
                let nz = nz as usize;
                let index = self.idx(nx, nz);
                if self.cells[index].collapsed_tile.is_some() {
                    continue;
                }
                let allowed = self.adjacency[current_tile][direction];
                let previous = self.cells[index].possible;
                self.cells[index].possible &= allowed;
                if self.cells[index].possible == 0 {
                    return false;
                }
                if self.cells[index].possible != previous {
                    self.cells[index].entropy =
                        wfc_calc_entropy(self.cells[index].possible, &self.weights);
                    if wfc_count_options(self.cells[index].possible) == 1 {
                        let tile = (0..WFC_TILE_COUNT)
                            .find(|candidate| self.cells[index].possible & (1u16 << candidate) != 0)
                            .unwrap_or(WFCTile::Empty as usize);
                        self.cells[index].collapsed_tile = Some(tile);
                        queue.push((nx, nz));
                    }
                }
            }
            iterations += 1;
        }
        true
    }

    fn collapse_one(&mut self) -> bool {
        let mut best = None;
        let mut best_entropy = f32::MAX;
        for x in 0..self.size {
            for z in 0..self.size {
                let cell = self.cells[self.idx(x, z)];
                if cell.collapsed_tile.is_some() {
                    continue;
                }
                if cell.entropy < best_entropy {
                    best_entropy = cell.entropy;
                    best = Some((x, z));
                }
            }
        }
        let (bx, bz) = match best {
            Some(value) => value,
            None => return false,
        };
        let possible = self.cells[self.idx(bx, bz)].possible;
        let mut total = 0.0;
        for index in 0..WFC_TILE_COUNT {
            if possible & (1u16 << index) != 0 {
                total += self.weights[index];
            }
        }
        let mut pick = self.rng.next_f32() * total.max(0.0001);
        let mut chosen = WFCTile::Empty as usize;
        for index in 0..WFC_TILE_COUNT {
            if possible & (1u16 << index) == 0 {
                continue;
            }
            pick -= self.weights[index];
            if pick <= 0.0 {
                chosen = index;
                break;
            }
        }
        let index = self.idx(bx, bz);
        self.cells[index].collapsed_tile = Some(chosen);
        self.cells[index].possible = 1u16 << chosen;
        self.cells[index].entropy = 0.0;
        if !self.propagate(bx, bz) {
            self.backtrack_depth += 1;
            if self.backtrack_depth > 50 {
                return false;
            }
            self.cells[index].possible = ((1u16 << WFC_TILE_COUNT) - 1) & !(1u16 << chosen);
            self.cells[index].collapsed_tile = None;
            self.cells[index].entropy = wfc_calc_entropy(self.cells[index].possible, &self.weights);
            if self.cells[index].possible == 0 {
                self.cells[index].possible = 1u16;
                self.cells[index].collapsed_tile = Some(WFCTile::Empty as usize);
                self.cells[index].entropy = 0.0;
            }
        }
        true
    }

    fn solve(&mut self) {
        for _ in 0..1000 {
            if !self.collapse_one() {
                break;
            }
        }
        for cell in &mut self.cells {
            if cell.collapsed_tile.is_none() {
                let mut best_weight = -1.0f32;
                let mut best_tile = WFCTile::Empty as usize;
                for tile in 0..WFC_TILE_COUNT {
                    if cell.possible & (1u16 << tile) != 0 && self.weights[tile] > best_weight {
                        best_weight = self.weights[tile];
                        best_tile = tile;
                    }
                }
                cell.collapsed_tile = Some(best_tile);
            }
        }
    }

    fn tile_to_cell(tile: usize) -> CellType {
        match tile {
            x if x == WFCTile::FloorSolid as usize
                || x == WFCTile::FloorHalfN as usize
                || x == WFCTile::FloorHalfE as usize
                || x == WFCTile::RoomCenter as usize
                || x == WFCTile::CorridorNS as usize
                || x == WFCTile::CorridorEW as usize =>
            {
                CellType::Horizontal
            }
            x if x == WFCTile::WallN as usize
                || x == WFCTile::WallE as usize
                || x == WFCTile::WallCornerNE as usize
                || x == WFCTile::WallCornerNW as usize
                || x == WFCTile::DoorN as usize
                || x == WFCTile::DoorE as usize =>
            {
                CellType::Facade
            }
            x if x == WFCTile::Stairwell as usize => CellType::Stair,
            x if x == WFCTile::ElevatorShaft as usize => CellType::Elevator,
            _ => CellType::Empty,
        }
    }
}

struct LSystem {
    rng: Rng32,
    pipe_probability: f32,
    elevator_probability: f32,
    termination_probability: f32,
}

impl LSystem {
    fn new(seed: u64, district: DistrictType) -> Self {
        let props = DISTRICTS[district as usize];
        Self {
            rng: Rng32::new(seed),
            pipe_probability: props.pipe_probability,
            elevator_probability: props.elevator_probability,
            termination_probability: if district == DistrictType::Slum {
                0.30
            } else {
                0.05
            },
        }
    }

    fn produce(&mut self, input: &[char], iterations: usize) -> Vec<char> {
        let mut current = input.to_vec();
        for _ in 0..iterations {
            let mut next = Vec::new();
            for symbol in &current {
                if *symbol != 'C' {
                    next.push(*symbol);
                    continue;
                }
                let roll = self.rng.next_f32();
                if roll < 0.60 {
                    next.extend(['C', 'U', 'C']);
                } else if roll < 0.60 + self.pipe_probability {
                    next.extend(['C', '[', '+', 'P', ']']);
                } else if roll < 0.60 + self.pipe_probability * 2.0 {
                    next.extend(['C', '[', '-', 'P', ']']);
                } else if roll < 0.60 + self.pipe_probability * 2.0 + self.elevator_probability {
                    next.extend(['C', '[', '+', 'E', ']']);
                } else if roll
                    < 0.60 + self.pipe_probability * 2.0 + self.elevator_probability + 0.15
                {
                    next.extend(['C', 'S']);
                } else if roll < 1.0 - self.termination_probability {
                    next.push('C');
                }
            }
            current = next;
            if current.len() > 500 {
                break;
            }
        }
        current
    }
}

#[derive(Clone, Copy)]
struct TurtleState {
    x: isize,
    y: isize,
    z: isize,
    dx: isize,
    dz: isize,
}

pub struct MegaStructureGenerator {
    pub(crate) size: usize,
    pub(crate) layers: usize,
    seed: String,
    config: GenerationConfig,
    pub(crate) seed_hash: u64,
    rng: Rng32,
    grid: Vec<CellType>,
    support_map: Vec<bool>,
    district_map: Vec<DistrictType>,
    connections: Vec<ConnectionRecord>,
    rooms: Vec<RoomRecord>,
    transit_nodes: Vec<TransitNodeRecord>,
    transit_edges: Vec<TransitEdgeRecord>,
    transit_attachments: Vec<TransitAttachmentRecord>,
    pattern_counts: BTreeMap<String, usize>,
}

impl MegaStructureGenerator {
    pub fn new(seed: String) -> Self {
        Self::with_config(seed, GenerationConfig::default())
    }

    pub fn with_config(seed: String, config: GenerationConfig) -> Self {
        config.validate().expect("validated generation config");
        let hash = seed_hash(&seed);
        let size = config.grid_size;
        let layers = config.grid_layers;
        let mut generator = Self {
            size,
            layers,
            seed,
            config,
            seed_hash: hash,
            rng: Rng32::new(hash),
            grid: vec![CellType::Empty; size * size * layers],
            support_map: vec![false; size * size * layers],
            district_map: vec![DistrictType::Residential; size * size],
            connections: Vec::new(),
            rooms: Vec::new(),
            transit_nodes: Vec::new(),
            transit_edges: Vec::new(),
            transit_attachments: Vec::new(),
            pattern_counts: BTreeMap::new(),
        };
        generator.generate_district_map();
        generator
    }

    fn idx(&self, x: usize, z: usize, y: usize) -> usize {
        x * self.size * self.layers + z * self.layers + y
    }

    fn district_idx(&self, x: usize, z: usize) -> usize {
        x * self.size + z
    }

    pub(crate) fn get(&self, x: usize, z: usize, y: usize) -> CellType {
        self.grid[self.idx(x, z, y)]
    }

    fn set(&mut self, x: usize, z: usize, y: usize, cell: CellType, supported: bool) {
        let index = self.idx(x, z, y);
        self.grid[index] = cell;
        if supported {
            self.support_map[index] = true;
        }
    }

    fn record_pattern(&mut self, pattern: &str) {
        *self.pattern_counts.entry(pattern.to_owned()).or_insert(0) += 1;
    }

    fn push_connection(&mut self, kind: &str, start: [usize; 3], end: [usize; 3]) {
        self.connections.push(ConnectionRecord {
            kind: kind.to_owned(),
            start,
            end,
        });
        self.record_pattern(kind);
    }

    fn push_room(&mut self, position: [usize; 3], district: DistrictType, label: &str) -> usize {
        let id = self.rooms.len();
        self.rooms.push(RoomRecord {
            id,
            position,
            district: district.name().to_owned(),
            label: label.to_owned(),
        });
        self.record_pattern("landmark_shell");
        id
    }

    fn push_transit_node(&mut self, kind: &str, position: [usize; 3]) -> usize {
        let id = self.transit_nodes.len();
        let district = self.district_at(
            position[0].min(self.size - 1),
            position[2].min(self.size - 1),
        );
        self.transit_nodes.push(TransitNodeRecord {
            id,
            kind: kind.to_owned(),
            position,
            district: district.name().to_owned(),
            stratum: self
                .stratum_at(position[1].min(self.layers - 1))
                .name()
                .to_owned(),
        });
        id
    }

    fn push_transit_edge(
        &mut self,
        kind: &str,
        start_node: usize,
        end_node: usize,
        points: Vec<[usize; 3]>,
    ) -> usize {
        let id = self.transit_edges.len();
        let stratum = points
            .first()
            .map(|point| {
                self.stratum_at(point[1].min(self.layers - 1))
                    .name()
                    .to_owned()
            })
            .unwrap_or_else(|| "SURFACE".to_owned());
        self.transit_edges.push(TransitEdgeRecord {
            id,
            kind: kind.to_owned(),
            start_node,
            end_node,
            length: points.len(),
            points,
            stratum,
        });
        id
    }

    fn push_transit_attachment(
        &mut self,
        route_id: usize,
        room_id: usize,
        attachment_kind: &str,
        position: [usize; 3],
    ) {
        self.transit_attachments.push(TransitAttachmentRecord {
            route_id,
            room_id,
            attachment_kind: attachment_kind.to_owned(),
            position,
        });
    }

    fn support_at(&self, x: usize, z: usize, y: usize) -> bool {
        self.support_map[self.idx(x, z, y)]
    }

    pub(crate) fn district_at(&self, x: usize, z: usize) -> DistrictType {
        self.district_map[self.district_idx(x, z)]
    }

    fn stratum_at(&self, y: usize) -> BiomeStratum {
        configured_stratum_for_y(y, self.layers, self.config.strata_separation)
    }

    pub(crate) fn stratum_name_at(&self, y: usize) -> &'static str {
        self.stratum_at(y).name()
    }

    pub(crate) fn visual_material_at(
        &self,
        x: usize,
        z: usize,
        y: usize,
        cell: CellType,
    ) -> MaterialType {
        if cell == CellType::Facade {
            let district_props = DISTRICTS[self.district_at(x, z) as usize];
            let neon_probability =
                (district_props.neon_probability * self.config.neon_intensity).clamp(0.0, 0.95);
            if hash_noise(self.seed_hash, x, z, y) > 1.0 - neon_probability {
                return MaterialType::Neon;
            }
        }
        cell_to_material(cell)
    }

    fn generate_district_map(&mut self) {
        for x in 0..self.size {
            for z in 0..self.size {
                let noise = (simplex::noise3(x as f32 * 0.05, z as f32 * 0.05, 0.0)
                    + simplex::noise3(x as f32 * 0.10, z as f32 * 0.10, 1.0) * 0.5
                    + simplex::noise3(x as f32 * 0.20, z as f32 * 0.20, 2.0) * 0.25)
                    * self.config.district_contrast;
                let district = if noise < -0.3 {
                    DistrictType::Slum
                } else if noise < -0.1 {
                    DistrictType::Industrial
                } else if noise < 0.1 {
                    DistrictType::Residential
                } else if noise < 0.3 {
                    DistrictType::Commercial
                } else {
                    DistrictType::Elite
                };
                let index = self.district_idx(x, z);
                self.district_map[index] = district;
            }
        }
    }

    pub fn generate(&mut self) {
        self.phase1_skeleton();
        self.phase2_floorplans();
        self.phase2b_circulation_graph();
        self.phase2d_route_aware_generation();
        self.apply_floor_thickness();
        self.phase2c_district_patterns();
        self.phase3_infrastructure();
        self.phase4_erosion();
        self.phase4b_decay_signatures();
        self.ensure_structural_integrity();
        self.add_support_pillars();
        self.carve_traversal_space();
        self.phase5_story_details();
    }

    pub fn seed(&self) -> &str {
        &self.seed
    }

    pub fn config(&self) -> &GenerationConfig {
        &self.config
    }

    pub(crate) fn nearest_room_label(&self, x: usize, y: usize, z: usize) -> Option<&str> {
        self.rooms
            .iter()
            .filter_map(|room| {
                let distance = room.position[0].abs_diff(x)
                    + room.position[1].abs_diff(y)
                    + room.position[2].abs_diff(z);
                (distance <= 4).then_some((distance, room.label.as_str()))
            })
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, label)| label)
    }

    pub(crate) fn nearest_route_label(&self, x: usize, y: usize, z: usize) -> Option<String> {
        self.transit_edges
            .iter()
            .flat_map(|edge| {
                edge.points.iter().map(move |point| {
                    let distance =
                        point[0].abs_diff(x) + point[1].abs_diff(y) + point[2].abs_diff(z);
                    (distance, edge)
                })
            })
            .filter(|(distance, _)| *distance <= 5)
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, edge)| format!("{}#{}", edge.kind, edge.id))
    }

    pub(crate) fn nearest_decay_feature(&self, x: usize, y: usize, z: usize) -> Option<&str> {
        if self.get(x, z, y) == CellType::Debris {
            return Some("DEBRIS_FIELD");
        }
        self.connections
            .iter()
            .filter(|connection| {
                matches!(
                    connection.kind.as_str(),
                    "collapse_scar" | "broken_facade" | "hanging_bridge_remnant" | "debris_field"
                )
            })
            .filter_map(|connection| {
                let distance = connection.start[0].abs_diff(x)
                    + connection.start[1].abs_diff(y)
                    + connection.start[2].abs_diff(z);
                (distance <= 5).then_some((distance, connection.kind.as_str()))
            })
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, kind)| kind)
    }

    pub(crate) fn rooms(&self) -> &[RoomRecord] {
        &self.rooms
    }

    pub(crate) fn transit_graph(&self) -> TransitGraphRecord {
        TransitGraphRecord {
            nodes: self.transit_nodes.clone(),
            edges: self.transit_edges.clone(),
            attachments: self.transit_attachments.clone(),
        }
    }

    fn phase1_skeleton(&mut self) {
        for x in 0..self.size {
            for z in 0..self.size {
                let district = self.district_at(x, z);
                let props = DISTRICTS[district as usize];
                let base_probability =
                    0.15 * props.core_density * self.config.district_density_scale;
                let noise_mod = simplex::noise3(x as f32 * 0.1, z as f32 * 0.1, 3.0) * 0.1;
                if self.rng.next_f32() >= (base_probability + noise_mod).max(0.02) {
                    continue;
                }

                let height_range = (self.layers as f32
                    * props.vertical_variation
                    * self.config.verticality_scale) as usize;
                let min_height = self.layers.saturating_sub(height_range).max(5);
                let max_height = self.layers.saturating_sub(2).max(min_height);
                let height = self.rng.range_usize(min_height, max_height);

                let mut axiom = Vec::new();
                for _ in 0..height {
                    axiom.extend(['C', 'U']);
                }
                let mut lsystem =
                    LSystem::new(self.seed_hash ^ ((x as u64) << 16) ^ z as u64, district);
                let result = lsystem.produce(&axiom, 3);
                self.interpret_lsystem(&result, x, z);

                let base_width: usize = if district == DistrictType::Slum { 1 } else { 2 };
                for y in 0..height {
                    let current_width = base_width.saturating_sub(y / 5).max(1);
                    for dx in -(current_width as isize)..=(current_width as isize) {
                        for dz in -(current_width as isize)..=(current_width as isize) {
                            let nx = x as isize + dx;
                            let nz = z as isize + dz;
                            if nx < 0
                                || nz < 0
                                || nx >= self.size as isize
                                || nz >= self.size as isize
                            {
                                continue;
                            }
                            self.set(nx as usize, nz as usize, y, CellType::Vertical, true);
                        }
                    }
                }
            }
        }
    }

    fn interpret_lsystem(&mut self, symbols: &[char], start_x: usize, start_z: usize) {
        let mut state = TurtleState {
            x: start_x as isize,
            y: 0,
            z: start_z as isize,
            dx: 1,
            dz: 0,
        };
        let mut stack = Vec::new();
        for symbol in symbols {
            match *symbol {
                'C' => {
                    if state.x >= 0
                        && state.y >= 0
                        && state.z >= 0
                        && state.x < self.size as isize
                        && state.z < self.size as isize
                        && state.y < self.layers as isize
                    {
                        self.set(
                            state.x as usize,
                            state.z as usize,
                            state.y as usize,
                            CellType::Vertical,
                            true,
                        );
                    }
                }
                'U' => state.y += 1,
                'P' => {
                    for step in 0..3 {
                        let nx = state.x + state.dx * step;
                        let nz = state.z + state.dz * step;
                        if nx < 0
                            || nz < 0
                            || nx >= self.size as isize
                            || nz >= self.size as isize
                            || state.y < 0
                            || state.y >= self.layers as isize
                        {
                            continue;
                        }
                        let current = self.get(nx as usize, nz as usize, state.y as usize);
                        if current == CellType::Empty {
                            self.set(
                                nx as usize,
                                nz as usize,
                                state.y as usize,
                                CellType::Pipe,
                                false,
                            );
                        }
                    }
                }
                'E' => {
                    for dy in 0..3 {
                        let nx = state.x + state.dx;
                        let nz = state.z + state.dz;
                        let ny = state.y + dy;
                        if nx < 0
                            || nz < 0
                            || ny < 0
                            || nx >= self.size as isize
                            || nz >= self.size as isize
                            || ny >= self.layers as isize
                        {
                            continue;
                        }
                        if self.get(nx as usize, nz as usize, ny as usize) == CellType::Empty {
                            self.set(
                                nx as usize,
                                nz as usize,
                                ny as usize,
                                CellType::Elevator,
                                false,
                            );
                        }
                    }
                }
                'S' => {
                    if state.x >= 0
                        && state.z >= 0
                        && state.y >= 0
                        && state.x < self.size as isize
                        && state.z < self.size as isize
                        && state.y < self.layers as isize
                        && self.get(state.x as usize, state.z as usize, state.y as usize)
                            == CellType::Vertical
                    {
                        self.set(
                            state.x as usize,
                            state.z as usize,
                            state.y as usize,
                            CellType::Stair,
                            true,
                        );
                    }
                }
                '+' => {
                    let previous_dx = state.dx;
                    state.dx = -state.dz;
                    state.dz = previous_dx;
                }
                '-' => {
                    let previous_dx = state.dx;
                    state.dx = state.dz;
                    state.dz = -previous_dx;
                }
                '[' => stack.push(state),
                ']' => {
                    if let Some(previous) = stack.pop() {
                        state = previous;
                    }
                }
                _ => {}
            }
        }
    }

    fn phase2_floorplans(&mut self) {
        for y in 0..self.layers {
            let district = self.district_at(self.size / 2, self.size / 2);
            let stratum = self.stratum_at(y);
            let mut solver = WfcSolver::new(
                self.seed_hash ^ (y as u64 * 12345),
                self.size,
                district,
                stratum,
                self.config.wfc_room_density,
            );
            for x in 0..self.size {
                for z in 0..self.size {
                    match self.get(x, z, y) {
                        CellType::Vertical | CellType::Stair => {
                            solver.constrain(x, z, WFCTile::Stairwell)
                        }
                        CellType::Elevator => solver.constrain(x, z, WFCTile::ElevatorShaft),
                        _ => {}
                    }
                }
            }
            solver.solve();
            for x in 0..self.size {
                for z in 0..self.size {
                    let existing = self.get(x, z, y);
                    if existing != CellType::Empty && existing != CellType::Horizontal {
                        continue;
                    }
                    let tile = solver.cells[solver.idx(x, z)]
                        .collapsed_tile
                        .unwrap_or(WFCTile::Empty as usize);
                    let cell = WfcSolver::tile_to_cell(tile);
                    if cell == CellType::Empty {
                        continue;
                    }
                    let mut adjacent = y > 0 && self.support_at(x, z, y - 1);
                    if !adjacent {
                        for (dx, dz) in [(0isize, 1isize), (1, 0), (0, -1), (-1, 0)] {
                            let nx = x as isize + dx;
                            let nz = z as isize + dz;
                            if nx < 0
                                || nz < 0
                                || nx >= self.size as isize
                                || nz >= self.size as isize
                            {
                                continue;
                            }
                            if self.get(nx as usize, nz as usize, y) != CellType::Empty {
                                adjacent = true;
                                break;
                            }
                        }
                    }
                    if adjacent {
                        self.set(x, z, y, cell, true);
                        if tile == WFCTile::RoomCenter as usize {
                            let local_district = self.district_at(x, z);
                            self.push_room(
                                [x, y, z],
                                local_district,
                                room_label(local_district, stratum),
                            );
                        }
                    }
                }
            }
        }
    }

    fn phase2b_circulation_graph(&mut self) {
        let hubs = self.select_transit_hubs();
        for hub in &hubs {
            self.add_vertical_transit_core(*hub);
        }

        for pair in hubs.windows(2) {
            let start = pair[0];
            let end = pair[1];
            let y = route_layer_for_hubs(start, end, self.layers);
            let kind = route_kind_for_y(y, self.layers);
            self.carve_route(start.0, start.1, end.0, end.1, y, kind);
        }

        if hubs.len() > 2 {
            let first = hubs[0];
            let last = hubs[hubs.len() - 1];
            let y = (self.layers * 2 / 3).clamp(1, self.layers.saturating_sub(1));
            self.carve_route(first.0, first.1, last.0, last.1, y, "express_spine");
        }
    }

    fn select_transit_hubs(&mut self) -> Vec<(usize, usize)> {
        let mut hubs = Vec::new();
        let target = (((4 + (self.size / 14)) as f32) * self.config.route_density)
            .round()
            .clamp(3.0, 12.0) as usize;
        let center = self.size / 2;
        hubs.push((center, center));

        let stride = (self.size / 4).max(4);
        for x in (stride / 2..self.size).step_by(stride) {
            for z in (stride / 2..self.size).step_by(stride) {
                if hubs.len() >= target {
                    break;
                }
                let district = self.district_at(x, z);
                let density_bias = match district {
                    DistrictType::Commercial | DistrictType::Slum => 0.75,
                    DistrictType::Industrial => 0.60,
                    DistrictType::Residential => 0.45,
                    DistrictType::Elite => 0.35,
                };
                let noise = hash_noise(self.seed_hash, x, z, 0);
                if noise < (density_bias * self.config.route_density).clamp(0.05, 0.95) {
                    hubs.push((x, z));
                }
            }
            if hubs.len() >= target {
                break;
            }
        }

        while hubs.len() < target {
            hubs.push((
                self.rng.range_usize(2, self.size.saturating_sub(3).max(2)),
                self.rng.range_usize(2, self.size.saturating_sub(3).max(2)),
            ));
        }

        hubs.sort_unstable();
        hubs.dedup();
        hubs
    }

    fn add_vertical_transit_core(&mut self, hub: (usize, usize)) {
        let (x, z) = hub;
        let district = self.district_at(x, z);
        let bottom_node = self.push_transit_node("vertical_core_base", [x, 0, z]);
        let top_node =
            self.push_transit_node("vertical_core_top", [x, self.layers.saturating_sub(1), z]);
        for y in 0..self.layers {
            let cell = if y % 4 == 0 {
                CellType::Elevator
            } else {
                CellType::Stair
            };
            self.set(x, z, y, cell, true);
            if x + 1 < self.size {
                self.set(x + 1, z, y, CellType::Vertical, true);
            }
            if z + 1 < self.size && district != DistrictType::Elite {
                self.set(x, z + 1, y, CellType::Vertical, true);
            }
        }
        let hub_y = (self.layers / 3).max(1);
        self.push_room(
            [x, hub_y, z],
            district,
            &format!("{}_TRANSIT_HUB", self.stratum_at(hub_y).name()),
        );
        self.push_connection(
            "vertical_transit_core",
            [x, 0, z],
            [x, self.layers.saturating_sub(1), z],
        );
        let points = (0..self.layers).map(|y| [x, y, z]).collect();
        self.push_transit_edge("vertical_transit_core", bottom_node, top_node, points);
    }

    fn carve_route(
        &mut self,
        start_x: usize,
        start_z: usize,
        end_x: usize,
        end_z: usize,
        y: usize,
        kind: &str,
    ) {
        let mut x = start_x as isize;
        let mut z = start_z as isize;
        let end_x = end_x as isize;
        let end_z = end_z as isize;
        let y = y.min(self.layers.saturating_sub(1));
        let route_cell = match kind {
            "service_tunnel" => CellType::Horizontal,
            "skybridge" | "express_spine" => CellType::Bridge,
            _ => CellType::Horizontal,
        };
        let start_node = self.push_transit_node("route_junction", [start_x, y, start_z]);
        let end_node =
            self.push_transit_node("route_junction", [end_x as usize, y, end_z as usize]);
        let mut points = Vec::new();

        while x != end_x {
            self.paint_route_cell(x as usize, z as usize, y, route_cell, kind);
            points.push([x as usize, y, z as usize]);
            x += (end_x - x).signum();
        }
        while z != end_z {
            self.paint_route_cell(x as usize, z as usize, y, route_cell, kind);
            points.push([x as usize, y, z as usize]);
            z += (end_z - z).signum();
        }
        self.paint_route_cell(x as usize, z as usize, y, route_cell, kind);
        points.push([x as usize, y, z as usize]);
        let route_id = self.push_transit_edge(kind, start_node, end_node, points);
        self.attach_route_rooms(
            route_id,
            (start_x, start_z),
            (end_x as usize, end_z as usize),
            y,
            kind,
        );

        self.push_connection(
            kind,
            [start_x, y, start_z],
            [end_x as usize, y, end_z as usize],
        );
    }

    fn paint_route_cell(&mut self, x: usize, z: usize, y: usize, route_cell: CellType, kind: &str) {
        if x >= self.size || z >= self.size || y >= self.layers {
            return;
        }
        self.set(x, z, y, route_cell, true);
        if y > 0 && kind == "service_tunnel" {
            self.set(x, z, y - 1, CellType::Pipe, false);
        }
        for (dx, dz) in [(1isize, 0isize), (0, 1)] {
            let nx = x as isize + dx;
            let nz = z as isize + dz;
            if nx < 0 || nz < 0 || nx >= self.size as isize || nz >= self.size as isize {
                continue;
            }
            if self.get(nx as usize, nz as usize, y) == CellType::Empty {
                self.set(nx as usize, nz as usize, y, CellType::Horizontal, true);
            }
        }
    }

    fn attach_route_rooms(
        &mut self,
        route_id: usize,
        start: (usize, usize),
        end: (usize, usize),
        y: usize,
        kind: &str,
    ) {
        let steps = start.0.abs_diff(end.0).max(start.1.abs_diff(end.1)).max(1);
        let interval = (steps / 3).max(2);
        for step in (0..=steps).step_by(interval) {
            let t = step as f32 / steps as f32;
            let x = lerpf(start.0 as f32, end.0 as f32, t).round() as usize;
            let z = lerpf(start.1 as f32, end.1 as f32, t).round() as usize;
            let district = self.district_at(x.min(self.size - 1), z.min(self.size - 1));
            let stratum = self.stratum_at(y);
            let label = route_room_label(kind, district, stratum);
            let position = [x.min(self.size - 1), y, z.min(self.size - 1)];
            let room_id = self.push_room(
                [x.min(self.size - 1), y, z.min(self.size - 1)],
                district,
                label,
            );
            self.push_transit_attachment(route_id, room_id, "route_room", position);
            self.paint_route_attachment(x, z, y, kind);
        }
        self.add_route_dead_end(route_id, end.0, end.1, y, kind);
        if steps > self.size / 3 {
            self.add_route_chokepoint(
                route_id,
                (start.0 + end.0) / 2,
                (start.1 + end.1) / 2,
                y,
                kind,
            );
        }
    }

    fn paint_route_attachment(&mut self, x: usize, z: usize, y: usize, kind: &str) {
        for (dx, dz) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
            let nx = x as isize + dx;
            let nz = z as isize + dz;
            if nx < 0 || nz < 0 || nx >= self.size as isize || nz >= self.size as isize {
                continue;
            }
            let nx = nx as usize;
            let nz = nz as usize;
            let cell = match kind {
                "service_tunnel" => CellType::Pipe,
                "skybridge" | "express_spine" => CellType::Facade,
                _ => CellType::Horizontal,
            };
            if self.get(nx, nz, y) == CellType::Empty {
                self.set(nx, nz, y, cell, cell == CellType::Horizontal);
            }
        }
    }

    fn add_route_dead_end(&mut self, route_id: usize, x: usize, z: usize, y: usize, kind: &str) {
        let district = self.district_at(x.min(self.size - 1), z.min(self.size - 1));
        let label = match kind {
            "service_tunnel" => "MAINTENANCE_DEAD_END",
            "skybridge" | "express_spine" => "SKYBRIDGE_TERMINAL",
            _ => "CORRIDOR_DEAD_END",
        };
        let position = [x.min(self.size - 1), y, z.min(self.size - 1)];
        let room_id = self.push_room(position, district, label);
        self.push_transit_node("dead_end", position);
        self.push_transit_attachment(route_id, room_id, "dead_end", position);
        let connection_kind = kind_name(kind, "dead_end");
        self.push_connection(&connection_kind, [x, y, z], [x, y, z]);
    }

    fn add_route_chokepoint(&mut self, route_id: usize, x: usize, z: usize, y: usize, kind: &str) {
        let x = x.min(self.size - 1);
        let z = z.min(self.size - 1);
        self.set(x, z, y, CellType::Stair, true);
        if y + 1 < self.layers {
            self.set(x, z, y + 1, CellType::Elevator, false);
        }
        let district = self.district_at(x, z);
        let position = [x, y, z];
        let room_id = self.push_room(position, district, "ROUTE_CHOKEPOINT");
        self.push_transit_node("chokepoint", position);
        self.push_transit_attachment(route_id, room_id, "chokepoint", position);
        let connection_kind = kind_name(kind, "chokepoint");
        self.push_connection(&connection_kind, [x, y, z], [x, y, z]);
    }

    fn phase2d_route_aware_generation(&mut self) {
        let edges = self.transit_edges.clone();
        for edge in edges {
            if edge.points.len() < 2 {
                continue;
            }
            let slots = ((edge.points.len() as f32 * 0.16 * self.config.route_density).round()
                as usize)
                .clamp(1, 6);
            for slot in 0..slots {
                let index =
                    ((slot + 1) * edge.points.len() / (slots + 1)).min(edge.points.len() - 1);
                self.add_route_aware_feature(edge.id, &edge.kind, edge.points[index], slot);
            }
        }
    }

    fn add_route_aware_feature(
        &mut self,
        route_id: usize,
        route_kind: &str,
        position: [usize; 3],
        slot: usize,
    ) {
        let x = position[0].min(self.size - 1);
        let y = position[1].min(self.layers - 1);
        let z = position[2].min(self.size - 1);
        let district = self.district_at(x, z);
        let label = route_aware_room_label(
            route_kind,
            district,
            self.stratum_at(y),
            self.config.landmark_frequency,
            hash_noise(self.seed_hash, x + slot, z, y),
        );
        let room_id = self.push_room([x, y, z], district, label);
        self.push_transit_attachment(route_id, room_id, "route_aware_feature", [x, y, z]);
        self.paint_route_aware_feature(x, z, y, route_kind, label);
        self.record_pattern(label);
        self.push_connection(
            &format!("route_attached_{}", label.to_lowercase()),
            [x, y, z],
            [x, y, z],
        );
    }

    fn paint_route_aware_feature(
        &mut self,
        x: usize,
        z: usize,
        y: usize,
        route_kind: &str,
        label: &str,
    ) {
        let shell = match label {
            "PIPE_JUNCTION" | "MAINTENANCE_CHECKPOINT" => CellType::Pipe,
            "MARKET_STALL" | "PATCH_BAZAAR" => CellType::Facade,
            "SECURITY_GATE" | "SKY_SECURITY_GATE" => CellType::Stair,
            "DATA_RELAY" | "DATA_VAULT" => CellType::Facade,
            _ => CellType::Horizontal,
        };
        for (dx, dz) in [(0isize, 0isize), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            let nx = x as isize + dx;
            let nz = z as isize + dz;
            if nx < 0 || nz < 0 || nx >= self.size as isize || nz >= self.size as isize {
                continue;
            }
            let nx = nx as usize;
            let nz = nz as usize;
            if self.get(nx, nz, y) == CellType::Empty || dx == 0 && dz == 0 {
                self.set(
                    nx,
                    nz,
                    y,
                    shell,
                    matches!(shell, CellType::Horizontal | CellType::Stair),
                );
            }
            if route_kind == "service_tunnel" && y > 0 && self.get(nx, nz, y - 1) == CellType::Empty
            {
                self.set(nx, nz, y - 1, CellType::Pipe, false);
            }
            if matches!(label, "DATA_RELAY" | "DATA_VAULT") && y + 1 < self.layers {
                self.set(nx, nz, y + 1, CellType::Antenna, false);
            }
        }
    }

    fn phase2c_district_patterns(&mut self) {
        self.add_industrial_service_trunks();
        self.add_slum_patchwork_walkways();
        self.add_elite_void_courts();
        self.add_commercial_neon_facades();
        self.add_stratum_markers();
    }

    fn add_industrial_service_trunks(&mut self) {
        let step = (self.size / 6).max(4);
        for x in (1..self.size).step_by(step) {
            for z in 0..self.size {
                if self.district_at(x, z) != DistrictType::Industrial {
                    continue;
                }
                for y in 1..self.layers.saturating_sub(1) {
                    if y % 3 == 0 || self.get(x, z, y) == CellType::Empty {
                        self.set(x, z, y, CellType::Pipe, false);
                    }
                }
                self.push_connection(
                    "industrial_service_trunk",
                    [x, 1, z],
                    [x, self.layers.saturating_sub(2), z],
                );
            }
        }
    }

    fn add_slum_patchwork_walkways(&mut self) {
        let attempts = ((self.size as f32) * self.config.bridge_frequency * 0.55) as usize;
        for _ in 0..attempts {
            let x = self.rng.range_usize(1, self.size.saturating_sub(2).max(1));
            let z = self.rng.range_usize(1, self.size.saturating_sub(2).max(1));
            if self.district_at(x, z) != DistrictType::Slum {
                continue;
            }
            let y = self.rng.range_usize(
                2,
                (self.layers * 2 / 3)
                    .max(2)
                    .min(self.layers.saturating_sub(1)),
            );
            let length = self.rng.range_usize(3, 8);
            let horizontal = self.rng.next_f32() < 0.5;
            let mut end = (x, z);
            for offset in 0..length {
                let nx = if horizontal {
                    (x + offset).min(self.size - 1)
                } else {
                    x
                };
                let nz = if horizontal {
                    z
                } else {
                    (z + offset).min(self.size - 1)
                };
                self.set(nx, nz, y, CellType::Bridge, true);
                if y + 1 < self.layers && self.rng.next_f32() < 0.35 {
                    self.set(nx, nz, y + 1, CellType::Cable, false);
                }
                end = (nx, nz);
            }
            self.push_connection("slum_patchwalk", [x, y, z], [end.0, y, end.1]);
        }
    }

    fn add_elite_void_courts(&mut self) {
        let mut carved = 0usize;
        for x in (3..self.size.saturating_sub(3)).step_by(7) {
            for z in (3..self.size.saturating_sub(3)).step_by(7) {
                if carved > self.size / 3 || self.district_at(x, z) != DistrictType::Elite {
                    continue;
                }
                let radius = 1 + (hash_noise(self.seed_hash, x, z, 8) > 0.65) as usize;
                let y_start = self.layers / 3;
                for dx in -(radius as isize)..=(radius as isize) {
                    for dz in -(radius as isize)..=(radius as isize) {
                        let nx = (x as isize + dx).clamp(0, self.size as isize - 1) as usize;
                        let nz = (z as isize + dz).clamp(0, self.size as isize - 1) as usize;
                        for y in y_start..self.layers {
                            let index = self.idx(nx, nz, y);
                            self.grid[index] = CellType::Empty;
                            self.support_map[index] = false;
                        }
                    }
                }
                self.push_room([x, y_start, z], DistrictType::Elite, "SKYLINE_VOID_COURT");
                carved += 1;
            }
        }
    }

    fn add_commercial_neon_facades(&mut self) {
        for x in 0..self.size {
            for z in 0..self.size {
                if self.district_at(x, z) != DistrictType::Commercial {
                    continue;
                }
                for y in (self.layers / 4)..self.layers {
                    if hash_noise(self.seed_hash, x, z, y) > 0.86
                        && self.get(x, z, y) == CellType::Empty
                    {
                        self.set(x, z, y, CellType::Facade, false);
                    }
                }
            }
        }
    }

    fn add_stratum_markers(&mut self) {
        for y in 0..self.layers {
            match self.stratum_at(y) {
                BiomeStratum::Underground if y > 0 => {
                    for x in (0..self.size).step_by(5) {
                        let z = (self.seed_hash as usize + x + y) % self.size;
                        if self.get(x, z, y) == CellType::Empty {
                            self.set(x, z, y, CellType::Pipe, false);
                        }
                    }
                }
                BiomeStratum::Skyline => {
                    for x in (2..self.size).step_by(9) {
                        let z = (x * 3 + y + self.seed_hash as usize) % self.size;
                        if self.get(x, z, y) != CellType::Empty && y + 1 < self.layers {
                            self.set(x, z, y + 1, CellType::Antenna, false);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn phase3_infrastructure(&mut self) {
        self.add_spline_bridges();
        self.add_spline_cables();
        self.add_spline_pipes();
        self.add_rooftop_details();
        self.add_external_elevators();
    }

    fn apply_floor_thickness(&mut self) {
        for x in 0..self.size {
            for z in 0..self.size {
                let thickness = DISTRICTS[self.district_at(x, z) as usize]
                    .floor_thickness
                    .max(1) as usize;
                for y in 0..self.layers {
                    if self.get(x, z, y) != CellType::Horizontal {
                        continue;
                    }
                    for extra in 1..thickness {
                        if y + extra >= self.layers {
                            break;
                        }
                        if self.get(x, z, y + extra) == CellType::Empty {
                            self.set(x, z, y + extra, CellType::Horizontal, true);
                        }
                    }
                }
            }
        }
    }

    fn add_spline_bridges(&mut self) {
        for _ in
            0..((self.size * self.layers) as f32 * 0.02 * self.config.bridge_frequency) as usize
        {
            let y = self
                .rng
                .range_usize(3, self.layers.saturating_sub(2).max(3));
            let mut cores = Vec::new();
            for x in 0..self.size {
                for z in 0..self.size {
                    if self.get(x, z, y) == CellType::Vertical {
                        cores.push((x, z));
                    }
                }
            }
            if cores.len() < 2 {
                continue;
            }
            let start = cores[self.rng.choose_index(cores.len())];
            let mut end = cores[self.rng.choose_index(cores.len())];
            if start == end {
                end = cores[(self.rng.choose_index(cores.len()) + 1) % cores.len()];
            }
            if start == end {
                continue;
            }
            let arch = 1.2
                + simplex::noise3(start.0 as f32 * 0.1, y as f32 * 0.1, start.1 as f32 * 0.1).abs()
                    * 1.4;
            let p0 = vec3(start.0 as f32, y as f32, start.1 as f32);
            let p1 = vec3(start.0 as f32, y as f32, start.1 as f32);
            let p2 = vec3(end.0 as f32, y as f32 + arch, end.1 as f32);
            let p3 = vec3(end.0 as f32, y as f32, end.1 as f32);
            for (x, z, yv) in rasterize_spline(p0, p1, p2, p3, 30) {
                if x >= self.size || z >= self.size || yv >= self.layers {
                    continue;
                }
                if self.get(x, z, yv) == CellType::Empty {
                    self.set(x, z, yv, CellType::Bridge, true);
                }
            }
            self.push_connection("bridge", [start.0, y, start.1], [end.0, y, end.1]);
        }
    }

    fn add_spline_cables(&mut self) {
        for _ in 0..((self.size as f32) * 0.5 * self.config.cable_frequency) as usize {
            let mut cores = Vec::new();
            for x in 0..self.size {
                for z in 0..self.size {
                    for y in 0..self.layers {
                        if self.get(x, z, y) == CellType::Vertical {
                            cores.push((x, z, y));
                        }
                    }
                }
            }
            if cores.len() < 2 {
                continue;
            }
            let start = cores[self.rng.choose_index(cores.len())];
            let end = cores[self.rng.choose_index(cores.len())];
            let manhattan = start.0.abs_diff(end.0) + start.1.abs_diff(end.1);
            if !(3..=15).contains(&manhattan) {
                continue;
            }
            let droop = start.2.min(end.2) as f32 - 2.0;
            let p0 = vec3(start.0 as f32, start.2 as f32, start.1 as f32);
            let p1 = vec3(
                (start.0 + end.0) as f32 * 0.5,
                droop,
                (start.1 + end.1) as f32 * 0.5,
            );
            let p2 = vec3(
                (start.0 + end.0) as f32 * 0.5,
                droop + 0.5,
                (start.1 + end.1) as f32 * 0.5,
            );
            let p3 = vec3(end.0 as f32, end.2 as f32, end.1 as f32);
            for (x, z, y) in rasterize_spline(p0, p1, p2, p3, 25) {
                if x >= self.size || z >= self.size || y >= self.layers {
                    continue;
                }
                if self.get(x, z, y) == CellType::Empty {
                    self.set(x, z, y, CellType::Cable, false);
                }
            }
            self.push_connection("cable", [start.0, start.2, start.1], [end.0, end.2, end.1]);
        }
    }

    fn add_spline_pipes(&mut self) {
        for _ in 0..((self.size * self.layers) as f32 * 0.03 * self.config.pipe_frequency) as usize
        {
            let x = self.rng.range_usize(0, self.size.saturating_sub(1));
            let z = self.rng.range_usize(0, self.size.saturating_sub(1));
            let y = self.rng.range_usize(1, self.layers.saturating_sub(2));
            let base = self.get(x, z, y);
            if base != CellType::Vertical && base != CellType::Horizontal {
                continue;
            }
            let direction = self.rng.choose_index(4);
            let ddx = [1isize, -1, 0, 0];
            let ddz = [0isize, 0, 1, -1];
            let mut cx = x as isize;
            let mut cz = z as isize;
            let mut waypoints = vec![vec3(x as f32, y as f32, z as f32)];
            for _ in 0..3 {
                cx += ddx[direction] * (3 + self.rng.range_usize(0, 2) as isize);
                cz += ddz[direction] * (self.rng.range_usize(0, 1) as isize - 1);
                cx = cx.clamp(0, self.size as isize - 1);
                cz = cz.clamp(0, self.size as isize - 1);
                waypoints.push(vec3(cx as f32, y as f32, cz as f32));
            }
            if waypoints.len() >= 4 {
                for (px, pz, py) in
                    rasterize_spline(waypoints[0], waypoints[1], waypoints[2], waypoints[3], 20)
                {
                    if px >= self.size || pz >= self.size || py >= self.layers {
                        continue;
                    }
                    if self.get(px, pz, py) == CellType::Empty {
                        self.set(px, pz, py, CellType::Pipe, false);
                    }
                }
                self.push_connection("pipe", [x, y, z], [cx as usize, y, cz as usize]);
            }
        }
    }

    fn add_rooftop_details(&mut self) {
        for x in 0..self.size {
            for z in 0..self.size {
                for y in (0..self.layers).rev() {
                    if self.get(x, z, y) == CellType::Empty {
                        continue;
                    }
                    if self.rng.next_f32() < 0.15 && y < self.layers - 1 {
                        let detail = if self.rng.next_f32() < 0.5 {
                            CellType::Antenna
                        } else {
                            CellType::Vent
                        };
                        let height = self.rng.range_usize(1, 3);
                        for dy in 1..=height {
                            if y + dy < self.layers {
                                self.set(x, z, y + dy, detail, false);
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    fn add_external_elevators(&mut self) {
        let directions = [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)];
        for x in 0..self.size {
            for z in 0..self.size {
                let is_core = (0..self.layers).any(|y| self.get(x, z, y) == CellType::Vertical);
                if !is_core || self.rng.next_f32() > 0.2 {
                    continue;
                }
                for (dx, dz) in directions {
                    let nx = x as isize + dx;
                    let nz = z as isize + dz;
                    if nx < 0 || nz < 0 || nx >= self.size as isize || nz >= self.size as isize {
                        continue;
                    }
                    for y in 0..self.layers {
                        let nx = nx as usize;
                        let nz = nz as usize;
                        if self.get(nx, nz, y) == CellType::Empty
                            && (y == 0
                                || matches!(
                                    self.get(nx, nz, y - 1),
                                    CellType::Elevator | CellType::Horizontal | CellType::Vertical
                                ))
                        {
                            self.set(nx, nz, y, CellType::Elevator, false);
                        }
                    }
                    break;
                }
            }
        }
    }

    fn phase4_erosion(&mut self) {
        self.erosion_structural_weakening();
        self.erosion_collapse_propagation();
    }

    fn phase4b_decay_signatures(&mut self) {
        let scars = ((self.size as f32)
            * self.config.erosion_strength
            * self.config.decay_story_density
            * 0.18) as usize;
        for _ in 0..scars {
            let x = self.rng.range_usize(2, self.size.saturating_sub(3).max(2));
            let z = self.rng.range_usize(2, self.size.saturating_sub(3).max(2));
            let y = self
                .rng
                .range_usize(2, self.layers.saturating_sub(2).max(2));
            if self.get(x, z, y) == CellType::Empty {
                continue;
            }
            let radius = if self.rng.next_f32() < 0.25 { 2 } else { 1 };
            let mut removed = 0usize;
            for dx in -(radius as isize)..=(radius as isize) {
                for dz in -(radius as isize)..=(radius as isize) {
                    for dy in 0..=1 {
                        let nx = x as isize + dx;
                        let nz = z as isize + dz;
                        let ny = y as isize + dy;
                        if nx < 0
                            || nz < 0
                            || ny < 0
                            || nx >= self.size as isize
                            || nz >= self.size as isize
                            || ny >= self.layers as isize
                        {
                            continue;
                        }
                        let index = self.idx(nx as usize, nz as usize, ny as usize);
                        if self.grid[index] != CellType::Empty {
                            self.grid[index] = CellType::Empty;
                            self.support_map[index] = false;
                            removed += 1;
                        }
                    }
                }
            }
            if y > 0 {
                for offset in 0..=radius {
                    let nx = (x + offset).min(self.size - 1);
                    if self.get(nx, z, y - 1) == CellType::Empty {
                        self.set(nx, z, y - 1, CellType::Horizontal, false);
                    }
                }
            }
            if removed > 0 {
                self.push_connection(
                    "collapse_scar",
                    [x, y, z],
                    [x, y.saturating_add(1).min(self.layers - 1), z],
                );
            }
        }
    }

    fn phase5_story_details(&mut self) {
        self.add_landmark_rooms();
        self.add_debris_fields();
        self.add_hanging_bridge_remnants();
        self.add_broken_facade_fields();
    }

    fn add_landmark_rooms(&mut self) {
        let attempts = ((self.size as f32 / 2.0) * self.config.landmark_frequency) as usize;
        let attempts = attempts.max(4);
        for _ in 0..attempts {
            let x = self.rng.range_usize(1, self.size.saturating_sub(2).max(1));
            let z = self.rng.range_usize(1, self.size.saturating_sub(2).max(1));
            let y = self
                .rng
                .range_usize(1, self.layers.saturating_sub(2).max(1));
            if self.get(x, z, y) == CellType::Empty {
                continue;
            }
            let district = self.district_at(x, z);
            let stratum = self.stratum_at(y);
            let label = match (district, stratum) {
                (
                    DistrictType::Elite | DistrictType::Commercial,
                    BiomeStratum::Midrise | BiomeStratum::Skyline,
                ) => "DATA_VAULT",
                (
                    DistrictType::Slum | DistrictType::Residential,
                    BiomeStratum::Underground | BiomeStratum::Surface,
                ) => "SHRINE",
                (DistrictType::Industrial, BiomeStratum::Underground | BiomeStratum::Surface) => {
                    "MAINTENANCE_SHAFT"
                }
                _ => continue,
            };
            self.push_room([x, y, z], district, label);
            self.mark_landmark_shell(x, z, y, label);
        }
    }

    fn mark_landmark_shell(&mut self, x: usize, z: usize, y: usize, label: &str) {
        for (dx, dz) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
            let nx = x as isize + dx;
            let nz = z as isize + dz;
            if nx < 0 || nz < 0 || nx >= self.size as isize || nz >= self.size as isize {
                continue;
            }
            let shell_cell = match label {
                "DATA_VAULT" => CellType::Facade,
                "SHRINE" => CellType::Cable,
                _ => CellType::Pipe,
            };
            if self.get(nx as usize, nz as usize, y) == CellType::Empty {
                self.set(nx as usize, nz as usize, y, shell_cell, false);
            }
        }
        self.record_pattern(label);
    }

    fn add_debris_fields(&mut self) {
        let scars: Vec<_> = self
            .connections
            .iter()
            .filter(|connection| connection.kind == "collapse_scar")
            .map(|connection| connection.start)
            .collect();
        for scar in scars {
            self.paint_debris_field(scar[0], scar[2], scar[1], "debris_field");
        }
        let random_fields = ((self.size as f32)
            * self.config.erosion_strength
            * self.config.decay_story_density
            * 0.08) as usize;
        for _ in 0..random_fields {
            let x = self.rng.range_usize(1, self.size.saturating_sub(2).max(1));
            let z = self.rng.range_usize(1, self.size.saturating_sub(2).max(1));
            let y = self.rng.range_usize(0, (self.layers / 2).max(1));
            self.paint_debris_field(x, z, y, "debris_field");
        }
    }

    fn paint_debris_field(&mut self, x: usize, z: usize, y: usize, pattern: &str) {
        let radius = if self.config.erosion_strength > 1.25 {
            2
        } else {
            1
        };
        for dx in -(radius as isize)..=(radius as isize) {
            for dz in -(radius as isize)..=(radius as isize) {
                let nx = x as isize + dx;
                let nz = z as isize + dz;
                if nx < 0 || nz < 0 || nx >= self.size as isize || nz >= self.size as isize {
                    continue;
                }
                let debris_y = y.saturating_sub((dx.abs() + dz.abs()) as usize % 2);
                if self.get(nx as usize, nz as usize, debris_y) == CellType::Empty
                    || self.rng.next_f32() < 0.45
                {
                    self.set(nx as usize, nz as usize, debris_y, CellType::Debris, false);
                }
            }
        }
        self.record_pattern(pattern);
    }

    fn add_hanging_bridge_remnants(&mut self) {
        let attempts = ((self.size as f32)
            * self.config.erosion_strength
            * self.config.decay_story_density
            * 0.10) as usize;
        for _ in 0..attempts {
            let x = self.rng.range_usize(2, self.size.saturating_sub(5).max(2));
            let z = self.rng.range_usize(2, self.size.saturating_sub(5).max(2));
            let y = self.rng.range_usize(
                (self.layers / 2).max(2),
                self.layers.saturating_sub(2).max(2),
            );
            let length = self.rng.range_usize(2, 5);
            let horizontal = self.rng.next_f32() < 0.5;
            let mut end = (x, z);
            for offset in 0..length {
                let nx = if horizontal {
                    (x + offset).min(self.size - 1)
                } else {
                    x
                };
                let nz = if horizontal {
                    z
                } else {
                    (z + offset).min(self.size - 1)
                };
                self.set(nx, nz, y, CellType::Bridge, false);
                if y > 0 {
                    self.set(nx, nz, y - 1, CellType::Cable, false);
                }
                if offset == length - 1 && y > 1 {
                    self.set(nx, nz, y - 2, CellType::Debris, false);
                }
                end = (nx, nz);
            }
            self.push_connection("hanging_bridge_remnant", [x, y, z], [end.0, y, end.1]);
        }
    }

    fn add_broken_facade_fields(&mut self) {
        let attempts = ((self.size * self.layers) as f32
            * self.config.erosion_strength
            * self.config.decay_story_density
            * 0.006) as usize;
        for _ in 0..attempts {
            let x = self.rng.range_usize(1, self.size.saturating_sub(2).max(1));
            let z = self.rng.range_usize(1, self.size.saturating_sub(2).max(1));
            let y = self
                .rng
                .range_usize(2, self.layers.saturating_sub(1).max(2));
            if self.get(x, z, y) == CellType::Empty {
                continue;
            }
            self.set(x, z, y, CellType::Debris, false);
            if y + 1 < self.layers && self.get(x, z, y + 1) == CellType::Empty {
                self.set(x, z, y + 1, CellType::Facade, false);
            }
            self.push_connection("broken_facade", [x, y, z], [x, y, z]);
        }
    }

    fn count_empty_neighbors(&self, x: usize, z: usize, y: usize) -> usize {
        let directions = [
            (-1isize, 0isize, 0isize),
            (1, 0, 0),
            (0, 0, -1),
            (0, 0, 1),
            (0, -1, 0),
            (0, 1, 0),
        ];
        let mut count = 0;
        for (dx, dy, dz) in directions {
            let nx = x as isize + dx;
            let ny = y as isize + dy;
            let nz = z as isize + dz;
            if nx < 0
                || ny < 0
                || nz < 0
                || nx >= self.size as isize
                || ny >= self.layers as isize
                || nz >= self.size as isize
            {
                count += 1;
                continue;
            }
            if self.get(nx as usize, nz as usize, ny as usize) == CellType::Empty {
                count += 1;
            }
        }
        count
    }

    fn erosion_structural_weakening(&mut self) {
        for x in 0..self.size {
            for z in 0..self.size {
                for y in 0..self.layers {
                    if self.get(x, z, y) == CellType::Empty {
                        continue;
                    }
                    let exposure = self.count_empty_neighbors(x, z, y);
                    let noise =
                        simplex::noise3(x as f32 * 0.2, y as f32 * 0.2, z as f32 * 0.2) * 0.5 + 0.5;
                    let base_threshold = if self.stratum_at(y) == BiomeStratum::Skyline {
                        0.3
                    } else {
                        0.6
                    };
                    let threshold =
                        (base_threshold / self.config.erosion_strength.max(0.01)).clamp(0.05, 0.95);
                    if exposure >= 4 && noise > threshold {
                        let index = self.idx(x, z, y);
                        self.grid[index] = CellType::Empty;
                        self.support_map[index] = false;
                    }
                }
            }
        }
    }

    fn has_support(&self, x: usize, z: usize, y: usize) -> bool {
        if y == 0 {
            return true;
        }
        if self.support_at(x, z, y - 1) {
            return true;
        }
        for (dx, dz) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
            let nx = x as isize + dx;
            let nz = z as isize + dz;
            if nx < 0 || nz < 0 || nx >= self.size as isize || nz >= self.size as isize {
                continue;
            }
            let neighbor = self.get(nx as usize, nz as usize, y);
            if matches!(neighbor, CellType::Horizontal | CellType::Bridge) {
                return true;
            }
        }
        false
    }

    fn erosion_collapse_propagation(&mut self) {
        for _ in 0..3 {
            for x in 0..self.size {
                for z in 0..self.size {
                    for y in 1..self.layers {
                        let cell = self.get(x, z, y);
                        if cell == CellType::Empty || cell == CellType::Vertical {
                            continue;
                        }
                        if !self.has_support(x, z, y) {
                            let index = self.idx(x, z, y);
                            self.grid[index] = CellType::Empty;
                            self.support_map[index] = false;
                        }
                    }
                }
            }
        }
    }

    fn ensure_structural_integrity(&mut self) {
        for y in 1..self.layers {
            for x in 0..self.size {
                for z in 0..self.size {
                    let cell = self.get(x, z, y);
                    if matches!(cell, CellType::Horizontal | CellType::Facade)
                        && !self.has_support(x, z, y)
                    {
                        let index = self.idx(x, z, y);
                        self.grid[index] = CellType::Empty;
                        self.support_map[index] = false;
                    }
                }
            }
        }
    }

    fn add_support_pillars(&mut self) {
        for x in 0..self.size {
            for z in 0..self.size {
                for y in 1..self.layers {
                    if self.get(x, z, y) == CellType::Horizontal && !self.has_support(x, z, y) {
                        for py in (0..y).rev() {
                            if self.get(x, z, py) == CellType::Empty {
                                self.set(x, z, py, CellType::Vertical, true);
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    fn carve_traversal_space(&mut self) {
        for y in 0..self.layers.saturating_sub(2) {
            for x in 0..self.size {
                for z in 0..self.size {
                    if !is_walkable_floor_cell(self.get(x, z, y)) {
                        continue;
                    }
                    for dy in 1..=2 {
                        if y + dy >= self.layers {
                            continue;
                        }
                        let above = self.get(x, z, y + dy);
                        if is_traversal_carveable_cell(above) {
                            let index = self.idx(x, z, y + dy);
                            self.grid[index] = CellType::Empty;
                            self.support_map[index] = false;
                        }
                    }
                }
            }
        }
    }

    fn district_records(&self) -> Vec<DistrictRecord> {
        let mut footprint = [0usize; DistrictType::COUNT];
        let mut occupied = [0usize; DistrictType::COUNT];
        let mut min_bounds = [[usize::MAX; 2]; DistrictType::COUNT];
        let mut max_bounds = [[0usize; 2]; DistrictType::COUNT];

        for x in 0..self.size {
            for z in 0..self.size {
                let district = self.district_at(x, z);
                let index = district as usize;
                footprint[index] += 1;
                min_bounds[index][0] = min_bounds[index][0].min(x);
                min_bounds[index][1] = min_bounds[index][1].min(z);
                max_bounds[index][0] = max_bounds[index][0].max(x);
                max_bounds[index][1] = max_bounds[index][1].max(z);
                for y in 0..self.layers {
                    if self.get(x, z, y) != CellType::Empty {
                        occupied[index] += 1;
                    }
                }
            }
        }

        [
            DistrictType::Industrial,
            DistrictType::Residential,
            DistrictType::Commercial,
            DistrictType::Slum,
            DistrictType::Elite,
        ]
        .into_iter()
        .filter(|district| footprint[*district as usize] > 0)
        .enumerate()
        .map(|(id, district)| {
            let index = district as usize;
            let total = (footprint[index] * self.layers).max(1);
            DistrictRecord {
                id,
                kind: district.name().to_owned(),
                bounds_min: min_bounds[index],
                bounds_max: max_bounds[index],
                footprint_cells: footprint[index],
                occupied_cells: occupied[index],
                occupied_ratio: occupied[index] as f32 / total as f32,
                dominant_grammar: district_grammar(district).to_owned(),
                generated_features: district_feature_names(district, &self.pattern_counts),
            }
        })
        .collect()
    }

    fn stratum_records(&self) -> Vec<StratumRecord> {
        let mut y_min = [usize::MAX; BiomeStratum::COUNT];
        let mut y_max = [0usize; BiomeStratum::COUNT];
        let mut cell_count = [0usize; BiomeStratum::COUNT];
        let mut occupied = [0usize; BiomeStratum::COUNT];

        for y in 0..self.layers {
            let stratum = self.stratum_at(y);
            let index = stratum as usize;
            y_min[index] = y_min[index].min(y);
            y_max[index] = y_max[index].max(y);
            cell_count[index] += self.size * self.size;
            for x in 0..self.size {
                for z in 0..self.size {
                    if self.get(x, z, y) != CellType::Empty {
                        occupied[index] += 1;
                    }
                }
            }
        }

        [
            BiomeStratum::Underground,
            BiomeStratum::Surface,
            BiomeStratum::Midrise,
            BiomeStratum::Skyline,
        ]
        .into_iter()
        .filter(|stratum| cell_count[*stratum as usize] > 0)
        .enumerate()
        .map(|(id, stratum)| {
            let index = stratum as usize;
            StratumRecord {
                id,
                name: stratum.name().to_owned(),
                y_min: y_min[index],
                y_max: y_max[index],
                cell_count: cell_count[index],
                occupied_cells: occupied[index],
                occupied_ratio: occupied[index] as f32 / cell_count[index].max(1) as f32,
                dominant_grammar: stratum_grammar(stratum).to_owned(),
                generated_features: stratum_feature_names(stratum, &self.pattern_counts),
            }
        })
        .collect()
    }

    pub fn saved_structure(&self) -> SavedStructure {
        let mut grid = vec![vec![vec![0u8; self.layers]; self.size]; self.size];
        let mut cell_counts = [0usize; CellType::COUNT];
        let mut material_counts = [0usize; MaterialType::COUNT];
        let mut occupied = 0usize;
        for (x, x_cells) in grid.iter_mut().enumerate().take(self.size) {
            for (z, z_cells) in x_cells.iter_mut().enumerate().take(self.size) {
                for (y, cell) in z_cells.iter_mut().enumerate().take(self.layers) {
                    let cell_type = self.get(x, z, y);
                    *cell = cell_type as u8;
                    cell_counts[cell_type as usize] += 1;
                    if cell_type != CellType::Empty {
                        occupied += 1;
                        material_counts[self.visual_material_at(x, z, y, cell_type) as usize] += 1;
                    }
                }
            }
        }
        let mut district_counts = [0usize; DistrictType::COUNT];
        for district in &self.district_map {
            district_counts[*district as usize] += 1;
        }
        let mut stratum_counts = [0usize; BiomeStratum::COUNT];
        for y in 0..self.layers {
            stratum_counts[self.stratum_at(y) as usize] += self.size * self.size;
        }
        let total_cells = (self.size * self.size * self.layers).max(1);
        let districts = self.district_records();
        let strata = self.stratum_records();
        let metadata = StructureMetadata {
            schema_version: STRUCTURE_SCHEMA_VERSION.to_owned(),
            profile: self.config.profile.to_string(),
            config: self.config.clone(),
            district_counts: district_counts_map(district_counts),
            stratum_counts: stratum_counts_map(stratum_counts),
            cell_counts: cell_counts_map(cell_counts),
            material_counts: material_counts_map(material_counts),
            connection_counts: connection_counts_map(&self.connections),
            room_counts: room_counts_map(&self.rooms),
            pattern_counts: self.pattern_counts.clone(),
            room_count: self.rooms.len(),
            connection_count: self.connections.len(),
            transit_node_count: self.transit_nodes.len(),
            transit_edge_count: self.transit_edges.len(),
            transit_attachment_count: self.transit_attachments.len(),
            district_record_count: districts.len(),
            stratum_record_count: strata.len(),
            occupied_cell_ratio: occupied as f32 / total_cells as f32,
        };
        SavedStructure {
            seed: self.seed.clone(),
            size: self.size,
            layers: self.layers,
            metadata,
            grid,
            connections: self.connections.clone(),
            rooms: self.rooms.clone(),
            transit_graph: self.transit_graph(),
            districts,
            strata,
        }
    }

    pub fn serialize(&self) -> serde_json::Result<String> {
        structure::to_json(&self.saved_structure())
    }

    pub fn save_outputs(&self) -> StructureResult<()> {
        structure::save_outputs(".", &self.seed, &self.saved_structure())
    }
}

pub fn generate_saved_structure(
    seed: String,
    config: GenerationConfig,
) -> StructureResult<SavedStructure> {
    config
        .validate()
        .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { error.into() })?;
    let mut generator = MegaStructureGenerator::with_config(seed, config);
    generator.generate();
    Ok(generator.saved_structure())
}

fn cell_counts_map(counts: [usize; CellType::COUNT]) -> BTreeMap<String, usize> {
    [
        CellType::Empty,
        CellType::Vertical,
        CellType::Horizontal,
        CellType::Bridge,
        CellType::Facade,
        CellType::Stair,
        CellType::Pipe,
        CellType::Antenna,
        CellType::Cable,
        CellType::Vent,
        CellType::Elevator,
        CellType::Debris,
    ]
    .into_iter()
    .map(|cell| (cell.name().to_owned(), counts[cell as usize]))
    .collect()
}

fn district_counts_map(counts: [usize; DistrictType::COUNT]) -> BTreeMap<String, usize> {
    [
        DistrictType::Industrial,
        DistrictType::Residential,
        DistrictType::Commercial,
        DistrictType::Slum,
        DistrictType::Elite,
    ]
    .into_iter()
    .map(|district| (district.name().to_owned(), counts[district as usize]))
    .collect()
}

fn material_counts_map(counts: [usize; MaterialType::COUNT]) -> BTreeMap<String, usize> {
    [
        MaterialType::Concrete,
        MaterialType::Glass,
        MaterialType::Metal,
        MaterialType::Neon,
        MaterialType::Rust,
        MaterialType::Steel,
    ]
    .into_iter()
    .map(|material| (material.name().to_owned(), counts[material as usize]))
    .collect()
}

fn stratum_counts_map(counts: [usize; BiomeStratum::COUNT]) -> BTreeMap<String, usize> {
    [
        BiomeStratum::Underground,
        BiomeStratum::Surface,
        BiomeStratum::Midrise,
        BiomeStratum::Skyline,
    ]
    .into_iter()
    .map(|stratum| (stratum.name().to_owned(), counts[stratum as usize]))
    .collect()
}

fn connection_counts_map(connections: &[ConnectionRecord]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for connection in connections {
        *counts.entry(connection.kind.clone()).or_insert(0) += 1;
    }
    counts
}

fn room_counts_map(rooms: &[RoomRecord]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for room in rooms {
        *counts.entry(room.label.clone()).or_insert(0) += 1;
    }
    counts
}

fn district_grammar(district: DistrictType) -> &'static str {
    match district {
        DistrictType::Industrial => {
            "service-trunk infrastructure, pipe trunks, vents, machine rooms"
        }
        DistrictType::Residential => "habitation modules, shared corridors, shrine pockets",
        DistrictType::Commercial => "market arteries, neon facades, public concourses",
        DistrictType::Slum => "stacked patchwork corridors, cables, improvised bridges",
        DistrictType::Elite => "clean void courts, skyline security, glass facades",
    }
}

fn stratum_grammar(stratum: BiomeStratum) -> &'static str {
    match stratum {
        BiomeStratum::Underground => "maintenance tunnels, pipe junctions, debris basins",
        BiomeStratum::Surface => "market access, service checks, dense public circulation",
        BiomeStratum::Midrise => "habitation, transit annexes, data relay infrastructure",
        BiomeStratum::Skyline => "skybridges, vaults, antennae, exposed windward decay",
    }
}

fn district_feature_names(
    district: DistrictType,
    pattern_counts: &BTreeMap<String, usize>,
) -> Vec<String> {
    let candidates: &[&str] = match district {
        DistrictType::Industrial => &[
            "industrial_service_trunk",
            "PIPE_JUNCTION",
            "SERVICE_DEPOT",
            "MACHINE_ROOM",
            "MAINTENANCE_SHAFT",
        ],
        DistrictType::Residential => &[
            "HABITATION_MODULE",
            "SHRINE",
            "CORRIDOR_DEAD_END",
            "TRANSIT_SERVICE_NODE",
            "debris_field",
        ],
        DistrictType::Commercial => &[
            "MARKET_HALL",
            "MARKET_CONCOURSE",
            "MARKET_STALL",
            "DATA_RELAY",
            "broken_facade",
        ],
        DistrictType::Slum => &[
            "slum_patchwalk",
            "PATCHWORK_JUNCTION",
            "PATCH_BAZAAR",
            "HABITATION_CLUSTER",
            "cable",
        ],
        DistrictType::Elite => &[
            "SKYLINE_VOID_COURT",
            "SKY_SECURITY_GATE",
            "DATA_VAULT",
            "SKY_VAULT",
            "skybridge",
        ],
    };
    present_features(candidates, pattern_counts)
}

fn stratum_feature_names(
    stratum: BiomeStratum,
    pattern_counts: &BTreeMap<String, usize>,
) -> Vec<String> {
    let candidates: &[&str] = match stratum {
        BiomeStratum::Underground => &[
            "service_tunnel",
            "PIPE_JUNCTION",
            "MAINTENANCE_CHECKPOINT",
            "MAINTENANCE_SHAFT",
            "debris_field",
        ],
        BiomeStratum::Surface => &[
            "artery",
            "MARKET_STALL",
            "PATCH_BAZAAR",
            "SHRINE",
            "collapse_scar",
        ],
        BiomeStratum::Midrise => &[
            "artery",
            "DATA_RELAY",
            "TRANSIT_ANNEX",
            "HABITATION_MODULE",
            "broken_facade",
        ],
        BiomeStratum::Skyline => &[
            "skybridge",
            "express_spine",
            "SKY_SECURITY_GATE",
            "DATA_VAULT",
            "hanging_bridge_remnant",
        ],
    };
    present_features(candidates, pattern_counts)
}

fn present_features(candidates: &[&str], pattern_counts: &BTreeMap<String, usize>) -> Vec<String> {
    candidates
        .iter()
        .filter(|candidate| pattern_counts.contains_key(**candidate))
        .map(|candidate| (*candidate).to_owned())
        .collect()
}

fn route_layer_for_hubs(start: (usize, usize), end: (usize, usize), layers: usize) -> usize {
    let distance = start.0.abs_diff(end.0) + start.1.abs_diff(end.1);
    if distance > 24 {
        (layers * 2 / 3).max(1)
    } else if distance > 12 {
        (layers / 3).max(1)
    } else {
        1.min(layers.saturating_sub(1))
    }
}

fn route_kind_for_y(y: usize, layers: usize) -> &'static str {
    if y <= 2 {
        "service_tunnel"
    } else if y >= layers * 2 / 3 {
        "skybridge"
    } else {
        "artery"
    }
}

fn route_room_label(kind: &str, district: DistrictType, stratum: BiomeStratum) -> &'static str {
    match (kind, district, stratum) {
        ("service_tunnel", _, _) => "MAINTENANCE_SHAFT",
        ("skybridge" | "express_spine", DistrictType::Elite, _) => "SKY_LOUNGE",
        ("skybridge" | "express_spine", _, _) => "SKYBRIDGE_NODE",
        ("artery", DistrictType::Commercial, _) => "MARKET_CONCOURSE",
        ("artery", DistrictType::Industrial, _) => "SERVICE_DEPOT",
        ("artery", DistrictType::Slum, _) => "PATCHWORK_JUNCTION",
        (_, _, BiomeStratum::Underground) => "SERVICE_TUNNEL_ROOM",
        _ => "TRANSIT_ANNEX",
    }
}

fn route_aware_room_label(
    kind: &str,
    district: DistrictType,
    stratum: BiomeStratum,
    landmark_frequency: f32,
    noise: f32,
) -> &'static str {
    if noise < 0.08 * landmark_frequency
        && matches!(stratum, BiomeStratum::Midrise | BiomeStratum::Skyline)
    {
        return "DATA_VAULT";
    }
    match (kind, district, stratum) {
        ("service_tunnel" | "vertical_transit_core", DistrictType::Industrial, _) => {
            "PIPE_JUNCTION"
        }
        ("service_tunnel", _, _) => "MAINTENANCE_CHECKPOINT",
        ("artery", DistrictType::Commercial, _) => "MARKET_STALL",
        ("artery", DistrictType::Slum, _) => "PATCH_BAZAAR",
        ("artery", DistrictType::Elite, _) => "SECURITY_GATE",
        ("skybridge" | "express_spine", DistrictType::Elite, _) => "SKY_SECURITY_GATE",
        ("skybridge" | "express_spine", _, BiomeStratum::Skyline) => "DATA_RELAY",
        _ => "TRANSIT_SERVICE_NODE",
    }
}

fn kind_name(base: &str, suffix: &str) -> String {
    format!("{base}_{suffix}")
}

fn room_label(district: DistrictType, stratum: BiomeStratum) -> &'static str {
    match (district, stratum) {
        (_, BiomeStratum::Underground) => "SERVICE_TUNNEL_ROOM",
        (DistrictType::Industrial, _) => "MACHINE_ROOM",
        (DistrictType::Commercial, BiomeStratum::Surface | BiomeStratum::Midrise) => "MARKET_HALL",
        (DistrictType::Slum, _) => "HABITATION_CLUSTER",
        (DistrictType::Elite, BiomeStratum::Skyline) => "SKY_VAULT",
        (DistrictType::Elite, _) => "ATRIUM_SUITE",
        (DistrictType::Residential, _) => "HABITATION_MODULE",
        _ => "ROOM_CENTER",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::validate_seed;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn generated(seed: &str) -> MegaStructureGenerator {
        let mut generator = MegaStructureGenerator::new(seed.to_owned());
        generator.generate();
        generator
    }

    fn generated_with(seed: &str, config: GenerationConfig) -> MegaStructureGenerator {
        let mut generator = MegaStructureGenerator::with_config(seed.to_owned(), config);
        generator.generate();
        generator
    }

    fn temp_test_dir(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("gibson_{name}_{}_{}", std::process::id(), nonce))
    }

    #[test]
    fn generation_is_deterministic_for_same_seed() {
        let a = generated("ABCD1234");
        let b = generated("ABCD1234");
        assert_eq!(a.serialize().unwrap(), b.serialize().unwrap());
    }

    #[test]
    fn serialize_deserialize_round_trips_saved_structure() {
        let generator = generated("ABCD1234");
        let saved = generator.saved_structure();
        let json = structure::to_json(&saved).unwrap();
        assert_eq!(saved, structure::from_json(&json).unwrap());
    }

    #[test]
    fn saved_structure_has_valid_dimensions_and_cell_ranges() {
        let saved = generated("ABCD1234").saved_structure();
        let default_config = GenerationConfig::default();
        assert_eq!(saved.size, default_config.grid_size);
        assert_eq!(saved.layers, default_config.grid_layers);
        assert_eq!(saved.grid.len(), default_config.grid_size);
        for x in &saved.grid {
            assert_eq!(x.len(), default_config.grid_size);
            for z in x {
                assert_eq!(z.len(), default_config.grid_layers);
                for cell in z {
                    assert!(*cell <= CellType::Debris as u8);
                }
            }
        }
    }

    #[test]
    fn built_in_profiles_generate_valid_structures() {
        for profile in crate::config::GenerationProfile::all() {
            let config = GenerationConfig::profile(profile);
            let saved = generated_with("ABCD1234", config.clone()).saved_structure();
            assert_eq!(saved.size, config.grid_size);
            assert_eq!(saved.layers, config.grid_layers);
            assert_eq!(saved.metadata.profile, profile.as_str());
            assert_eq!(saved.grid.len(), config.grid_size);
            assert!(saved
                .grid
                .iter()
                .flatten()
                .flatten()
                .all(|cell| *cell <= CellType::Debris as u8));
        }
    }

    #[test]
    fn same_seed_and_config_are_deterministic_but_profiles_change_output() {
        let dense = GenerationConfig::profile(crate::config::GenerationProfile::Dense);
        let first = generated_with("ABCD1234", dense.clone())
            .serialize()
            .unwrap();
        let second = generated_with("ABCD1234", dense).serialize().unwrap();
        assert_eq!(first, second);

        let balanced = generated_with("ABCD1234", GenerationConfig::default())
            .serialize()
            .unwrap();
        assert_ne!(first, balanced);
    }

    #[test]
    fn headless_generation_metadata_matches_exported_structure() {
        let saved =
            generate_saved_structure("ABCD1234".to_owned(), GenerationConfig::default()).unwrap();
        let occupied = saved
            .grid
            .iter()
            .flatten()
            .flatten()
            .filter(|cell| **cell != CellType::Empty as u8)
            .count();
        let total_cells = saved.size * saved.size * saved.layers;
        assert_eq!(saved.metadata.room_count, saved.rooms.len());
        assert_eq!(saved.metadata.connection_count, saved.connections.len());
        assert_eq!(
            saved.metadata.cell_counts.values().sum::<usize>(),
            total_cells
        );
        assert!(
            (saved.metadata.occupied_cell_ratio - occupied as f32 / total_cells as f32).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn semantic_patterns_emit_routes_and_typed_rooms() {
        let saved = generated("ABCD1234").saved_structure();
        assert!(saved
            .metadata
            .connection_counts
            .contains_key("vertical_transit_core"));
        assert!(saved.metadata.connection_counts.keys().any(|kind| matches!(
            kind.as_str(),
            "artery" | "service_tunnel" | "skybridge" | "express_spine"
        )));
        assert!(saved.metadata.room_counts.keys().any(|label| {
            matches!(
                label.as_str(),
                "MACHINE_ROOM"
                    | "MARKET_HALL"
                    | "HABITATION_CLUSTER"
                    | "HABITATION_MODULE"
                    | "SERVICE_TUNNEL_ROOM"
                    | "ATRIUM_SUITE"
                    | "SKY_VAULT"
            )
        }));
        assert!(saved
            .metadata
            .pattern_counts
            .contains_key("vertical_transit_core"));
        assert!(saved.metadata.pattern_counts.keys().any(|pattern| {
            matches!(
                pattern.as_str(),
                "artery"
                    | "service_tunnel"
                    | "skybridge"
                    | "express_spine"
                    | "vertical_transit_core"
                    | "slum_patchwalk"
                    | "industrial_service_trunk"
                    | "bridge"
                    | "pipe"
                    | "cable"
                    | "collapse_scar"
                    | "debris_field"
                    | "hanging_bridge_remnant"
                    | "broken_facade"
                    | "landmark_shell"
                    | "DATA_VAULT"
                    | "SHRINE"
                    | "MAINTENANCE_SHAFT"
                    | "ROUTE_CHOKEPOINT"
                    | "MAINTENANCE_DEAD_END"
                    | "CORRIDOR_DEAD_END"
                    | "SKYBRIDGE_TERMINAL"
                    | "MARKET_CONCOURSE"
                    | "PATCHWORK_JUNCTION"
                    | "SERVICE_DEPOT"
                    | "TRANSIT_ANNEX"
            )
        }));
    }

    #[test]
    fn exported_transit_graph_has_valid_topology() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(
            saved.metadata.transit_node_count,
            saved.transit_graph.nodes.len()
        );
        assert_eq!(
            saved.metadata.transit_edge_count,
            saved.transit_graph.edges.len()
        );
        assert_eq!(
            saved.metadata.transit_attachment_count,
            saved.transit_graph.attachments.len()
        );
        assert!(!saved.transit_graph.nodes.is_empty());
        assert!(!saved.transit_graph.edges.is_empty());
        assert!(!saved.transit_graph.attachments.is_empty());

        for (expected_id, node) in saved.transit_graph.nodes.iter().enumerate() {
            assert_eq!(node.id, expected_id);
            assert!(node.position[0] < saved.size);
            assert!(node.position[1] < saved.layers);
            assert!(node.position[2] < saved.size);
        }

        for (expected_id, edge) in saved.transit_graph.edges.iter().enumerate() {
            assert_eq!(edge.id, expected_id);
            assert!(edge.start_node < saved.transit_graph.nodes.len());
            assert!(edge.end_node < saved.transit_graph.nodes.len());
            assert_eq!(edge.length, edge.points.len());
            assert!(!edge.points.is_empty());
            for point in &edge.points {
                assert!(point[0] < saved.size);
                assert!(point[1] < saved.layers);
                assert!(point[2] < saved.size);
            }
        }

        for attachment in &saved.transit_graph.attachments {
            assert!(attachment.route_id < saved.transit_graph.edges.len());
            assert!(attachment.room_id < saved.rooms.len());
            assert_eq!(saved.rooms[attachment.room_id].id, attachment.room_id);
            assert!(attachment.position[0] < saved.size);
            assert!(attachment.position[1] < saved.layers);
            assert!(attachment.position[2] < saved.size);
        }
    }

    #[test]
    fn route_aware_generation_attaches_features_to_routes() {
        let saved = generated("ABCD1234").saved_structure();
        assert!(saved
            .transit_graph
            .attachments
            .iter()
            .any(|attachment| attachment.attachment_kind == "route_aware_feature"));
        assert!(saved.rooms.iter().any(|room| {
            matches!(
                room.label.as_str(),
                "PIPE_JUNCTION"
                    | "MAINTENANCE_CHECKPOINT"
                    | "MARKET_STALL"
                    | "PATCH_BAZAAR"
                    | "SECURITY_GATE"
                    | "SKY_SECURITY_GATE"
                    | "DATA_RELAY"
                    | "TRANSIT_SERVICE_NODE"
                    | "DATA_VAULT"
            )
        }));
    }

    #[test]
    fn district_and_stratum_records_describe_generated_space() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(saved.metadata.district_record_count, saved.districts.len());
        assert_eq!(saved.metadata.stratum_record_count, saved.strata.len());
        assert_eq!(
            saved
                .districts
                .iter()
                .map(|district| district.footprint_cells)
                .sum::<usize>(),
            saved.size * saved.size
        );
        assert_eq!(
            saved
                .strata
                .iter()
                .map(|stratum| stratum.cell_count)
                .sum::<usize>(),
            saved.size * saved.size * saved.layers
        );
        for district in &saved.districts {
            assert!(district.bounds_min[0] <= district.bounds_max[0]);
            assert!(district.bounds_min[1] <= district.bounds_max[1]);
            assert!(district.bounds_max[0] < saved.size);
            assert!(district.bounds_max[1] < saved.size);
            assert!(!district.dominant_grammar.is_empty());
            assert!((0.0..=1.0).contains(&district.occupied_ratio));
        }
        for stratum in &saved.strata {
            assert!(stratum.y_min <= stratum.y_max);
            assert!(stratum.y_max < saved.layers);
            assert!(!stratum.dominant_grammar.is_empty());
            assert!((0.0..=1.0).contains(&stratum.occupied_ratio));
        }
        assert!(saved
            .districts
            .iter()
            .chain(saved.districts.iter())
            .any(|district| !district.generated_features.is_empty()));
        assert!(saved
            .strata
            .iter()
            .any(|stratum| !stratum.generated_features.is_empty()));
    }

    #[test]
    fn exported_json_schema_keeps_semantic_sections() {
        let saved = generated("ABCD1234").saved_structure();
        let value: serde_json::Value =
            serde_json::from_str(&structure::to_json(&saved).unwrap()).unwrap();
        assert_eq!(
            value["metadata"]["schema_version"],
            serde_json::json!(STRUCTURE_SCHEMA_VERSION)
        );
        for key in [
            "seed",
            "size",
            "layers",
            "metadata",
            "grid",
            "connections",
            "rooms",
            "transit_graph",
            "districts",
            "strata",
        ] {
            assert!(value.get(key).is_some(), "missing top-level key {key}");
        }
        for key in [
            "route_density",
            "landmark_frequency",
            "decay_story_density",
            "district_contrast",
            "strata_separation",
        ] {
            assert!(
                value["metadata"]["config"].get(key).is_some(),
                "missing config key {key}"
            );
        }
        for key in [
            "transit_node_count",
            "transit_edge_count",
            "transit_attachment_count",
            "district_record_count",
            "stratum_record_count",
        ] {
            assert!(
                value["metadata"].get(key).is_some(),
                "missing metadata key {key}"
            );
        }
    }

    #[test]
    fn decay_profiles_emit_debris_and_remnant_patterns() {
        let saved = generated_with(
            "ABCD1234",
            GenerationConfig::profile(crate::config::GenerationProfile::Decayed),
        )
        .saved_structure();
        assert!(
            saved
                .metadata
                .cell_counts
                .get("DEBRIS")
                .copied()
                .unwrap_or_default()
                > 0
        );
        assert!(saved.metadata.pattern_counts.keys().any(|pattern| {
            matches!(
                pattern.as_str(),
                "debris_field"
                    | "hanging_bridge_remnant"
                    | "broken_facade"
                    | "ROUTE_CHOKEPOINT"
                    | "MAINTENANCE_DEAD_END"
                    | "CORRIDOR_DEAD_END"
                    | "SKYBRIDGE_TERMINAL"
            )
        }));
    }

    #[test]
    fn nearest_room_semantics_are_queryable_for_exported_rooms() {
        let generator = generated("ABCD1234");
        let saved = generator.saved_structure();
        let room = saved.rooms.first().expect("generated room");
        assert_eq!(
            generator.nearest_room_label(room.position[0], room.position[1], room.position[2]),
            Some(room.label.as_str())
        );
    }

    #[test]
    fn strata_metadata_covers_every_cell() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(
            saved.metadata.stratum_counts.values().sum::<usize>(),
            saved.size * saved.size * saved.layers
        );
        for expected in ["UNDERGROUND", "SURFACE", "MIDRISE", "SKYLINE"] {
            assert!(saved.metadata.stratum_counts.contains_key(expected));
        }
    }

    #[test]
    fn persistence_loads_saved_structures_and_reports_write_errors() {
        let saved = generated("ABCD1234").saved_structure();
        let dir = temp_test_dir("persistence");
        fs::create_dir_all(&dir).unwrap();

        let structure_path = dir.join(structure::STRUCTURE_FILE);
        structure::save_structure(&structure_path, &saved).unwrap();
        assert_eq!(saved, structure::load_structure(&structure_path).unwrap());

        let directory_as_file = dir.join("not_a_file");
        fs::create_dir_all(&directory_as_file).unwrap();
        assert!(structure::save_structure(&directory_as_file, &saved).is_err());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn validates_seed_shape() {
        assert!(validate_seed("ABCD1234"));
        assert!(!validate_seed("abc123"));
        assert!(!validate_seed("ABCD-123"));
    }
}
