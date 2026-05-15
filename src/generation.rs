use macroquad::prelude::*;

use crate::seed::{seed_hash, Rng32};
use crate::structure::{self, ConnectionRecord, RoomRecord, SavedStructure, StructureResult};

pub(crate) const GRID_SIZE: usize = 30;
pub(crate) const GRID_LAYERS: usize = 15;
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
}

impl CellType {
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
        CellType::Pipe => MaterialType::Rust,
    }
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
    cells: Vec<WfcCell>,
    weights: [f32; WFC_TILE_COUNT],
    adjacency: [[u16; 4]; WFC_TILE_COUNT],
    rng: Rng32,
    backtrack_depth: usize,
}

impl WfcSolver {
    fn new(seed: u64, district: DistrictType, stratum: BiomeStratum) -> Self {
        let (adjacency, mut weights) = wfc_init_tables();
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
            GRID_SIZE * GRID_SIZE
        ];
        for cell in &mut cells {
            cell.entropy = wfc_calc_entropy(cell.possible, &weights);
        }
        Self {
            cells,
            weights,
            adjacency,
            rng: Rng32::new(seed),
            backtrack_depth: 0,
        }
    }

    fn idx(x: usize, z: usize) -> usize {
        x * GRID_SIZE + z
    }

    fn constrain(&mut self, x: usize, z: usize, tile: WFCTile) {
        let cell = &mut self.cells[Self::idx(x, z)];
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
            let current_tile = match self.cells[Self::idx(x, z)].collapsed_tile {
                Some(tile) => tile,
                None => {
                    iterations += 1;
                    continue;
                }
            };
            for (direction, (dx, dz)) in directions.iter().enumerate() {
                let nx = x as isize + dx;
                let nz = z as isize + dz;
                if nx < 0 || nz < 0 || nx >= GRID_SIZE as isize || nz >= GRID_SIZE as isize {
                    continue;
                }
                let nx = nx as usize;
                let nz = nz as usize;
                let index = Self::idx(nx, nz);
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
        for x in 0..GRID_SIZE {
            for z in 0..GRID_SIZE {
                let cell = self.cells[Self::idx(x, z)];
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
        let possible = self.cells[Self::idx(bx, bz)].possible;
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
        let index = Self::idx(bx, bz);
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
    pub(crate) seed_hash: u64,
    rng: Rng32,
    grid: Vec<CellType>,
    support_map: Vec<bool>,
    district_map: Vec<DistrictType>,
    connections: Vec<ConnectionRecord>,
    rooms: Vec<RoomRecord>,
}

impl MegaStructureGenerator {
    pub fn new(seed: String) -> Self {
        let hash = seed_hash(&seed);
        let mut generator = Self {
            size: GRID_SIZE,
            layers: GRID_LAYERS,
            seed,
            seed_hash: hash,
            rng: Rng32::new(hash),
            grid: vec![CellType::Empty; GRID_SIZE * GRID_SIZE * GRID_LAYERS],
            support_map: vec![false; GRID_SIZE * GRID_SIZE * GRID_LAYERS],
            district_map: vec![DistrictType::Residential; GRID_SIZE * GRID_SIZE],
            connections: Vec::new(),
            rooms: Vec::new(),
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

    fn support_at(&self, x: usize, z: usize, y: usize) -> bool {
        self.support_map[self.idx(x, z, y)]
    }

    pub(crate) fn district_at(&self, x: usize, z: usize) -> DistrictType {
        self.district_map[self.district_idx(x, z)]
    }

    fn generate_district_map(&mut self) {
        for x in 0..self.size {
            for z in 0..self.size {
                let noise = simplex::noise3(x as f32 * 0.05, z as f32 * 0.05, 0.0)
                    + simplex::noise3(x as f32 * 0.10, z as f32 * 0.10, 1.0) * 0.5
                    + simplex::noise3(x as f32 * 0.20, z as f32 * 0.20, 2.0) * 0.25;
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
        self.apply_floor_thickness();
        self.phase3_infrastructure();
        self.phase4_erosion();
        self.ensure_structural_integrity();
        self.add_support_pillars();
        self.carve_traversal_space();
    }

    pub fn seed(&self) -> &str {
        &self.seed
    }

    fn phase1_skeleton(&mut self) {
        for x in 0..self.size {
            for z in 0..self.size {
                let district = self.district_at(x, z);
                let props = DISTRICTS[district as usize];
                let base_probability = 0.15 * props.core_density;
                let noise_mod = simplex::noise3(x as f32 * 0.1, z as f32 * 0.1, 3.0) * 0.1;
                if self.rng.next_f32() >= (base_probability + noise_mod).max(0.02) {
                    continue;
                }

                let height_range = (self.layers as f32 * props.vertical_variation) as usize;
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
            let stratum = biome_for_y(y);
            let mut solver = WfcSolver::new(self.seed_hash ^ (y as u64 * 12345), district, stratum);
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
                    let tile = solver.cells[WfcSolver::idx(x, z)]
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
                            self.rooms.push(RoomRecord {
                                position: [x, y, z],
                                district: district.name().to_owned(),
                                label: "ROOM_CENTER".to_owned(),
                            });
                        }
                    }
                }
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
        for _ in 0..((self.size * self.layers) as f32 * 0.02) as usize {
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
            self.connections.push(ConnectionRecord {
                kind: "bridge".to_owned(),
                start: [start.0, y, start.1],
                end: [end.0, y, end.1],
            });
        }
    }

    fn add_spline_cables(&mut self) {
        for _ in 0..((self.size as f32) * 0.5) as usize {
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
            self.connections.push(ConnectionRecord {
                kind: "cable".to_owned(),
                start: [start.0, start.2, start.1],
                end: [end.0, end.2, end.1],
            });
        }
    }

    fn add_spline_pipes(&mut self) {
        for _ in 0..((self.size * self.layers) as f32 * 0.03) as usize {
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
                self.connections.push(ConnectionRecord {
                    kind: "pipe".to_owned(),
                    start: [x, y, z],
                    end: [cx as usize, y, cz as usize],
                });
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
                    let threshold = if biome_for_y(y) == BiomeStratum::Skyline {
                        0.3
                    } else {
                        0.6
                    };
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

    pub fn saved_structure(&self) -> SavedStructure {
        let mut grid = vec![vec![vec![0u8; self.layers]; self.size]; self.size];
        for (x, x_cells) in grid.iter_mut().enumerate().take(self.size) {
            for (z, z_cells) in x_cells.iter_mut().enumerate().take(self.size) {
                for (y, cell) in z_cells.iter_mut().enumerate().take(self.layers) {
                    *cell = self.get(x, z, y) as u8;
                }
            }
        }
        SavedStructure::new(
            self.seed.clone(),
            self.size,
            self.layers,
            grid,
            self.connections.clone(),
            self.rooms.clone(),
        )
    }

    pub fn serialize(&self) -> serde_json::Result<String> {
        structure::to_json(&self.saved_structure())
    }

    pub fn save_outputs(&self) -> StructureResult<()> {
        structure::save_outputs(".", &self.seed, &self.saved_structure())
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
        assert_eq!(saved.size, GRID_SIZE);
        assert_eq!(saved.layers, GRID_LAYERS);
        assert_eq!(saved.grid.len(), GRID_SIZE);
        for x in &saved.grid {
            assert_eq!(x.len(), GRID_SIZE);
            for z in x {
                assert_eq!(z.len(), GRID_LAYERS);
                for cell in z {
                    assert!(*cell <= CellType::Elevator as u8);
                }
            }
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
