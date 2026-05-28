use macroquad::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

use crate::config::{GenerationConfig, MegastructureTypology};
use crate::rules::CompiledRulePackSet;
use crate::seed::{seed_hash, Rng32};
use crate::structure::{
    self, ConnectionRecord, ConstructionEraRecord, ContestedBorderRecord, DistrictBorderRecord,
    DistrictLifecycleRecord, DistrictRecord, EntityPathRecord, EntityPressureFieldRecord,
    EntityRecord, FactionRecord, FailurePropagationRecord, HazardZoneRecord,
    InfrastructureFlowRecord, LayoutMutationRecord, LoadPathRecord, MacroMassingRecord,
    MesoPlacementRecord, MicroDetailRecord, MissionPathRecord, NarrativeLandmarkRecord,
    PathAnalysisRecord, ResourceNetworkRecord, RoomClusterRecord, RoomRecord,
    RouteSimulationRecord, RuleInfluenceRecord, RulePackRecord, SavedStructure,
    SectionQualityRecord, StabilityRatingRecord, StratumRecord, StressFieldRecord,
    StructuralSystemRecord, StructureMetadata, StructureResult, TemporalPhaseRecord,
    TemporalStateRecord, TerritoryRecord, TransitAttachmentRecord, TransitEdgeRecord,
    TransitGraphRecord, TransitNodeRecord, TypologyBandRecord, TypologyFrameRecord,
    TypologyQualityRecord, STRUCTURE_SCHEMA_VERSION,
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
    rule_packs: CompiledRulePackSet,
    pub(crate) seed_hash: u64,
    rng: Rng32,
    grid: Vec<CellType>,
    support_map: Vec<bool>,
    district_map: Vec<DistrictType>,
    typology_frame: TypologyFrameRecord,
    connections: Vec<ConnectionRecord>,
    rooms: Vec<RoomRecord>,
    transit_nodes: Vec<TransitNodeRecord>,
    transit_edges: Vec<TransitEdgeRecord>,
    transit_attachments: Vec<TransitAttachmentRecord>,
    district_borders: Vec<DistrictBorderRecord>,
    infrastructure_flows: Vec<InfrastructureFlowRecord>,
    hazard_zones: Vec<HazardZoneRecord>,
    entities: Vec<EntityRecord>,
    entity_paths: Vec<EntityPathRecord>,
    entity_pressure_fields: Vec<EntityPressureFieldRecord>,
    layout_mutations: Vec<LayoutMutationRecord>,
    pattern_counts: BTreeMap<String, usize>,
}

impl MegaStructureGenerator {
    pub fn new(seed: String) -> Self {
        Self::with_config(seed, GenerationConfig::default())
    }

    pub fn with_config(seed: String, config: GenerationConfig) -> Self {
        Self::with_config_and_rules(seed, config, CompiledRulePackSet::default())
    }

    pub fn with_config_and_rules(
        seed: String,
        config: GenerationConfig,
        rule_packs: CompiledRulePackSet,
    ) -> Self {
        config.validate().expect("validated generation config");
        let hash = seed_hash(&seed);
        let size = config.grid_size;
        let layers = config.grid_layers;
        let typology_frame = build_typology_frame(config.typology, size, layers);
        let mut generator = Self {
            size,
            layers,
            seed,
            config,
            rule_packs,
            seed_hash: hash,
            rng: Rng32::new(hash),
            grid: vec![CellType::Empty; size * size * layers],
            support_map: vec![false; size * size * layers],
            district_map: vec![DistrictType::Residential; size * size],
            typology_frame,
            connections: Vec::new(),
            rooms: Vec::new(),
            transit_nodes: Vec::new(),
            transit_edges: Vec::new(),
            transit_attachments: Vec::new(),
            district_borders: Vec::new(),
            infrastructure_flows: Vec::new(),
            hazard_zones: Vec::new(),
            entities: Vec::new(),
            entity_paths: Vec::new(),
            entity_pressure_fields: Vec::new(),
            layout_mutations: Vec::new(),
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
            cluster_id: None,
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
        let role =
            route_role_for(kind, points.first().copied(), points.last().copied(), self).to_owned();
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
            role,
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
                let district = if let Some(district) = self.typology_district_at(x, z) {
                    district
                } else if noise < -0.3 {
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

    fn typology_district_at(&self, x: usize, z: usize) -> Option<DistrictType> {
        let center = self.size / 2;
        let corridor = (self.size / 8).max(2);
        let dx = x.abs_diff(center);
        let dz = z.abs_diff(center);
        let radius = ((dx * dx + dz * dz) as f32).sqrt();
        let ring_radius = self.size as f32 * 0.34;
        let angle_band = if x >= center && z < center {
            0
        } else if x >= center {
            1
        } else if z >= center {
            2
        } else {
            3
        };
        match self.config.typology {
            MegastructureTypology::DenseEnclave => None,
            MegastructureTypology::ArcologySpire => {
                if dx <= corridor && dz <= corridor {
                    Some(DistrictType::Elite)
                } else if dx.max(dz) <= corridor * 2 {
                    Some(DistrictType::Commercial)
                } else if z < center {
                    Some(DistrictType::Residential)
                } else {
                    Some(DistrictType::Industrial)
                }
            }
            MegastructureTypology::LinearCity => {
                if dz <= corridor / 2 {
                    Some(DistrictType::Commercial)
                } else if dz <= corridor {
                    Some(DistrictType::Residential)
                } else if x % 5 == 0 {
                    Some(DistrictType::Industrial)
                } else {
                    Some(DistrictType::Slum)
                }
            }
            MegastructureTypology::BridgeVoid => {
                if dx <= corridor && dz <= corridor {
                    Some(DistrictType::Slum)
                } else if (x < center && z < center) || (x >= center && z >= center) {
                    Some(DistrictType::Residential)
                } else if x >= center && z < center {
                    Some(DistrictType::Elite)
                } else {
                    Some(DistrictType::Industrial)
                }
            }
            MegastructureTypology::MarinePlatform => {
                if x < corridor
                    || z < corridor
                    || x + corridor >= self.size
                    || z + corridor >= self.size
                {
                    Some(DistrictType::Industrial)
                } else if dz <= corridor {
                    Some(DistrictType::Commercial)
                } else if z < center {
                    Some(DistrictType::Residential)
                } else {
                    Some(DistrictType::Slum)
                }
            }
            MegastructureTypology::OrbitalRing => {
                if (radius - ring_radius).abs() <= corridor as f32 {
                    Some(match angle_band {
                        0 => DistrictType::Commercial,
                        1 => DistrictType::Residential,
                        2 => DistrictType::Industrial,
                        _ => DistrictType::Elite,
                    })
                } else if dx <= corridor / 2 || dz <= corridor / 2 {
                    Some(DistrictType::Industrial)
                } else {
                    Some(DistrictType::Slum)
                }
            }
            MegastructureTypology::UndergroundHive => {
                if dx <= corridor || dz <= corridor {
                    Some(DistrictType::Industrial)
                } else if (x + z) % 5 == 0 {
                    Some(DistrictType::Commercial)
                } else {
                    Some(DistrictType::Residential)
                }
            }
            MegastructureTypology::MountainBurrow => {
                if x < center {
                    Some(DistrictType::Industrial)
                } else if dz <= corridor {
                    Some(DistrictType::Commercial)
                } else {
                    Some(DistrictType::Residential)
                }
            }
            MegastructureTypology::DesertArcology => {
                if dx <= corridor && dz <= corridor {
                    Some(DistrictType::Elite)
                } else if dx.max(dz) <= corridor * 3 {
                    Some(DistrictType::Residential)
                } else {
                    Some(DistrictType::Industrial)
                }
            }
            MegastructureTypology::AirportCity => {
                if dz <= corridor / 2 {
                    Some(DistrictType::Commercial)
                } else if z < center {
                    Some(DistrictType::Industrial)
                } else {
                    Some(DistrictType::Residential)
                }
            }
            MegastructureTypology::DamCity => {
                if dx <= corridor {
                    Some(DistrictType::Industrial)
                } else if x < center {
                    Some(DistrictType::Residential)
                } else {
                    Some(DistrictType::Commercial)
                }
            }
            MegastructureTypology::ShipyardStack => {
                if dz <= corridor {
                    Some(DistrictType::Industrial)
                } else if x % 4 == 0 {
                    Some(DistrictType::Commercial)
                } else {
                    Some(DistrictType::Slum)
                }
            }
            MegastructureTypology::VolcanicCaldera => {
                if radius < self.size as f32 * 0.18 {
                    Some(DistrictType::Industrial)
                } else if (radius - ring_radius).abs() <= corridor as f32 {
                    Some(DistrictType::Commercial)
                } else if z < center {
                    Some(DistrictType::Residential)
                } else {
                    Some(DistrictType::Slum)
                }
            }
            MegastructureTypology::IceShelfCity => {
                if x % (corridor + 2) == 0 {
                    Some(DistrictType::Industrial)
                } else if z < center {
                    Some(DistrictType::Residential)
                } else {
                    Some(DistrictType::Commercial)
                }
            }
            MegastructureTypology::CanopyBabel => {
                if dx <= corridor || dz <= corridor {
                    Some(DistrictType::Industrial)
                } else if radius < self.size as f32 * 0.32 {
                    Some(DistrictType::Residential)
                } else {
                    Some(DistrictType::Commercial)
                }
            }
            MegastructureTypology::SpaceElevatorAnchor => {
                if dx <= corridor && dz <= corridor {
                    Some(DistrictType::Elite)
                } else if dx <= corridor || dz <= corridor {
                    Some(DistrictType::Industrial)
                } else if radius < self.size as f32 * 0.34 {
                    Some(DistrictType::Commercial)
                } else {
                    Some(DistrictType::Residential)
                }
            }
            MegastructureTypology::CrawlerCity => {
                if dz <= corridor {
                    Some(DistrictType::Industrial)
                } else if (x / corridor.max(1)).is_multiple_of(2) {
                    Some(DistrictType::Commercial)
                } else {
                    Some(DistrictType::Residential)
                }
            }
            MegastructureTypology::ReefAtollArcology => {
                if (radius - ring_radius).abs() <= corridor as f32 {
                    Some(DistrictType::Residential)
                } else if radius < ring_radius - corridor as f32 {
                    Some(DistrictType::Commercial)
                } else {
                    Some(DistrictType::Industrial)
                }
            }
            MegastructureTypology::StratospherePlatform => {
                if dx <= corridor || dz <= corridor {
                    Some(DistrictType::Industrial)
                } else if radius < self.size as f32 * 0.28 {
                    Some(DistrictType::Elite)
                } else {
                    Some(DistrictType::Residential)
                }
            }
            MegastructureTypology::SinkholeCitadel => {
                if radius < self.size as f32 * 0.18 {
                    Some(DistrictType::Slum)
                } else if (radius - self.size as f32 * 0.28).abs() <= corridor as f32 {
                    Some(DistrictType::Commercial)
                } else {
                    Some(DistrictType::Residential)
                }
            }
        }
    }

    fn typology_occupancy_bias(&self, x: usize, z: usize, y: usize) -> f32 {
        let center = self.size / 2;
        let corridor = (self.size / 8).max(2);
        let dx = x.abs_diff(center);
        let dz = z.abs_diff(center);
        let radius = ((dx * dx + dz * dz) as f32).sqrt();
        let ring_radius = self.size as f32 * 0.34;
        match self.config.typology {
            MegastructureTypology::DenseEnclave => 1.0,
            MegastructureTypology::ArcologySpire => {
                let taper = 1.0 - y as f32 / self.layers.max(1) as f32 * 0.35;
                if dx <= corridor && dz <= corridor {
                    1.85
                } else if dx.max(dz) <= corridor * 3 {
                    1.15 * taper
                } else {
                    0.45 * taper
                }
            }
            MegastructureTypology::LinearCity => {
                if dz <= corridor / 2 {
                    1.75
                } else if dz <= corridor {
                    1.05
                } else {
                    0.25
                }
            }
            MegastructureTypology::BridgeVoid => {
                let tower = (x < center - corridor && z < center - corridor)
                    || (x > center + corridor && z > center + corridor)
                    || (x < center - corridor && z > center + corridor)
                    || (x > center + corridor && z < center - corridor);
                if dx <= corridor && dz <= corridor {
                    0.10
                } else if tower {
                    1.25
                } else if dx <= corridor || dz <= corridor {
                    0.42
                } else {
                    0.75
                }
            }
            MegastructureTypology::MarinePlatform => {
                if y <= self.layers / 5 {
                    1.45
                } else if x % corridor == 0 || z % corridor == 0 {
                    1.20
                } else {
                    0.70
                }
            }
            MegastructureTypology::OrbitalRing => {
                let ring = (radius - ring_radius).abs();
                if ring <= corridor as f32 {
                    1.55
                } else if dx <= 1 || dz <= 1 {
                    1.05
                } else if radius < ring_radius - corridor as f32 {
                    0.08
                } else {
                    0.35
                }
            }
            MegastructureTypology::UndergroundHive => {
                if y < self.layers / 2 {
                    1.45
                } else if dx <= corridor || dz <= corridor {
                    0.8
                } else {
                    0.25
                }
            }
            MegastructureTypology::MountainBurrow => {
                let cliff = x < center + corridor;
                if cliff && y < self.layers * 3 / 4 {
                    1.3
                } else if x > center + corridor {
                    0.35
                } else {
                    0.75
                }
            }
            MegastructureTypology::DesertArcology => {
                if dx.max(dz) <= corridor * 2 {
                    1.55
                } else if y <= self.layers / 4 {
                    1.1
                } else {
                    0.32
                }
            }
            MegastructureTypology::AirportCity => {
                if dz <= corridor {
                    1.35
                } else if z < center && y <= self.layers / 3 {
                    1.15
                } else {
                    0.55
                }
            }
            MegastructureTypology::DamCity => {
                if dx <= corridor {
                    1.6
                } else if x < center {
                    0.65
                } else {
                    1.0
                }
            }
            MegastructureTypology::ShipyardStack => {
                if dz <= corridor || y <= self.layers / 4 {
                    1.35
                } else if dx <= corridor {
                    1.0
                } else {
                    0.58
                }
            }
            MegastructureTypology::VolcanicCaldera => {
                let caldera = self.size as f32 * 0.18;
                if radius < caldera {
                    0.08
                } else if (radius - ring_radius).abs() <= corridor as f32 {
                    1.55
                } else if y <= self.layers / 3 {
                    0.85
                } else {
                    0.36
                }
            }
            MegastructureTypology::IceShelfCity => {
                if y <= self.layers / 5 {
                    1.28
                } else if x % (corridor + 2) == 0 {
                    1.05
                } else {
                    0.48
                }
            }
            MegastructureTypology::CanopyBabel => {
                if y >= self.layers / 2 {
                    1.42
                } else if dx <= corridor || dz <= corridor {
                    1.2
                } else {
                    0.42
                }
            }
            MegastructureTypology::SpaceElevatorAnchor => {
                if dx <= corridor && dz <= corridor {
                    1.75
                } else if dx <= corridor || dz <= corridor {
                    1.12
                } else {
                    0.52
                }
            }
            MegastructureTypology::CrawlerCity => {
                if dz <= corridor {
                    1.48
                } else if y <= self.layers / 3 {
                    1.05
                } else {
                    0.38
                }
            }
            MegastructureTypology::ReefAtollArcology => {
                if (radius - ring_radius).abs() <= corridor as f32 {
                    1.55
                } else if radius < ring_radius - corridor as f32 {
                    0.25
                } else if y <= self.layers / 4 {
                    1.0
                } else {
                    0.45
                }
            }
            MegastructureTypology::StratospherePlatform => {
                if y >= self.layers / 2 {
                    1.35
                } else if dx <= corridor || dz <= corridor {
                    1.0
                } else {
                    0.24
                }
            }
            MegastructureTypology::SinkholeCitadel => {
                if radius < self.size as f32 * 0.16 {
                    0.06
                } else if (radius - self.size as f32 * 0.28).abs() <= corridor as f32 {
                    1.5
                } else if y < self.layers * 2 / 3 {
                    0.85
                } else {
                    0.36
                }
            }
        }
    }

    pub fn generate(&mut self) {
        self.phase1_skeleton();
        self.phase1b_macro_massing();
        self.phase1c_typology_massing();
        self.phase2_floorplans();
        self.phase2b_circulation_graph();
        self.phase2d_route_aware_generation();
        self.apply_floor_thickness();
        self.phase2c_district_patterns();
        self.phase2e_district_adjacency();
        self.phase2f_cellular_automata_patterns();
        self.phase3_infrastructure();
        self.phase3b_infrastructure_flows();
        self.phase3c_micro_details();
        self.phase4_erosion();
        self.phase4b_decay_signatures();
        self.phase4c_hazard_zones();
        self.ensure_structural_integrity();
        self.add_support_pillars();
        self.carve_traversal_space();
        self.phase5_story_details();
        self.phase5b_program_aware_rooms();
        self.phase6_entity_dynamics();
        self.ensure_structural_integrity();
        self.add_support_pillars();
        self.carve_traversal_space();
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
            .map(|(_, edge)| format!("{}:{}#{}", edge.role, edge.kind, edge.id))
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

    pub(crate) fn district_borders(&self) -> &[DistrictBorderRecord] {
        &self.district_borders
    }

    pub(crate) fn computed_room_clusters(&self) -> Vec<RoomClusterRecord> {
        self.room_clusters().0
    }

    pub(crate) fn infrastructure_flows(&self) -> &[InfrastructureFlowRecord] {
        &self.infrastructure_flows
    }

    pub(crate) fn hazard_zones(&self) -> &[HazardZoneRecord] {
        &self.hazard_zones
    }

    fn phase1_skeleton(&mut self) {
        for x in 0..self.size {
            for z in 0..self.size {
                let district = self.district_at(x, z);
                let props = DISTRICTS[district as usize];
                let typology_bias = self.typology_occupancy_bias(x, z, 0);
                if typology_bias <= 0.12 {
                    continue;
                }
                let base_probability =
                    0.15 * props.core_density * self.config.district_density_scale * typology_bias;
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
            self.apply_advanced_wfc_constraints(&mut solver, y);
            solver.solve();
            for x in 0..self.size {
                for z in 0..self.size {
                    let existing = self.get(x, z, y);
                    if existing != CellType::Empty && existing != CellType::Horizontal {
                        continue;
                    }
                    if self.typology_occupancy_bias(x, z, y) < 0.18 {
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

    fn apply_advanced_wfc_constraints(&self, solver: &mut WfcSolver, y: usize) {
        if self.config.advanced_pattern_complexity <= 0.0 {
            return;
        }
        let stratum = self.stratum_at(y);
        let interval = (9.0 / self.config.advanced_pattern_complexity.max(0.5))
            .round()
            .clamp(4.0, 12.0) as usize;
        for x in (interval / 2..self.size).step_by(interval) {
            for z in (interval / 2..self.size).step_by(interval) {
                let district = self.district_at(x, z);
                let tile = match (district, stratum) {
                    (DistrictType::Commercial | DistrictType::Elite, BiomeStratum::Midrise)
                    | (DistrictType::Elite, BiomeStratum::Skyline) => WFCTile::RoomCenter,
                    (
                        DistrictType::Industrial,
                        BiomeStratum::Underground | BiomeStratum::Surface,
                    ) => WFCTile::CorridorNS,
                    (DistrictType::Slum, _) => WFCTile::FloorHalfN,
                    _ => continue,
                };
                if hash_noise(self.seed_hash, x, z, y)
                    < (0.18 * self.config.advanced_pattern_complexity).clamp(0.05, 0.55)
                {
                    solver.constrain(x, z, tile);
                }
            }
        }
        if stratum == BiomeStratum::Skyline {
            let margin = 2;
            for x in margin..self.size.saturating_sub(margin) {
                for z in margin..self.size.saturating_sub(margin) {
                    if (x + z + y) % interval == 0 && hash_noise(self.seed_hash, x, z, y) > 0.72 {
                        solver.constrain(x, z, WFCTile::Empty);
                    }
                }
            }
        }
    }

    fn phase1b_macro_massing(&mut self) {
        let stride = (self.size / 5).max(4);
        for x in (stride / 2..self.size.saturating_sub(1)).step_by(stride) {
            for z in (stride / 2..self.size.saturating_sub(1)).step_by(stride) {
                let district = self.district_at(x, z);
                let lifecycle = self.lifecycle_for_district(district, 0.5);
                let y_min = (self.layers / 4).max(1);
                let y_max = (self.layers * 3 / 4).max(y_min + 1).min(self.layers - 1);
                let roll = hash_noise(self.seed_hash, x, z, y_min);
                if matches!(district, DistrictType::Elite | DistrictType::Commercial) && roll > 0.45
                {
                    for y in y_min..=y_max {
                        for (dx, dz) in [(0isize, 0isize), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                            if let Some((nx, nz)) = self.offset_xz(x, z, dx, dz) {
                                self.set(nx, nz, y, CellType::Empty, false);
                            }
                        }
                    }
                    self.push_connection("macro_void", [x, y_min, z], [x, y_max, z]);
                } else if lifecycle.occupancy_pressure > 0.72 {
                    for y in y_min..=y_max {
                        self.set(x, z, y, CellType::Vertical, true);
                        if x + 1 < self.size {
                            self.set(x + 1, z, y, CellType::Horizontal, true);
                        }
                    }
                    self.push_connection("macro_density_spine", [x, y_min, z], [x, y_max, z]);
                }
            }
        }
    }

    fn phase1c_typology_massing(&mut self) {
        match self.config.typology {
            MegastructureTypology::DenseEnclave => {}
            MegastructureTypology::ArcologySpire => self.add_arcology_spire_massing(),
            MegastructureTypology::LinearCity => self.add_linear_city_massing(),
            MegastructureTypology::BridgeVoid => self.add_bridge_void_massing(),
            MegastructureTypology::MarinePlatform => self.add_marine_platform_massing(),
            MegastructureTypology::OrbitalRing => self.add_orbital_ring_massing(),
            MegastructureTypology::UndergroundHive => self.add_underground_hive_massing(),
            MegastructureTypology::MountainBurrow => self.add_mountain_burrow_massing(),
            MegastructureTypology::DesertArcology => self.add_desert_arcology_massing(),
            MegastructureTypology::AirportCity => self.add_airport_city_massing(),
            MegastructureTypology::DamCity => self.add_dam_city_massing(),
            MegastructureTypology::ShipyardStack => self.add_shipyard_stack_massing(),
            MegastructureTypology::VolcanicCaldera => self.add_volcanic_caldera_massing(),
            MegastructureTypology::IceShelfCity => self.add_ice_shelf_city_massing(),
            MegastructureTypology::CanopyBabel => self.add_canopy_babel_massing(),
            MegastructureTypology::SpaceElevatorAnchor => self.add_space_elevator_anchor_massing(),
            MegastructureTypology::CrawlerCity => self.add_crawler_city_massing(),
            MegastructureTypology::ReefAtollArcology => self.add_reef_atoll_arcology_massing(),
            MegastructureTypology::StratospherePlatform => self.add_stratosphere_platform_massing(),
            MegastructureTypology::SinkholeCitadel => self.add_sinkhole_citadel_massing(),
        }
    }

    fn add_arcology_spire_massing(&mut self) {
        let c = self.size / 2;
        let radius = (self.size / 7).max(2);
        for y in 0..self.layers {
            let band_radius = radius.saturating_sub(y / 10).max(1);
            for x in c.saturating_sub(band_radius)..=(c + band_radius).min(self.size - 1) {
                for z in c.saturating_sub(band_radius)..=(c + band_radius).min(self.size - 1) {
                    self.set(x, z, y, CellType::Vertical, true);
                }
            }
            if y % 5 == 0 {
                for offset in -(radius as isize * 2)..=(radius as isize * 2) {
                    if let Some((x, z)) = self.offset_xz(c, c, offset, 0) {
                        self.set(x, z, y, CellType::Bridge, true);
                    }
                    if let Some((x, z)) = self.offset_xz(c, c, 0, offset) {
                        self.set(x, z, y, CellType::Bridge, true);
                    }
                }
            }
        }
        self.push_connection("typology_arcology_core", [c, 0, c], [c, self.layers - 1, c]);
    }

    fn add_linear_city_massing(&mut self) {
        let z = self.size / 2;
        let width = (self.size / 10).max(1);
        for x in 0..self.size {
            for dz in -(width as isize)..=(width as isize) {
                if let Some((nx, nz)) = self.offset_xz(x, z, 0, dz) {
                    for y in 0..self.layers.min((self.layers * 2 / 3).max(2)) {
                        let cell = if dz == 0 {
                            CellType::Horizontal
                        } else {
                            CellType::Vertical
                        };
                        self.set(nx, nz, y, cell, true);
                    }
                }
            }
        }
        self.push_connection("typology_linear_spine", [0, 1, z], [self.size - 1, 1, z]);
    }

    fn add_bridge_void_massing(&mut self) {
        let c = self.size / 2;
        let void = (self.size / 7).max(2);
        for x in c.saturating_sub(void)..=(c + void).min(self.size - 1) {
            for z in c.saturating_sub(void)..=(c + void).min(self.size - 1) {
                for y in 0..self.layers {
                    self.set(x, z, y, CellType::Empty, false);
                }
            }
        }
        let anchors =
            typology_anchor_points(MegastructureTypology::BridgeVoid, self.size, self.layers);
        for anchor in anchors {
            for y in 0..self.layers {
                self.set(anchor[0], anchor[2], y, CellType::Vertical, true);
            }
        }
        self.push_connection(
            "typology_bridge_void",
            [c.saturating_sub(void), 0, c.saturating_sub(void)],
            [
                (c + void).min(self.size - 1),
                self.layers - 1,
                (c + void).min(self.size - 1),
            ],
        );
    }

    fn add_marine_platform_massing(&mut self) {
        let deck_y = (self.layers / 4).max(1);
        let step = (self.size / 5).max(3);
        for x in 1..self.size.saturating_sub(1) {
            for z in 1..self.size.saturating_sub(1) {
                if x % step == 0
                    || z % step == 0
                    || (deck_y > 0 && hash_noise(self.seed_hash, x, z, deck_y) < 0.30)
                {
                    self.set(x, z, deck_y, CellType::Bridge, true);
                }
            }
        }
        for x in (step / 2..self.size).step_by(step) {
            for z in (step / 2..self.size).step_by(step) {
                for y in 0..=deck_y {
                    self.set(x, z, y, CellType::Vertical, true);
                }
            }
        }
        self.push_connection(
            "typology_marine_deck",
            [1, deck_y, 1],
            [self.size - 2, deck_y, self.size - 2],
        );
    }

    fn add_orbital_ring_massing(&mut self) {
        let c = self.size as f32 / 2.0;
        let ring_radius = self.size as f32 * 0.34;
        let tube = (self.size as f32 * 0.08).max(2.0);
        let y_mid = self.layers / 2;
        for x in 0..self.size {
            for z in 0..self.size {
                let dx = x as f32 - c;
                let dz = z as f32 - c;
                let radial = (dx * dx + dz * dz).sqrt();
                if (radial - ring_radius).abs() <= tube {
                    for y in y_mid.saturating_sub(2)..=(y_mid + 2).min(self.layers - 1) {
                        self.set(x, z, y, CellType::Horizontal, true);
                    }
                } else if radial < ring_radius - tube {
                    for y in 0..self.layers {
                        self.set(x, z, y, CellType::Empty, false);
                    }
                }
            }
        }
        self.push_connection(
            "typology_orbital_ring",
            [0, y_mid, self.size / 2],
            [self.size - 1, y_mid, self.size / 2],
        );
    }

    fn add_underground_hive_massing(&mut self) {
        let c = self.size / 2;
        for y in 0..(self.layers * 2 / 3).max(1) {
            let radius = (self.size / 3).saturating_sub(y / 4).max(3);
            for x in c.saturating_sub(radius)..=(c + radius).min(self.size - 1) {
                for z in c.saturating_sub(radius)..=(c + radius).min(self.size - 1) {
                    if (x + z + y) % 4 != 0 {
                        self.set(x, z, y, CellType::Horizontal, true);
                    }
                }
            }
        }
        self.push_connection(
            "typology_underground_hive",
            [c, 0, c],
            [c, self.layers / 2, c],
        );
    }

    fn add_mountain_burrow_massing(&mut self) {
        let cliff = self.size / 2;
        for x in 0..=cliff {
            for z in 1..self.size.saturating_sub(1) {
                let height = ((self.layers as f32) * (1.0 - x as f32 / (cliff + 1) as f32))
                    .round()
                    .max(2.0) as usize;
                for y in 0..height.min(self.layers) {
                    self.set(x, z, y, CellType::Horizontal, true);
                }
            }
        }
        self.push_connection(
            "typology_mountain_burrow",
            [0, 1, self.size / 2],
            [cliff, self.layers / 2, self.size / 2],
        );
    }

    fn add_desert_arcology_massing(&mut self) {
        self.add_arcology_spire_massing();
        let y = (self.layers / 5).max(1);
        for x in 1..self.size.saturating_sub(1) {
            for z in 1..self.size.saturating_sub(1) {
                if x % 5 == 0 || z % 5 == 0 {
                    self.set(x, z, y, CellType::Facade, true);
                }
            }
        }
        self.push_connection(
            "typology_desert_solar_field",
            [1, y, 1],
            [self.size - 2, y, self.size - 2],
        );
    }

    fn add_airport_city_massing(&mut self) {
        let z = self.size / 2;
        let runway = (self.size / 12).max(1);
        for x in 0..self.size {
            for dz in -(runway as isize)..=(runway as isize) {
                if let Some((nx, nz)) = self.offset_xz(x, z, 0, dz) {
                    self.set(nx, nz, 1, CellType::Bridge, true);
                }
            }
        }
        for x in (self.size / 6..self.size).step_by((self.size / 5).max(4)) {
            for y in 1..(self.layers / 2).max(2) {
                self.set(x, z.saturating_sub(runway + 2), y, CellType::Vertical, true);
            }
        }
        self.push_connection("typology_airport_runway", [0, 1, z], [self.size - 1, 1, z]);
    }

    fn add_dam_city_massing(&mut self) {
        let x = self.size / 2;
        let width = (self.size / 10).max(1);
        for dx in -(width as isize)..=(width as isize) {
            for z in 0..self.size {
                for y in 0..self.layers {
                    if let Some((nx, nz)) = self.offset_xz(x, z, dx, 0) {
                        self.set(nx, nz, y, CellType::Vertical, true);
                    }
                }
            }
        }
        self.push_connection(
            "typology_dam_wall",
            [x, 0, 0],
            [x, self.layers - 1, self.size - 1],
        );
    }

    fn add_shipyard_stack_massing(&mut self) {
        let z = self.size / 2;
        for x in 1..self.size.saturating_sub(1) {
            for y in 0..(self.layers / 3).max(2) {
                self.set(x, z, y, CellType::Bridge, true);
                if x % 4 == 0 {
                    self.set(x, z.saturating_sub(3), y, CellType::Vertical, true);
                    self.set(x, (z + 3).min(self.size - 1), y, CellType::Vertical, true);
                }
            }
        }
        self.push_connection(
            "typology_shipyard_drydock",
            [1, 1, z],
            [self.size - 2, 1, z],
        );
    }

    fn add_volcanic_caldera_massing(&mut self) {
        let c = self.size / 2;
        let inner = (self.size as f32 * 0.18).round() as usize;
        let outer = (self.size as f32 * 0.38).round() as usize;
        for x in 0..self.size {
            for z in 0..self.size {
                let dx = x.abs_diff(c);
                let dz = z.abs_diff(c);
                let r = ((dx * dx + dz * dz) as f32).sqrt() as usize;
                if r < inner {
                    for y in 0..self.layers {
                        self.set(x, z, y, CellType::Empty, false);
                    }
                    self.set(x, z, 0, CellType::Vent, false);
                } else if r <= outer {
                    let height = (self.layers / 2 + (outer.saturating_sub(r) / 2)).min(self.layers);
                    for y in 0..height {
                        self.set(x, z, y, CellType::Horizontal, true);
                    }
                }
            }
        }
        for y in 0..self.layers {
            self.set(c, c, y, CellType::Vent, false);
        }
        self.push_connection(
            "typology_volcanic_caldera",
            [c.saturating_sub(outer), 1, c],
            [(c + outer).min(self.size - 1), self.layers / 2, c],
        );
    }

    fn add_ice_shelf_city_massing(&mut self) {
        let shelf_y = (self.layers / 5).max(1);
        let step = (self.size / 6).max(4);
        for x in 0..self.size {
            for z in 0..self.size {
                if x % step == 0 || z % (step + 1) == 0 {
                    for y in 0..=shelf_y {
                        self.set(x, z, y, CellType::Vertical, true);
                    }
                } else if hash_noise(self.seed_hash, x, z, shelf_y) < 0.55 {
                    self.set(x, z, shelf_y, CellType::Bridge, true);
                }
                if (x + z) % (step + 2) == 0 {
                    self.set(x, z, shelf_y.saturating_add(1).min(self.layers - 1), CellType::Pipe, true);
                }
            }
        }
        self.push_connection(
            "typology_ice_shelf",
            [1, shelf_y, 1],
            [self.size - 2, shelf_y, self.size - 2],
        );
    }

    fn add_canopy_babel_massing(&mut self) {
        let c = self.size / 2;
        let canopy_y = (self.layers * 2 / 3).max(1).min(self.layers - 1);
        for anchor in typology_anchor_points(MegastructureTypology::CanopyBabel, self.size, self.layers) {
            for y in 0..=canopy_y {
                self.set(anchor[0], anchor[2], y, CellType::Vertical, true);
                if y % 4 == 0 {
                    for offset in -2..=2 {
                        if let Some((x, z)) = self.offset_xz(anchor[0], anchor[2], offset, 0) {
                            self.set(x, z, y, CellType::Bridge, true);
                        }
                    }
                }
            }
        }
        for x in c.saturating_sub(self.size / 3)..=(c + self.size / 3).min(self.size - 1) {
            for z in c.saturating_sub(self.size / 3)..=(c + self.size / 3).min(self.size - 1) {
                if hash_noise(self.seed_hash, x, z, canopy_y) < 0.50 {
                    self.set(x, z, canopy_y, CellType::Bridge, true);
                }
            }
        }
        self.push_connection("typology_canopy_babel", [c, 0, c], [c, canopy_y, c]);
    }

    fn add_space_elevator_anchor_massing(&mut self) {
        let c = self.size / 2;
        let radius = (self.size / 8).max(2);
        for y in 0..self.layers {
            for x in c.saturating_sub(radius)..=(c + radius).min(self.size - 1) {
                for z in c.saturating_sub(radius)..=(c + radius).min(self.size - 1) {
                    self.set(x, z, y, CellType::Vertical, true);
                }
            }
            if y % 4 == 0 {
                self.carve_route(
                    c.saturating_sub(self.size / 3),
                    c,
                    (c + self.size / 3).min(self.size - 1),
                    c,
                    y,
                    "cargo_ring",
                );
            }
        }
        self.push_connection("typology_space_elevator_anchor", [c, 0, c], [c, self.layers - 1, c]);
    }

    fn add_crawler_city_massing(&mut self) {
        let z = self.size / 2;
        let width = (self.size / 8).max(2);
        for x in 1..self.size.saturating_sub(1) {
            for dz in -(width as isize)..=(width as isize) {
                if let Some((nx, nz)) = self.offset_xz(x, z, 0, dz) {
                    for y in 0..(self.layers / 3).max(2) {
                        self.set(nx, nz, y, CellType::Horizontal, true);
                    }
                    if x % 5 == 0 {
                        self.set(nx, nz, 0, CellType::Bridge, true);
                    }
                }
            }
        }
        self.push_connection("typology_crawler_city", [1, 1, z], [self.size - 2, 1, z]);
    }

    fn add_reef_atoll_arcology_massing(&mut self) {
        let c = self.size / 2;
        let ring_radius = self.size as f32 * 0.34;
        let tube = (self.size as f32 * 0.08).max(2.0);
        let deck_y = (self.layers / 4).max(1);
        for x in 0..self.size {
            for z in 0..self.size {
                let dx = x as f32 - c as f32;
                let dz = z as f32 - c as f32;
                let radial = (dx * dx + dz * dz).sqrt();
                if (radial - ring_radius).abs() <= tube {
                    for y in 0..=deck_y {
                        self.set(x, z, y, if y == deck_y { CellType::Bridge } else { CellType::Vertical }, true);
                    }
                } else if radial < ring_radius - tube {
                    for y in 0..self.layers / 3 {
                        self.set(x, z, y, CellType::Empty, false);
                    }
                }
            }
        }
        self.push_connection(
            "typology_reef_atoll",
            [c, deck_y, c.saturating_sub(ring_radius as usize)],
            [c, deck_y, (c + ring_radius as usize).min(self.size - 1)],
        );
    }

    fn add_stratosphere_platform_massing(&mut self) {
        let c = self.size / 2;
        let deck_y = (self.layers * 2 / 3).max(1).min(self.layers - 1);
        let radius = self.size / 3;
        for x in c.saturating_sub(radius)..=(c + radius).min(self.size - 1) {
            for z in c.saturating_sub(radius)..=(c + radius).min(self.size - 1) {
                if hash_noise(self.seed_hash, x, z, deck_y) < 0.72 {
                    self.set(x, z, deck_y, CellType::Bridge, true);
                }
            }
        }
        for anchor in typology_anchor_points(MegastructureTypology::StratospherePlatform, self.size, self.layers) {
            for y in deck_y.saturating_sub(3)..=deck_y {
                self.set(anchor[0], anchor[2], y, CellType::Vertical, true);
            }
            self.set(anchor[0], anchor[2], (deck_y + 1).min(self.layers - 1), CellType::Vent, true);
        }
        self.push_connection("typology_stratosphere_platform", [c, deck_y, 1], [c, deck_y, self.size - 2]);
    }

    fn add_sinkhole_citadel_massing(&mut self) {
        let c = self.size / 2;
        let inner = (self.size as f32 * 0.16).round() as usize;
        let outer = (self.size as f32 * 0.30).round() as usize;
        for x in 0..self.size {
            for z in 0..self.size {
                let dx = x.abs_diff(c);
                let dz = z.abs_diff(c);
                let r = ((dx * dx + dz * dz) as f32).sqrt() as usize;
                if r < inner {
                    for y in 0..self.layers {
                        self.set(x, z, y, CellType::Empty, false);
                    }
                } else if r <= outer {
                    for y in 0..self.layers {
                        if y % 3 != 1 || r > inner + 1 {
                            self.set(x, z, y, CellType::Horizontal, true);
                        }
                    }
                }
            }
        }
        self.push_connection("typology_sinkhole_citadel", [c, 0, c.saturating_sub(outer)], [c, self.layers - 1, (c + outer).min(self.size - 1)]);
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
        self.add_ring_routes(&hubs);
        self.add_typology_routes(&hubs);
        self.add_vertical_transfer_links(&hubs);
        self.ensure_service_to_skyline_path(&hubs);
    }

    fn add_typology_routes(&mut self, hubs: &[(usize, usize)]) {
        match self.config.typology {
            MegastructureTypology::DenseEnclave => {}
            MegastructureTypology::ArcologySpire => {
                let c = self.size / 2;
                for y in [self.layers / 4, self.layers / 2, self.layers * 3 / 4] {
                    let y = y.clamp(1, self.layers.saturating_sub(1));
                    self.carve_route(
                        c.saturating_sub(self.size / 4),
                        c,
                        (c + self.size / 4).min(self.size - 1),
                        c,
                        y,
                        "station_loop",
                    );
                    self.carve_route(
                        c,
                        c.saturating_sub(self.size / 4),
                        c,
                        (c + self.size / 4).min(self.size - 1),
                        y,
                        "station_loop",
                    );
                }
            }
            MegastructureTypology::LinearCity => {
                let z = self.size / 2;
                let y = (self.layers / 3).max(1);
                self.carve_route(1, z, self.size - 2, z, y, "linear_express");
                for x in (self.size / 6..self.size).step_by((self.size / 5).max(4)) {
                    self.carve_route(
                        x,
                        z.saturating_sub(self.size / 5),
                        x,
                        (z + self.size / 5).min(self.size - 1),
                        y,
                        "station_loop",
                    );
                }
            }
            MegastructureTypology::BridgeVoid => {
                let anchors = if hubs.len() >= 4 {
                    hubs.iter().take(4).copied().collect::<Vec<_>>()
                } else {
                    typology_anchor_points(
                        MegastructureTypology::BridgeVoid,
                        self.size,
                        self.layers,
                    )
                    .into_iter()
                    .map(|point| (point[0], point[2]))
                    .collect()
                };
                let y = (self.layers * 2 / 3).max(1);
                for pair in anchors.windows(2) {
                    self.carve_route(pair[0].0, pair[0].1, pair[1].0, pair[1].1, y, "void_bridge");
                }
            }
            MegastructureTypology::MarinePlatform => {
                let y = (self.layers / 4).max(1);
                let mid = self.size / 2;
                self.carve_route(1, mid, self.size - 2, mid, y, "marine_causeway");
                self.carve_route(mid, 1, mid, self.size - 2, y, "marine_causeway");
                for anchor in typology_anchor_points(
                    MegastructureTypology::MarinePlatform,
                    self.size,
                    self.layers,
                ) {
                    self.carve_route(anchor[0], anchor[2], mid, mid, 1, "pylon_service");
                }
            }
            MegastructureTypology::OrbitalRing => {
                let points = typology_anchor_points(
                    MegastructureTypology::OrbitalRing,
                    self.size,
                    self.layers,
                );
                let y = self.layers / 2;
                for pair in points.windows(2) {
                    self.carve_route(
                        pair[0][0], pair[0][2], pair[1][0], pair[1][2], y, "rim_loop",
                    );
                }
                if let (Some(first), Some(last)) = (points.first(), points.last()) {
                    self.carve_route(last[0], last[2], first[0], first[2], y, "rim_loop");
                }
                let c = self.size / 2;
                for point in points.iter().step_by(2) {
                    self.carve_route(c, c, point[0], point[2], y, "spoke_transfer");
                }
            }
            MegastructureTypology::UndergroundHive => {
                let y = (self.layers / 4).max(1);
                let c = self.size / 2;
                self.carve_route(c, 1, c, self.size - 2, y, "hive_trunk");
                self.carve_route(1, c, self.size - 2, c, y, "hive_gallery");
                for x in (self.size / 6..self.size).step_by((self.size / 5).max(4)) {
                    self.carve_route(
                        x,
                        c.saturating_sub(self.size / 5),
                        x,
                        (c + self.size / 5).min(self.size - 1),
                        y,
                        "cavern_loop",
                    );
                }
            }
            MegastructureTypology::MountainBurrow => {
                let y = (self.layers / 3).max(1);
                let c = self.size / 2;
                self.carve_route(1, c, c, c, y, "cliff_gallery");
                self.carve_route(c / 2, 1, c / 2, self.size - 2, y, "burrow_spine");
            }
            MegastructureTypology::DesertArcology => {
                let c = self.size / 2;
                let y = (self.layers / 3).max(1);
                self.carve_route(c, 1, c, self.size - 2, y, "climate_spine");
                self.carve_route(1, c, self.size - 2, c, y, "solar_service_ring");
            }
            MegastructureTypology::AirportCity => {
                let z = self.size / 2;
                let y = (self.layers / 5).max(1);
                self.carve_route(1, z, self.size - 2, z, y, "runway_spine");
                for x in (self.size / 6..self.size).step_by((self.size / 5).max(4)) {
                    self.carve_route(
                        x,
                        z,
                        x,
                        (z + self.size / 4).min(self.size - 1),
                        y,
                        "terminal_loop",
                    );
                }
            }
            MegastructureTypology::DamCity => {
                let x = self.size / 2;
                let y = (self.layers / 3).max(1);
                self.carve_route(x, 1, x, self.size - 2, y, "dam_wall_spine");
                self.carve_route(
                    x.saturating_sub(self.size / 5),
                    self.size / 2,
                    (x + self.size / 5).min(self.size - 1),
                    self.size / 2,
                    y,
                    "turbine_gallery",
                );
            }
            MegastructureTypology::ShipyardStack => {
                let z = self.size / 2;
                let y = (self.layers / 4).max(1);
                self.carve_route(1, z, self.size - 2, z, y, "drydock_spine");
                self.carve_route(
                    1,
                    z.saturating_sub(self.size / 6),
                    self.size - 2,
                    z.saturating_sub(self.size / 6),
                    y,
                    "gantry_loop",
                );
                self.carve_route(
                    1,
                    (z + self.size / 6).min(self.size - 1),
                    self.size - 2,
                    (z + self.size / 6).min(self.size - 1),
                    y,
                    "gantry_loop",
                );
            }
            MegastructureTypology::VolcanicCaldera => {
                let points = typology_anchor_points(
                    MegastructureTypology::VolcanicCaldera,
                    self.size,
                    self.layers,
                );
                let y = (self.layers / 3).max(1);
                for pair in points.windows(2) {
                    self.carve_route(
                        pair[0][0],
                        pair[0][2],
                        pair[1][0],
                        pair[1][2],
                        y,
                        "caldera_ring",
                    );
                }
                if let (Some(first), Some(last)) = (points.first(), points.last()) {
                    self.carve_route(last[0], last[2], first[0], first[2], y, "caldera_ring");
                }
                let c = self.size / 2;
                self.carve_route(c, 1, c, self.size - 2, 1, "geothermal_shaft");
            }
            MegastructureTypology::IceShelfCity => {
                let y = (self.layers / 5).max(1);
                let c = self.size / 2;
                self.carve_route(1, c, self.size - 2, c, y, "meltwater_spine");
                for x in (self.size / 6..self.size).step_by((self.size / 5).max(4)) {
                    self.carve_route(
                        x,
                        c.saturating_sub(self.size / 4),
                        x,
                        (c + self.size / 4).min(self.size - 1),
                        y,
                        "crevasse_bridge",
                    );
                }
            }
            MegastructureTypology::CanopyBabel => {
                let y = (self.layers * 2 / 3).max(1).min(self.layers - 1);
                let c = self.size / 2;
                self.carve_route(1, c, self.size - 2, c, y, "canopy_walk");
                self.carve_route(c, 1, c, self.size - 2, y, "canopy_walk");
                self.carve_route(c, c, c, c, (self.layers / 4).max(1), "root_service");
            }
            MegastructureTypology::SpaceElevatorAnchor => {
                let c = self.size / 2;
                let y = (self.layers / 2).max(1);
                self.carve_route(c, 1, c, self.size - 2, y, "tether_core");
                self.carve_route(1, c, self.size - 2, c, y, "cargo_ring");
                self.carve_route(c, 1, c, self.size - 2, (self.layers / 4).max(1), "ground_anchor");
            }
            MegastructureTypology::CrawlerCity => {
                let z = self.size / 2;
                let y = (self.layers / 5).max(1);
                self.carve_route(1, z, self.size - 2, z, y, "crawler_track");
                self.carve_route(
                    1,
                    z.saturating_sub(self.size / 7),
                    self.size - 2,
                    z.saturating_sub(self.size / 7),
                    y,
                    "engine_spine",
                );
                self.carve_route(
                    1,
                    (z + self.size / 7).min(self.size - 1),
                    self.size - 2,
                    (z + self.size / 7).min(self.size - 1),
                    y,
                    "convoy_deck",
                );
            }
            MegastructureTypology::ReefAtollArcology => {
                let points = typology_anchor_points(
                    MegastructureTypology::ReefAtollArcology,
                    self.size,
                    self.layers,
                );
                let y = (self.layers / 4).max(1);
                for pair in points.windows(2) {
                    self.carve_route(pair[0][0], pair[0][2], pair[1][0], pair[1][2], y, "reef_ring");
                }
                if let (Some(first), Some(last)) = (points.first(), points.last()) {
                    self.carve_route(last[0], last[2], first[0], first[2], y, "reef_ring");
                }
                let c = self.size / 2;
                self.carve_route(1, c, self.size - 2, c, y, "lagoon_causeway");
            }
            MegastructureTypology::StratospherePlatform => {
                let y = (self.layers * 2 / 3).max(1).min(self.layers - 1);
                let c = self.size / 2;
                self.carve_route(1, c, self.size - 2, c, y, "pressure_deck");
                self.carve_route(c, 1, c, self.size - 2, y, "lift_cell_spine");
            }
            MegastructureTypology::SinkholeCitadel => {
                let points = typology_anchor_points(
                    MegastructureTypology::SinkholeCitadel,
                    self.size,
                    self.layers,
                );
                let y = (self.layers / 2).max(1);
                for pair in points.windows(2) {
                    self.carve_route(pair[0][0], pair[0][2], pair[1][0], pair[1][2], y, "sinkhole_ring");
                }
                if let (Some(first), Some(last)) = (points.first(), points.last()) {
                    self.carve_route(last[0], last[2], first[0], first[2], y, "sinkhole_ring");
                }
                let c = self.size / 2;
                self.carve_route(c, c.saturating_sub(self.size / 4), c, (c + self.size / 4).min(self.size - 1), 1, "descent_shaft");
            }
        }
    }

    fn add_ring_routes(&mut self, hubs: &[(usize, usize)]) {
        if hubs.len() < 4 {
            return;
        }
        let y = (self.layers / 3).max(1);
        for pair in hubs
            .iter()
            .step_by(2)
            .copied()
            .collect::<Vec<_>>()
            .windows(2)
        {
            self.carve_route(pair[0].0, pair[0].1, pair[1].0, pair[1].1, y, "ring_route");
        }
        if let (Some(first), Some(last)) = (hubs.first(), hubs.last()) {
            self.carve_route(last.0, last.1, first.0, first.1, y, "ring_route");
        }
    }

    fn add_vertical_transfer_links(&mut self, hubs: &[(usize, usize)]) {
        for hub in hubs.iter().take(4) {
            let low = (self.layers / 4).max(1);
            let high = (self.layers * 3 / 4).min(self.layers.saturating_sub(1));
            let start_node = self.push_transit_node("transfer_low", [hub.0, low, hub.1]);
            let end_node = self.push_transit_node("transfer_high", [hub.0, high, hub.1]);
            let points: Vec<_> = (low..=high).map(|y| [hub.0, y, hub.1]).collect();
            for point in &points {
                self.set(point[0], point[2], point[1], CellType::Elevator, true);
            }
            let route_id =
                self.push_transit_edge("vertical_transfer", start_node, end_node, points);
            let district = self.district_at(hub.0, hub.1);
            let room_id = self.push_room([hub.0, high, hub.1], district, "VERTICAL_TRANSFER");
            self.push_transit_attachment(
                route_id,
                room_id,
                "vertical_transfer",
                [hub.0, high, hub.1],
            );
            self.push_connection(
                "vertical_transfer",
                [hub.0, low, hub.1],
                [hub.0, high, hub.1],
            );
        }
    }

    fn ensure_service_to_skyline_path(&mut self, hubs: &[(usize, usize)]) {
        let start = hubs
            .first()
            .copied()
            .unwrap_or((self.size / 2, self.size / 2));
        let end = hubs.last().copied().unwrap_or(start);
        self.carve_route(start.0, start.1, end.0, end.1, 1, "service_tunnel");
        let skyline_y = self.layers.saturating_sub(2).max(1);
        self.carve_route(end.0, end.1, start.0, start.1, skyline_y, "skybridge");
        let start_node = self.push_transit_node("mission_transfer_low", [end.0, 1, end.1]);
        let end_node = self.push_transit_node("mission_transfer_high", [end.0, skyline_y, end.1]);
        let points: Vec<_> = (1..=skyline_y).map(|y| [end.0, y, end.1]).collect();
        for point in &points {
            self.set(point[0], point[2], point[1], CellType::Elevator, true);
        }
        let route_id =
            self.push_transit_edge("mission_vertical_transfer", start_node, end_node, points);
        let district = self.district_at(end.0, end.1);
        let room_id = self.push_room(
            [end.0, skyline_y, end.1],
            district,
            "SERVICE_TO_SKYLINE_LOCK",
        );
        self.push_transit_attachment(
            route_id,
            room_id,
            "mission_transfer",
            [end.0, skyline_y, end.1],
        );
        self.push_connection(
            "mission_vertical_transfer",
            [end.0, 1, end.1],
            [end.0, skyline_y, end.1],
        );
    }

    fn select_transit_hubs(&mut self) -> Vec<(usize, usize)> {
        let mut hubs = Vec::new();
        let target = (((4 + (self.size / 14)) as f32) * self.config.route_density)
            .round()
            .clamp(3.0, 12.0) as usize;
        let center = self.size / 2;
        match self.config.typology {
            MegastructureTypology::LinearCity => {
                let z = center;
                for x in (2..self.size.saturating_sub(2)).step_by((self.size / 5).max(4)) {
                    hubs.push((x, z));
                }
            }
            MegastructureTypology::OrbitalRing
            | MegastructureTypology::BridgeVoid
            | MegastructureTypology::MarinePlatform => {
                for point in typology_anchor_points(self.config.typology, self.size, self.layers) {
                    hubs.push((point[0], point[2]));
                }
            }
            _ => hubs.push((center, center)),
        }

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
                let lifecycle = self.lifecycle_for_district(district, 0.5);
                let rule_pack = self.rule_pack_for(district, BiomeStratum::Surface);
                let noise = hash_noise(self.seed_hash, x, z, 0);
                if noise
                    < (density_bias
                        * lifecycle.density_bias
                        * rule_pack.route_weight
                        * self.config.route_density)
                        .clamp(0.05, 0.95)
                {
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
            "skybridge"
            | "express_spine"
            | "void_bridge"
            | "rim_loop"
            | "spoke_transfer"
            | "marine_causeway"
            | "caldera_ring"
            | "crevasse_bridge"
            | "canopy_walk"
            | "cargo_ring"
            | "crawler_track"
            | "reef_ring"
            | "lagoon_causeway"
            | "pressure_deck"
            | "sinkhole_ring" => CellType::Bridge,
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
                self.add_route_aware_feature(
                    edge.id,
                    &edge.kind,
                    &edge.role,
                    edge.points[index],
                    slot,
                );
            }
        }
    }

    fn add_route_aware_feature(
        &mut self,
        route_id: usize,
        route_kind: &str,
        route_role: &str,
        position: [usize; 3],
        slot: usize,
    ) {
        let x = position[0].min(self.size - 1);
        let y = position[1].min(self.layers - 1);
        let z = position[2].min(self.size - 1);
        let district = self.district_at(x, z);
        let label = route_aware_room_label(
            route_kind,
            route_role,
            district,
            self.stratum_at(y),
            self.config.landmark_frequency,
            hash_noise(self.seed_hash, x + slot, z, y),
        );
        let room_id = self.push_room([x, y, z], district, label);
        self.push_transit_attachment(route_id, room_id, "route_aware_feature", [x, y, z]);
        self.paint_route_aware_feature(x, z, y, route_kind, route_role, label);
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
        route_role: &str,
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
            if matches!(route_role, "restricted_spine" | "evacuation_route")
                && y + 1 < self.layers
                && self.get(nx, nz, y + 1) == CellType::Empty
            {
                self.set(nx, nz, y + 1, CellType::Facade, false);
            }
            if route_role == "maintenance_backbone"
                && y > 0
                && self.get(nx, nz, y - 1) == CellType::Empty
            {
                self.set(nx, nz, y - 1, CellType::Pipe, false);
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

    fn phase2e_district_adjacency(&mut self) {
        let mut border_id = self.district_borders.len();
        for x in 0..self.size.saturating_sub(1) {
            for z in 0..self.size.saturating_sub(1) {
                for (nx, nz) in [(x + 1, z), (x, z + 1)] {
                    let a = self.district_at(x, z);
                    let b = self.district_at(nx, nz);
                    if a == b || hash_noise(self.seed_hash, x + nx, z + nz, border_id) > 0.18 {
                        continue;
                    }
                    let y = border_feature_y(a, b, self.layers);
                    let feature = border_feature_name(a, b);
                    let district = border_owner_district(a, b);
                    let position = [x, y, z];
                    let room_id = self.push_room(position, district, feature);
                    self.paint_border_feature(x, z, y, feature);
                    self.push_connection(feature, position, [nx, y, nz]);
                    self.district_borders.push(DistrictBorderRecord {
                        id: border_id,
                        from_district: a.name().to_owned(),
                        to_district: b.name().to_owned(),
                        bounds_min: [x.min(nx), z.min(nz)],
                        bounds_max: [x.max(nx), z.max(nz)],
                        y,
                        feature: feature.to_owned(),
                        route_ids: self.nearby_route_ids(position, 6),
                        room_ids: vec![room_id],
                    });
                    border_id += 1;
                }
            }
        }
    }

    fn nearby_route_ids(&self, position: [usize; 3], radius: usize) -> Vec<usize> {
        let mut route_ids = Vec::new();
        for edge in &self.transit_edges {
            if edge.points.iter().any(|point| {
                point[0].abs_diff(position[0])
                    + point[1].abs_diff(position[1])
                    + point[2].abs_diff(position[2])
                    <= radius
            }) {
                route_ids.push(edge.id);
                if route_ids.len() >= 4 {
                    break;
                }
            }
        }
        route_ids
    }

    fn paint_border_feature(&mut self, x: usize, z: usize, y: usize, feature: &str) {
        let cell = match feature {
            "SCRAP_MARKET" | "BORDER_MARKET" => CellType::Facade,
            "SECURITY_THRESHOLD" => CellType::Stair,
            "SURFACE_COMMONS" => CellType::Horizontal,
            "SCRAP_ZONE" => CellType::Debris,
            _ => CellType::Cable,
        };
        for dx in 0..=1 {
            for dz in 0..=1 {
                let nx = (x + dx).min(self.size - 1);
                let nz = (z + dz).min(self.size - 1);
                self.set(
                    nx,
                    nz,
                    y,
                    cell,
                    matches!(cell, CellType::Horizontal | CellType::Stair),
                );
                if matches!(feature, "SCRAP_ZONE" | "SCRAP_MARKET") && y > 0 {
                    self.set(nx, nz, y - 1, CellType::Debris, false);
                }
                if feature == "SECURITY_THRESHOLD" && y + 1 < self.layers {
                    self.set(nx, nz, y + 1, CellType::Facade, false);
                }
            }
        }
        self.record_pattern(feature);
    }

    fn phase2f_cellular_automata_patterns(&mut self) {
        if self.config.advanced_pattern_complexity <= 0.0 {
            return;
        }
        let iterations = (1.0 + self.config.advanced_pattern_complexity).round() as usize;
        for iteration in 0..iterations.clamp(1, 4) {
            let mut updates = Vec::new();
            for x in 1..self.size.saturating_sub(1) {
                for z in 1..self.size.saturating_sub(1) {
                    for y in 1..self.layers.saturating_sub(1) {
                        let cell = self.get(x, z, y);
                        let occupied_neighbors = 6 - self.count_empty_neighbors(x, z, y);
                        let district = self.district_at(x, z);
                        let lifecycle = self.lifecycle_for_district(district, 0.5);
                        let noise = hash_noise(self.seed_hash, x + iteration, z, y);
                        if cell == CellType::Empty
                            && occupied_neighbors >= 4
                            && lifecycle.occupancy_pressure > 0.65
                            && noise < 0.08 * self.config.advanced_pattern_complexity
                        {
                            let fill = if district == DistrictType::Industrial {
                                CellType::Pipe
                            } else if district == DistrictType::Slum {
                                CellType::Bridge
                            } else {
                                CellType::Horizontal
                            };
                            updates.push((
                                x,
                                z,
                                y,
                                fill,
                                matches!(fill, CellType::Horizontal | CellType::Bridge),
                            ));
                        } else if cell != CellType::Empty
                            && occupied_neighbors <= 1
                            && lifecycle.maintenance_level < 0.45
                            && noise > 1.0 - 0.06 * self.config.advanced_pattern_complexity
                        {
                            updates.push((x, z, y, CellType::Debris, false));
                        }
                    }
                }
            }
            for (x, z, y, cell, supported) in updates.into_iter().take(self.size * 3) {
                self.set(x, z, y, cell, supported);
                self.record_pattern("cellular_activity_field");
            }
        }
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

    fn phase3b_infrastructure_flows(&mut self) {
        let edges = self.transit_edges.clone();
        for edge in edges {
            let flow_kinds = flow_kinds_for_role(&edge.role);
            for (offset, kind) in flow_kinds.iter().enumerate() {
                self.add_infrastructure_flow(&edge, kind, offset);
            }
        }
    }

    fn phase3c_micro_details(&mut self) {
        let edges = self.transit_edges.clone();
        for edge in edges {
            let stride = (edge.points.len() / 4).max(2);
            for (index, point) in edge.points.iter().enumerate().step_by(stride) {
                let district =
                    self.district_at(point[0].min(self.size - 1), point[2].min(self.size - 1));
                let rule_pack = self.rule_pack_for(district, self.stratum_at(point[1]));
                if hash_noise(self.seed_hash, edge.id + index, point[2], point[1])
                    > rule_pack.detail_weight.clamp(0.15, 0.95)
                {
                    continue;
                }
                let kind = micro_detail_kind(&edge, index);
                self.paint_micro_detail(*point, kind);
                self.record_pattern(kind);
            }
        }
    }

    fn paint_micro_detail(&mut self, point: [usize; 3], kind: &str) {
        let x = point[0].min(self.size - 1);
        let y = point[1].min(self.layers - 1);
        let z = point[2].min(self.size - 1);
        let cell = match kind {
            "signage" => CellType::Facade,
            "barricade" => CellType::Stair,
            "leak" => CellType::Pipe,
            "antenna_cluster" => CellType::Antenna,
            "vent_cluster" => CellType::Vent,
            _ => CellType::Cable,
        };
        self.set(
            x,
            z,
            y,
            cell,
            matches!(cell, CellType::Facade | CellType::Stair),
        );
        if y + 1 < self.layers && matches!(kind, "signage" | "antenna_cluster") {
            self.set(x, z, y + 1, CellType::Antenna, false);
        }
        if y > 0 && matches!(kind, "leak" | "cable_bundle") {
            self.set(x, z, y - 1, CellType::Cable, false);
        }
    }

    fn add_infrastructure_flow(&mut self, edge: &TransitEdgeRecord, kind: &str, offset: usize) {
        if edge.points.is_empty() {
            return;
        }
        let stride = (edge.points.len() / 5).max(1);
        let sample_points: Vec<_> = edge
            .points
            .iter()
            .skip(offset.min(stride - 1))
            .step_by(stride)
            .copied()
            .take(6)
            .collect();
        for point in &sample_points {
            self.paint_flow_point(*point, kind);
        }
        let id = self.infrastructure_flows.len();
        self.infrastructure_flows.push(InfrastructureFlowRecord {
            id,
            kind: kind.to_owned(),
            route_id: edge.id,
            intensity: flow_intensity(kind, edge.role.as_str(), self.config.neon_intensity),
            source: edge.points.first().copied().unwrap_or([0, 0, 0]),
            sink: edge.points.last().copied().unwrap_or([0, 0, 0]),
            sample_points,
        });
        self.record_pattern(kind);
    }

    fn paint_flow_point(&mut self, point: [usize; 3], kind: &str) {
        let x = point[0].min(self.size - 1);
        let y = point[1].min(self.layers - 1);
        let z = point[2].min(self.size - 1);
        let cell = match kind {
            "power_bus" | "data_spine" => CellType::Cable,
            "water_reclamation" | "waste_chute" => CellType::Pipe,
            "ventilation_loop" => CellType::Vent,
            _ => CellType::Pipe,
        };
        self.set(x, z, y, cell, matches!(cell, CellType::Vent));
        if kind == "data_spine" && y + 1 < self.layers {
            self.set(x, z, y + 1, CellType::Antenna, false);
        }
        if kind == "waste_chute" && y > 0 {
            self.set(x, z, y - 1, CellType::Debris, false);
        }
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

    fn phase4c_hazard_zones(&mut self) {
        let edges = self.transit_edges.clone();
        for edge in edges {
            if edge.points.is_empty() {
                continue;
            }
            let anchor = edge.points[edge.points.len() / 2];
            let district =
                self.district_at(anchor[0].min(self.size - 1), anchor[2].min(self.size - 1));
            let lifecycle = self.lifecycle_for_district(district, 0.5);
            let rule_pack = self.rule_pack_for(district, self.stratum_at(anchor[1]));
            let stress_pressure = self.route_stress_score_for_edge(&edge, anchor);
            let probability = hazard_probability_for_role(&edge.role)
                * lifecycle.decay_bias
                * rule_pack.decay_weight
                * (1.0 + self.failure_pressure_at(anchor) * 0.45)
                * (1.0 + stress_pressure * 0.75)
                * self.config.decay_story_density;
            if hash_noise(self.seed_hash, edge.id, edge.length, self.layers)
                > probability.clamp(0.0, 0.95)
            {
                continue;
            }
            let kind = hazard_kind_for_edge(&edge);
            self.add_hazard_zone_with_severity(kind, anchor, vec![edge.id], stress_pressure * 0.20);
        }
        self.add_typology_hazard_zones();
        self.add_stress_hazard_zones();
    }

    fn add_typology_hazard_zones(&mut self) {
        let specs: &[(&str, &[&str])] = match self.config.typology {
            MegastructureTypology::DenseEnclave => return,
            MegastructureTypology::ArcologySpire => &[
                ("core_lockdown", &["vertical_transit_core"]),
                ("atrium_stack_fire", &["station_loop"]),
                (
                    "elevator_choke",
                    &["vertical_transfer", "vertical_transit_core"],
                ),
            ],
            MegastructureTypology::LinearCity => &[
                ("station_crush", &["station_loop"]),
                ("transit_bottleneck", &["linear_express"]),
                (
                    "infrastructure_cascade",
                    &["linear_express", "service_tunnel"],
                ),
            ],
            MegastructureTypology::BridgeVoid => &[
                ("cantilever_fatigue", &["void_bridge"]),
                ("wind_shear", &["void_bridge", "skybridge"]),
                ("bridge_failure", &["void_bridge"]),
            ],
            MegastructureTypology::MarinePlatform => &[
                ("salt_corrosion", &["pylon_service"]),
                ("storm_surge", &["marine_causeway"]),
                ("pump_outage", &["pylon_service", "service_tunnel"]),
            ],
            MegastructureTypology::OrbitalRing => &[
                ("pressure_breach", &["rim_loop"]),
                ("spoke_shear", &["spoke_transfer"]),
                ("rim_blackout", &["rim_loop"]),
            ],
            MegastructureTypology::UndergroundHive => &[
                ("cavern_collapse", &["cavern_loop"]),
                ("methane_pocket", &["hive_gallery"]),
                ("sump_flood", &["hive_trunk"]),
            ],
            MegastructureTypology::MountainBurrow => &[
                ("rockfall_choke", &["cliff_gallery"]),
                ("slope_shear", &["burrow_spine"]),
                ("ventilation_dead_zone", &["cliff_gallery"]),
            ],
            MegastructureTypology::DesertArcology => &[
                ("heat_bloom", &["solar_service_ring"]),
                ("seal_failure", &["climate_spine"]),
                ("water_reclaimer_outage", &["climate_spine"]),
            ],
            MegastructureTypology::AirportCity => &[
                ("runway_debris", &["runway_spine"]),
                ("terminal_crush", &["terminal_loop"]),
                ("fuel_line_fire", &["runway_spine"]),
            ],
            MegastructureTypology::DamCity => &[
                ("spillway_surge", &["dam_wall_spine"]),
                ("turbine_trip", &["turbine_gallery"]),
                ("wall_seepage", &["dam_wall_spine"]),
            ],
            MegastructureTypology::ShipyardStack => &[
                ("drydock_flood", &["drydock_spine"]),
                ("gantry_collapse", &["gantry_loop"]),
                ("weld_fire", &["drydock_spine"]),
            ],
            MegastructureTypology::VolcanicCaldera => &[
                ("lava_tube_breach", &["geothermal_shaft"]),
                ("ashfall_choke", &["caldera_ring"]),
                ("geothermal_blowout", &["geothermal_shaft"]),
            ],
            MegastructureTypology::IceShelfCity => &[
                ("thermal_fracture", &["crevasse_bridge"]),
                ("meltwater_surge", &["meltwater_spine"]),
                ("crevasse_shear", &["crevasse_bridge"]),
            ],
            MegastructureTypology::CanopyBabel => &[
                ("canopy_fire", &["canopy_walk"]),
                ("trunk_rot", &["root_service"]),
                ("root_service_collapse", &["root_service"]),
            ],
            MegastructureTypology::SpaceElevatorAnchor => &[
                ("tether_shear", &["tether_core"]),
                ("cargo_ring_lockdown", &["cargo_ring"]),
                ("anchor_quake", &["ground_anchor"]),
            ],
            MegastructureTypology::CrawlerCity => &[
                ("track_collapse", &["crawler_track"]),
                ("engine_fire", &["engine_spine"]),
                ("convoy_jam", &["convoy_deck"]),
            ],
            MegastructureTypology::ReefAtollArcology => &[
                ("reef_bleach", &["reef_ring"]),
                ("lagoon_surge", &["lagoon_causeway"]),
                ("pylon_scour", &["reef_ring"]),
            ],
            MegastructureTypology::StratospherePlatform => &[
                ("lift_cell_leak", &["lift_cell_spine"]),
                ("wind_shear", &["pressure_deck"]),
                ("pressure_deck_breach", &["pressure_deck"]),
            ],
            MegastructureTypology::SinkholeCitadel => &[
                ("rim_rockfall", &["sinkhole_ring"]),
                ("sump_gas", &["descent_shaft"]),
                ("descent_collapse", &["descent_shaft"]),
            ],
        };
        for (offset, (kind, route_kinds)) in specs.iter().enumerate() {
            if self.hazard_zones.iter().any(|hazard| hazard.kind == *kind) {
                continue;
            }
            let edge = self
                .transit_edges
                .iter()
                .find(|edge| {
                    route_kinds
                        .iter()
                        .any(|route_kind| edge.kind.contains(route_kind))
                })
                .or_else(|| {
                    self.transit_edges
                        .get(offset % self.transit_edges.len().max(1))
                });
            let Some(edge) = edge else {
                continue;
            };
            let anchor = edge.points.get(edge.points.len() / 2).copied().unwrap_or([
                self.size / 2,
                (self.layers / 2).max(1),
                self.size / 2,
            ]);
            self.add_hazard_zone(kind, anchor, vec![edge.id]);
        }
    }

    fn add_hazard_zone(&mut self, kind: &str, anchor: [usize; 3], route_ids: Vec<usize>) {
        self.add_hazard_zone_with_severity(kind, anchor, route_ids, 0.0);
    }

    fn add_hazard_zone_with_severity(
        &mut self,
        kind: &str,
        anchor: [usize; 3],
        route_ids: Vec<usize>,
        severity_boost: f32,
    ) {
        let radius = if self.config.erosion_strength > 1.2 {
            2
        } else {
            1
        };
        let mut bounds_min = anchor;
        let mut bounds_max = anchor;
        for dx in -(radius as isize)..=(radius as isize) {
            for dz in -(radius as isize)..=(radius as isize) {
                let nx = (anchor[0] as isize + dx).clamp(0, self.size as isize - 1) as usize;
                let nz = (anchor[2] as isize + dz).clamp(0, self.size as isize - 1) as usize;
                let y = anchor[1].min(self.layers - 1);
                self.paint_hazard_cell(nx, nz, y, kind);
                bounds_min[0] = bounds_min[0].min(nx);
                bounds_min[2] = bounds_min[2].min(nz);
                bounds_max[0] = bounds_max[0].max(nx);
                bounds_max[2] = bounds_max[2].max(nz);
            }
        }
        let room_ids = self
            .rooms
            .iter()
            .filter(|room| {
                room.position[0] >= bounds_min[0]
                    && room.position[0] <= bounds_max[0]
                    && room.position[2] >= bounds_min[2]
                    && room.position[2] <= bounds_max[2]
                    && room.position[1].abs_diff(anchor[1]) <= 2
            })
            .map(|room| room.id)
            .collect();
        let id = self.hazard_zones.len();
        self.hazard_zones.push(HazardZoneRecord {
            id,
            kind: kind.to_owned(),
            severity: (0.35 + self.config.erosion_strength * 0.22 + severity_boost).clamp(0.0, 1.0),
            bounds_min,
            bounds_max,
            route_ids,
            room_ids,
        });
        self.record_pattern(kind);
    }

    fn add_stress_hazard_zones(&mut self) {
        let mut candidates: Vec<_> = self
            .transit_edges
            .iter()
            .filter_map(|edge| {
                let anchor = edge.points.get(edge.points.len() / 2).copied()?;
                let stress = self.route_stress_score_for_edge(edge, anchor);
                (stress >= 0.46).then_some((
                    stress,
                    edge.id,
                    stress_hazard_kind_for_edge(edge),
                    anchor,
                ))
            })
            .collect();
        candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (stress, route_id, kind, anchor) in candidates.into_iter().take(4) {
            if self
                .hazard_zones
                .iter()
                .any(|hazard| hazard.kind == kind && hazard.route_ids.contains(&route_id))
            {
                continue;
            }
            self.add_hazard_zone_with_severity(kind, anchor, vec![route_id], stress * 0.28);
        }
    }

    fn paint_hazard_cell(&mut self, x: usize, z: usize, y: usize, kind: &str) {
        let cell = match kind {
            "flood_sump"
            | "unstable_span"
            | "storm_surge"
            | "bridge_failure"
            | "cantilever_fatigue"
            | "spoke_shear"
            | "cavern_collapse"
            | "rockfall_choke"
            | "slope_shear"
            | "runway_debris"
            | "gantry_collapse"
            | "ashfall_choke"
            | "thermal_fracture"
            | "crevasse_shear"
            | "trunk_rot"
            | "root_service_collapse"
            | "tether_shear"
            | "anchor_quake"
            | "track_collapse"
            | "convoy_jam"
            | "pylon_scour"
            | "rim_rockfall"
            | "descent_collapse"
            | "stress_bridge_failure"
            | "stress_pylon_failure"
            | "stress_spoke_shear"
            | "stress_gantry_failure"
            | "stress_deck_crack"
            | "stress_cavern_shift"
            | "stress_slope_shear"
            | "stress_ice_fracture"
            | "stress_canopy_failure"
            | "stress_tether_shear"
            | "stress_track_failure"
            | "stress_pylon_scour"
            | "stress_lift_cell_failure"
            | "stress_rim_collapse"
            | "stress_frame_overload" => CellType::Debris,
            "security_sweep"
            | "core_lockdown"
            | "station_crush"
            | "seal_failure"
            | "terminal_crush"
            | "cargo_ring_lockdown"
            | "pressure_deck_breach"
            | "wall_seepage"
            | "stress_wall_seepage" => CellType::Facade,
            "blackout_pocket"
            | "rim_blackout"
            | "infrastructure_cascade"
            | "pump_outage"
            | "elevator_choke"
            | "methane_pocket"
            | "water_reclaimer_outage"
            | "turbine_trip"
            | "fuel_line_fire"
            | "weld_fire"
            | "geothermal_blowout"
            | "stress_geothermal_breach"
            | "lift_cell_leak"
            | "engine_fire" => CellType::Cable,
            _ => CellType::Vent,
        };
        self.set(x, z, y, cell, matches!(cell, CellType::Facade));
        if matches!(
            kind,
            "vent_heat_plume"
                | "pressure_breach"
                | "atrium_stack_fire"
                | "lava_tube_breach"
                | "canopy_fire"
                | "sump_gas"
        ) && y + 1 < self.layers
        {
            self.set(x, z, y + 1, CellType::Vent, false);
        }
    }

    fn phase5_story_details(&mut self) {
        self.add_landmark_rooms();
        self.apply_landmark_aware_geometry();
        self.add_debris_fields();
        self.add_hanging_bridge_remnants();
        self.add_broken_facade_fields();
    }

    fn phase5b_program_aware_rooms(&mut self) {
        let specs: &[(&str, &[&str])] = match self.config.typology {
            MegastructureTypology::DenseEnclave => &[
                ("RELAY_ROOM", &["ring_route", "express_spine"]),
                ("EVAC_SHAFT_CONTROL", &["vertical_transit_core"]),
            ],
            MegastructureTypology::ArcologySpire => &[
                ("CORE_CONTROL_ROOM", &["vertical_transit_core"]),
                ("ATRIUM_SMOKE_CONTROL", &["station_loop"]),
            ],
            MegastructureTypology::LinearCity => &[
                ("RELAY_ROOM", &["linear_express"]),
                ("STATION_CONTROL", &["station_loop"]),
            ],
            MegastructureTypology::BridgeVoid => &[
                ("COUNTERWEIGHT_ROOM", &["void_bridge"]),
                ("WIND_BARRIER_CONTROL", &["void_bridge"]),
            ],
            MegastructureTypology::MarinePlatform => &[
                ("PUMP_ROOM", &["pylon_service"]),
                ("CAUSEWAY_LOCK_CONTROL", &["marine_causeway"]),
            ],
            MegastructureTypology::OrbitalRing => &[
                ("AIRLOCK_CONTROL", &["rim_loop"]),
                ("SPOKE_RELAY_ROOM", &["spoke_transfer"]),
            ],
            MegastructureTypology::UndergroundHive => &[
                ("SUMP_CONTROL", &["hive_trunk"]),
                ("CAVERN_SURVEY_ROOM", &["cavern_loop"]),
            ],
            MegastructureTypology::MountainBurrow => &[
                ("VENTILATION_ROOM", &["cliff_gallery"]),
                ("ROCK_BOLT_CONTROL", &["burrow_spine"]),
            ],
            MegastructureTypology::DesertArcology => &[
                ("CLIMATE_PLANT", &["climate_spine"]),
                ("SOLAR_RELAY_ROOM", &["solar_service_ring"]),
            ],
            MegastructureTypology::AirportCity => &[
                ("HANGAR_CONTROL", &["runway_spine"]),
                ("TERMINAL_RELAY_ROOM", &["terminal_loop"]),
            ],
            MegastructureTypology::DamCity => &[
                ("TURBINE_HALL", &["turbine_gallery"]),
                ("SPILLWAY_CONTROL", &["dam_wall_spine"]),
            ],
            MegastructureTypology::ShipyardStack => &[
                ("DRYDOCK_CONTROL", &["drydock_spine"]),
                ("GANTRY_OPERATOR_ROOM", &["gantry_loop"]),
            ],
        };
        for (label, route_kinds) in specs {
            if self.rooms.iter().any(|room| room.label == *label) {
                continue;
            }
            let edge = self
                .transit_edges
                .iter()
                .find(|edge| route_kinds.iter().any(|kind| edge.kind == *kind))
                .or_else(|| self.transit_edges.first());
            let Some(edge) = edge else {
                continue;
            };
            let position = edge.points.get(edge.points.len() / 2).copied().unwrap_or([
                self.size / 2,
                (self.layers / 2).max(1),
                self.size / 2,
            ]);
            let district = self.district_at(position[0], position[2]);
            self.push_room(position, district, label);
            self.record_pattern(&format!("program_room_{}", label.to_ascii_lowercase()));
        }
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

    fn apply_landmark_aware_geometry(&mut self) {
        let rooms = self.rooms.clone();
        for room in rooms {
            match room.label.as_str() {
                "SHRINE" => self.carve_landmark_plaza(room.position, "shrine_plaza"),
                "DATA_VAULT" | "SKY_VAULT" => self.harden_landmark_vault(room.position),
                "MAINTENANCE_SHAFT" | "MACHINE_ROOM" => {
                    self.extend_landmark_service_shaft(room.position)
                }
                label if label.contains("MARKET") || label.contains("BAZAAR") => {
                    self.widen_landmark_market_routes(room.position)
                }
                _ => {}
            }
        }

        let hazards = self.hazard_zones.clone();
        for hazard in hazards {
            let center = [
                (hazard.bounds_min[0] + hazard.bounds_max[0]) / 2,
                (hazard.bounds_min[1] + hazard.bounds_max[1]) / 2,
                (hazard.bounds_min[2] + hazard.bounds_max[2]) / 2,
            ];
            self.scar_landmark_hazard(center, &hazard.kind);
        }
    }

    fn carve_landmark_plaza(&mut self, position: [usize; 3], pattern: &str) {
        let [x, y, z] = position;
        for dx in -2isize..=2 {
            for dz in -2isize..=2 {
                if dx.abs() + dz.abs() > 3 {
                    continue;
                }
                if let Some((nx, nz)) = self.offset_xz(x, z, dx, dz) {
                    self.set(nx, nz, y, CellType::Horizontal, true);
                    if y + 1 < self.layers && (dx == 0 || dz == 0) {
                        self.set(nx, nz, y + 1, CellType::Cable, false);
                    }
                }
            }
        }
        self.push_connection(pattern, position, position);
        self.record_pattern("landmark_plaza");
    }

    fn harden_landmark_vault(&mut self, position: [usize; 3]) {
        let route_ids = self.nearby_route_ids(position, 8);
        for route_id in route_ids {
            let Some(edge) = self.transit_edges.get(route_id).cloned() else {
                continue;
            };
            for point in edge.points.iter().step_by(2) {
                for (dx, dz) in [(0isize, 0isize), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if let Some((nx, nz)) = self.offset_xz(point[0], point[2], dx, dz) {
                        self.set(nx, nz, point[1], CellType::Facade, false);
                        if point[1] + 1 < self.layers && edge.role == "restricted_spine" {
                            self.set(nx, nz, point[1] + 1, CellType::Antenna, false);
                        }
                    }
                }
            }
        }
        self.push_connection("landmark_vault_spine", position, position);
        self.record_pattern("landmark_vault_spine");
    }

    fn extend_landmark_service_shaft(&mut self, position: [usize; 3]) {
        let [x, y, z] = position;
        let low = y.saturating_sub(3);
        let high = (y + 3).min(self.layers - 1);
        for ny in low..=high {
            self.set(x, z, ny, CellType::Pipe, false);
            if x + 1 < self.size {
                self.set(x + 1, z, ny, CellType::Vent, false);
            }
        }
        self.push_connection("landmark_service_shaft", [x, low, z], [x, high, z]);
        self.record_pattern("landmark_service_shaft");
    }

    fn widen_landmark_market_routes(&mut self, position: [usize; 3]) {
        let route_ids = self.nearby_route_ids(position, 7);
        for route_id in route_ids {
            let Some(edge) = self.transit_edges.get(route_id).cloned() else {
                continue;
            };
            for point in edge.points.iter().step_by(2) {
                for (dx, dz) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                    if let Some((nx, nz)) = self.offset_xz(point[0], point[2], dx, dz) {
                        self.set(nx, nz, point[1], CellType::Facade, false);
                    }
                }
            }
        }
        self.push_connection("landmark_market_route_widening", position, position);
        self.record_pattern("landmark_market_route_widening");
    }

    fn scar_landmark_hazard(&mut self, position: [usize; 3], hazard_kind: &str) {
        let [x, y, z] = position;
        for dx in -2isize..=2 {
            for dz in -2isize..=2 {
                if dx.abs() + dz.abs() > 3 {
                    continue;
                }
                if let Some((nx, nz)) = self.offset_xz(x, z, dx, dz) {
                    let scar = match hazard_kind {
                        "security_sweep" => CellType::Facade,
                        "blackout_pocket" => CellType::Cable,
                        "vent_heat_plume" => CellType::Vent,
                        _ => CellType::Debris,
                    };
                    self.set(nx, nz, y, scar, scar == CellType::Facade);
                    if y > 0 && matches!(scar, CellType::Debris | CellType::Cable) {
                        self.set(nx, nz, y - 1, CellType::Debris, false);
                    }
                }
            }
        }
        self.push_connection("landmark_hazard_scar", position, position);
        self.record_pattern("landmark_hazard_scar");
    }

    fn offset_xz(&self, x: usize, z: usize, dx: isize, dz: isize) -> Option<(usize, usize)> {
        let nx = x as isize + dx;
        let nz = z as isize + dz;
        (nx >= 0 && nz >= 0 && nx < self.size as isize && nz < self.size as isize)
            .then_some((nx as usize, nz as usize))
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

    fn phase6_entity_dynamics(&mut self) {
        self.entities.clear();
        self.entity_paths.clear();
        self.entity_pressure_fields.clear();
        self.layout_mutations.clear();
        if self.config.entity_density <= 0.0 || self.transit_edges.is_empty() {
            return;
        }

        let (room_clusters, _) = self.room_clusters();
        if room_clusters.is_empty() {
            return;
        }
        let failure_zones = self.failure_propagation_records();
        let resource_networks = self.resource_networks();
        let (factions, _, contested_borders) = self.ownership_layer(&room_clusters);
        let temporal_state = self.temporal_state(&factions, &contested_borders, &resource_networks);
        let route_simulation =
            self.route_simulation(&temporal_state, &resource_networks, &failure_zones);

        self.spawn_entity_records(
            &room_clusters,
            &factions,
            &temporal_state,
            &route_simulation,
        );
        self.build_entity_pressure_fields();
        self.apply_entity_layout_mutations(&temporal_state);
    }

    fn spawn_entity_records(
        &mut self,
        room_clusters: &[RoomClusterRecord],
        factions: &[FactionRecord],
        temporal_state: &TemporalStateRecord,
        route_simulation: &[RouteSimulationRecord],
    ) {
        let density_weight = self.average_entity_density_weight(room_clusters);
        let target = ((self.transit_edges.len() + room_clusters.len() / 3) as f32
            * self.config.entity_density
            * density_weight
            * 0.85)
            .round()
            .clamp(0.0, 96.0) as usize;
        if target == 0 {
            return;
        }

        let mut candidates: Vec<_> = room_clusters
            .iter()
            .map(|cluster| {
                let density_bias =
                    self.entity_rule_weight_at(cluster.anchor_position, "density", "market_crowd");
                let pressure = (cluster.room_ids.len() as f32
                    + cluster.route_ids.len() as f32 * 2.0
                    + hash_noise(
                        self.seed_hash,
                        cluster.id,
                        cluster.anchor_position[2],
                        cluster.anchor_position[1],
                    ))
                    * density_bias;
                (pressure, cluster)
            })
            .collect();
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));

        for (_, cluster) in candidates.into_iter().cycle().take(target) {
            let entity_id = self.entities.len();
            let kind = entity_kind_for_cluster(cluster);
            if hash_noise(
                self.seed_hash,
                entity_id,
                cluster.id,
                cluster.anchor_position[1],
            ) > self
                .entity_rule_weight_at(cluster.anchor_position, "kind", kind)
                .clamp(0.05, 1.0)
            {
                continue;
            }
            let active_phase_ids = active_phases_for_entity(kind, temporal_state);
            let route_ids = self.entity_route_ids(cluster, kind, route_simulation);
            if route_ids.is_empty() {
                continue;
            }
            let origin = cluster.anchor_position;
            let destination = self
                .transit_edges
                .get(*route_ids.last().unwrap_or(&route_ids[0]))
                .and_then(|edge| edge.points.last().copied())
                .unwrap_or(origin);
            let faction_id = faction_id_by_name(factions, &cluster.owner_district);
            let layout_influence = entity_layout_influence(kind, cluster, &route_ids);
            self.entities.push(EntityRecord {
                id: entity_id,
                kind: kind.to_owned(),
                faction_id,
                home_cluster_id: Some(cluster.id),
                origin,
                destination,
                route_ids: route_ids.clone(),
                active_phase_ids: active_phase_ids.clone(),
                movement_profile: movement_profile_for_entity(kind).to_owned(),
                layout_influence,
            });
            let sample_points = self.entity_sample_points(origin, destination, &route_ids);
            let (congestion, risk) = entity_path_pressure(&route_ids, route_simulation);
            self.entity_paths.push(EntityPathRecord {
                id: self.entity_paths.len(),
                entity_id,
                sample_points,
                route_ids,
                travel_cost: (1.0 + congestion * 1.6 + risk * 2.0).clamp(0.0, 5.0),
                congestion,
                risk,
                reaches_destination: true,
            });
            self.record_pattern(kind);
        }
    }

    fn entity_route_ids(
        &self,
        cluster: &RoomClusterRecord,
        kind: &str,
        route_simulation: &[RouteSimulationRecord],
    ) -> Vec<usize> {
        let start_routes = self.entity_start_routes(cluster);
        let goal_route = self.best_entity_destination_route(cluster, kind, route_simulation);
        let graph = self.entity_route_graph(kind, route_simulation);
        let mut best_path = Vec::new();
        let mut best_cost = f32::MAX;
        for start_route in start_routes {
            let (path, cost) = shortest_route_path(&graph, start_route, goal_route);
            if !path.is_empty() && cost < best_cost {
                best_cost = cost;
                best_path = path;
            }
        }
        if best_path.is_empty() {
            best_path.push(self.best_entity_route(kind, route_simulation));
        }
        best_path.truncate(6);
        best_path
    }

    fn entity_start_routes(&self, cluster: &RoomClusterRecord) -> Vec<usize> {
        let mut route_ids = cluster.route_ids.clone();
        if route_ids.is_empty() {
            route_ids = self.nearby_route_ids(cluster.anchor_position, 10);
        }
        if route_ids.is_empty() && !self.transit_edges.is_empty() {
            route_ids.push(0);
        }
        route_ids.sort_unstable();
        route_ids.dedup();
        route_ids
    }

    fn best_entity_destination_route(
        &self,
        cluster: &RoomClusterRecord,
        kind: &str,
        route_simulation: &[RouteSimulationRecord],
    ) -> usize {
        self.transit_edges
            .iter()
            .map(|edge| {
                let midpoint = edge
                    .points
                    .get(edge.points.len() / 2)
                    .copied()
                    .unwrap_or([0, 0, 0]);
                let distance = midpoint[0].abs_diff(cluster.anchor_position[0])
                    + midpoint[1].abs_diff(cluster.anchor_position[1])
                    + midpoint[2].abs_diff(cluster.anchor_position[2]);
                let distance_bonus = (distance as f32 / self.size.max(1) as f32).clamp(0.0, 1.5);
                (
                    self.entity_route_score(edge.id, kind, route_simulation) + distance_bonus,
                    edge.id,
                )
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
            .unwrap_or(0)
    }

    fn entity_route_graph(
        &self,
        kind: &str,
        route_simulation: &[RouteSimulationRecord],
    ) -> Vec<Vec<(usize, f32)>> {
        let edge_count = self.transit_edges.len();
        let mut graph = vec![Vec::new(); edge_count];
        for a in 0..edge_count {
            for b in (a + 1)..edge_count {
                if self.routes_touch(a, b) {
                    let cost_ab = self.entity_route_cost(b, kind, route_simulation);
                    let cost_ba = self.entity_route_cost(a, kind, route_simulation);
                    graph[a].push((b, cost_ab));
                    graph[b].push((a, cost_ba));
                }
            }
        }
        graph
    }

    fn routes_touch(&self, a: usize, b: usize) -> bool {
        let Some(left) = self.transit_edges.get(a) else {
            return false;
        };
        let Some(right) = self.transit_edges.get(b) else {
            return false;
        };
        if left.start_node == right.start_node
            || left.start_node == right.end_node
            || left.end_node == right.start_node
            || left.end_node == right.end_node
        {
            return true;
        }
        for left_point in left.points.iter().step_by((left.points.len() / 8).max(1)) {
            if right
                .points
                .iter()
                .step_by((right.points.len() / 8).max(1))
                .any(|right_point| {
                    left_point[0].abs_diff(right_point[0])
                        + left_point[1].abs_diff(right_point[1])
                        + left_point[2].abs_diff(right_point[2])
                        <= 2
                })
            {
                return true;
            }
        }
        false
    }

    fn best_entity_route(&self, kind: &str, route_simulation: &[RouteSimulationRecord]) -> usize {
        self.transit_edges
            .iter()
            .map(|edge| {
                (
                    self.entity_route_score(edge.id, kind, route_simulation),
                    edge.id,
                )
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
            .unwrap_or(0)
    }

    fn entity_route_score(
        &self,
        route_id: usize,
        kind: &str,
        route_simulation: &[RouteSimulationRecord],
    ) -> f32 {
        let Some(edge) = self.transit_edges.get(route_id) else {
            return 0.0;
        };
        let simulation = route_simulation.get(route_id);
        let congestion = simulation
            .map(|sim| sim.market_congestion.max(sim.civilian_density))
            .unwrap_or(0.0);
        let risk = simulation
            .map(|sim| sim.blackout_risk.max(sim.security_pressure))
            .unwrap_or(0.0);
        let role_bonus = match (kind, edge.role.as_str()) {
            ("market_crowd", "market_run") => 0.42,
            ("corp_patrol", "restricted_spine") => 0.45,
            ("evacuee_flow", "evacuation_route") => 0.48,
            ("maintenance_crawler", "maintenance_backbone") => 0.42,
            ("builder_swarm", "primary_artery" | "maintenance_backbone") => 0.30,
            ("scavenger_drift", "service_loop") => 0.34,
            _ => 0.08,
        };
        let length_bias = (edge.length as f32 / self.size.max(1) as f32).clamp(0.0, 1.0) * 0.18;
        match kind {
            "corp_patrol" => role_bonus + risk * 0.22 + length_bias,
            "evacuee_flow" => role_bonus + (1.0 - risk) * 0.28 + length_bias,
            "market_crowd" => role_bonus + congestion * 0.34 + length_bias,
            "scavenger_drift" => role_bonus + risk * 0.18 + congestion * 0.12,
            _ => role_bonus + length_bias + congestion * 0.12,
        }
    }

    fn entity_route_cost(
        &self,
        route_id: usize,
        kind: &str,
        route_simulation: &[RouteSimulationRecord],
    ) -> f32 {
        let Some(edge) = self.transit_edges.get(route_id) else {
            return 9999.0;
        };
        let simulation = route_simulation.get(route_id);
        let hazard_pressure = self
            .hazard_zones
            .iter()
            .filter(|hazard| hazard.route_ids.contains(&route_id))
            .map(|hazard| hazard.severity)
            .sum::<f32>()
            .clamp(0.0, 1.0);
        let outage_pressure = self
            .resource_networks()
            .iter()
            .filter(|network| network.outage && network.route_ids.contains(&route_id))
            .count() as f32
            * 0.18;
        let congestion = simulation
            .map(|sim| sim.market_congestion.max(sim.civilian_density))
            .unwrap_or(0.0);
        let security = simulation.map(|sim| sim.security_pressure).unwrap_or(0.0);
        let route_fit = self.entity_route_score(route_id, kind, route_simulation);
        let role_penalty = match (kind, edge.role.as_str()) {
            ("corp_patrol", "restricted_spine") => -0.25,
            ("evacuee_flow", "evacuation_route") => -0.30,
            ("market_crowd", "market_run") => -0.25,
            ("maintenance_crawler", "maintenance_backbone") => -0.25,
            ("scavenger_drift", "service_loop") => -0.18,
            ("builder_swarm", "maintenance_backbone" | "primary_artery") => -0.18,
            _ => 0.0,
        };
        let risk_factor = match kind {
            "corp_patrol" => security * -0.12 + hazard_pressure * 0.10,
            "scavenger_drift" => hazard_pressure * -0.08 + congestion * 0.12,
            "evacuee_flow" => hazard_pressure * 0.60 + security * 0.30,
            _ => hazard_pressure * 0.35 + security * 0.18,
        };
        (1.0 + edge.length as f32 / self.size.max(1) as f32
            + congestion * 0.42
            + risk_factor
            + outage_pressure
            - route_fit * 0.35
            + role_penalty)
            .clamp(0.05, 20.0)
    }

    fn average_entity_density_weight(&self, room_clusters: &[RoomClusterRecord]) -> f32 {
        if room_clusters.is_empty() {
            return 1.0;
        }
        let sum = room_clusters
            .iter()
            .map(|cluster| self.entity_rule_weight_at(cluster.anchor_position, "density", ""))
            .sum::<f32>();
        (sum / room_clusters.len() as f32).clamp(0.0, 4.0)
    }

    fn entity_rule_weight_at(&self, point: [usize; 3], metric: &str, kind: &str) -> f32 {
        let district = self.district_at(point[0].min(self.size - 1), point[2].min(self.size - 1));
        let stratum = self.stratum_at(point[1].min(self.layers - 1));
        let pack = self.rule_pack_for(district, stratum);
        match metric {
            "density" => pack.entity_density_weight,
            "layout" => pack.entity_layout_weight,
            "kind" => match kind {
                "corp_patrol" => pack.patrol_weight,
                "market_crowd" | "evacuee_flow" | "scavenger_drift" => pack.crowd_weight,
                "builder_swarm" | "maintenance_crawler" => pack.builder_weight,
                _ => 1.0,
            },
            _ => 1.0,
        }
    }

    fn entity_sample_points(
        &self,
        origin: [usize; 3],
        destination: [usize; 3],
        route_ids: &[usize],
    ) -> Vec<[usize; 3]> {
        let mut samples = vec![origin];
        for route_id in route_ids {
            let Some(edge) = self.transit_edges.get(*route_id) else {
                continue;
            };
            let stride = (edge.points.len() / 6).max(1);
            for point in edge.points.iter().step_by(stride).take(8) {
                if samples.last() != Some(point) {
                    samples.push(*point);
                }
            }
        }
        if samples.last().copied() != Some(destination) {
            samples.push(destination);
        }
        samples
    }

    fn build_entity_pressure_fields(&mut self) {
        let mut grouped: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for entity in &self.entities {
            grouped
                .entry(pressure_kind_for_entity(&entity.kind).to_owned())
                .or_default()
                .push(entity.id);
        }

        for (kind, entity_ids) in grouped {
            let mut bounds_min = [usize::MAX; 3];
            let mut bounds_max = [0usize; 3];
            let mut affected_route_ids = BTreeSet::new();
            let mut affected_room_ids = BTreeSet::new();
            let mut path_count = 0usize;
            let mut risk_sum = 0.0f32;
            let mut congestion_sum = 0.0f32;
            for entity_id in &entity_ids {
                let Some(path) = self
                    .entity_paths
                    .iter()
                    .find(|path| path.entity_id == *entity_id)
                else {
                    continue;
                };
                path_count += 1;
                risk_sum += path.risk;
                congestion_sum += path.congestion;
                for point in &path.sample_points {
                    for axis in 0..3 {
                        bounds_min[axis] = bounds_min[axis].min(point[axis]);
                        bounds_max[axis] = bounds_max[axis].max(point[axis]);
                    }
                }
                for route_id in &path.route_ids {
                    affected_route_ids.insert(*route_id);
                }
            }
            if path_count == 0 {
                continue;
            }
            for room in &self.rooms {
                if room.position[0] >= bounds_min[0]
                    && room.position[0] <= bounds_max[0]
                    && room.position[1] >= bounds_min[1].saturating_sub(1)
                    && room.position[1] <= (bounds_max[1] + 1).min(self.layers - 1)
                    && room.position[2] >= bounds_min[2]
                    && room.position[2] <= bounds_max[2]
                {
                    affected_room_ids.insert(room.id);
                }
            }
            let intensity = ((path_count as f32 / 8.0)
                + risk_sum / path_count as f32 * 0.25
                + congestion_sum / path_count as f32 * 0.25)
                .clamp(0.05, 1.0);
            self.entity_pressure_fields.push(EntityPressureFieldRecord {
                id: self.entity_pressure_fields.len(),
                kind,
                bounds_min,
                bounds_max,
                intensity,
                source_entity_ids: entity_ids,
                affected_route_ids: affected_route_ids.into_iter().collect(),
                affected_room_ids: affected_room_ids.into_iter().collect(),
            });
        }
    }

    fn apply_entity_layout_mutations(&mut self, temporal_state: &TemporalStateRecord) {
        if self.config.entity_layout_pressure <= 0.0 {
            return;
        }
        let fields = self.entity_pressure_fields.clone();
        for field in fields {
            let limit = (field.intensity
                * self.config.entity_layout_pressure
                * self.entity_layout_weight_for_field(&field)
                * self.config.advanced_pattern_complexity.max(0.4)
                * 8.0)
                .round()
                .clamp(1.0, 18.0) as usize;
            let sample_points = self.pressure_mutation_points(&field, limit);
            if sample_points.is_empty() {
                continue;
            }
            let mut added = 0usize;
            let mut removed = 0usize;
            let mut bounds_min = [usize::MAX; 3];
            let mut bounds_max = [0usize; 3];
            for point in &sample_points {
                for axis in 0..3 {
                    bounds_min[axis] = bounds_min[axis].min(point[axis]);
                    bounds_max[axis] = bounds_max[axis].max(point[axis]);
                }
                let (a, r) = self.apply_entity_mutation_point(*point, &field.kind);
                added += a;
                removed += r;
            }
            if added == 0 && removed == 0 {
                continue;
            }
            let phase_id = phase_for_pressure(&field.kind, temporal_state);
            let mutation_kind = layout_mutation_kind(&field.kind).to_owned();
            self.push_connection(&mutation_kind, bounds_min, bounds_max);
            self.layout_mutations.push(LayoutMutationRecord {
                id: self.layout_mutations.len(),
                kind: mutation_kind.clone(),
                phase_id,
                bounds_min,
                bounds_max,
                source_pressure_field_id: field.id,
                affected_route_ids: field.affected_route_ids.clone(),
                affected_room_ids: field.affected_room_ids.clone(),
                added_cell_count: added,
                removed_cell_count: removed,
                sample_points,
                reason: format!("{} altered traversal and service geometry", field.kind),
            });
            self.record_pattern(&mutation_kind);
        }
    }

    fn entity_layout_weight_for_field(&self, field: &EntityPressureFieldRecord) -> f32 {
        let center = [
            (field.bounds_min[0] + field.bounds_max[0]) / 2,
            (field.bounds_min[1] + field.bounds_max[1]) / 2,
            (field.bounds_min[2] + field.bounds_max[2]) / 2,
        ];
        self.entity_rule_weight_at(center, "layout", "")
            .clamp(0.0, 4.0)
    }

    fn pressure_mutation_points(
        &self,
        field: &EntityPressureFieldRecord,
        limit: usize,
    ) -> Vec<[usize; 3]> {
        let mut points = Vec::new();
        for route_id in &field.affected_route_ids {
            let Some(edge) = self.transit_edges.get(*route_id) else {
                continue;
            };
            let stride = (edge.points.len() / limit.max(1)).max(1);
            for point in edge.points.iter().step_by(stride) {
                if point[0] >= field.bounds_min[0]
                    && point[0] <= field.bounds_max[0]
                    && point[2] >= field.bounds_min[2]
                    && point[2] <= field.bounds_max[2]
                {
                    points.push(*point);
                    if points.len() >= limit {
                        return points;
                    }
                }
            }
        }
        points
    }

    fn apply_entity_mutation_point(
        &mut self,
        point: [usize; 3],
        pressure_kind: &str,
    ) -> (usize, usize) {
        let [x, y, z] = [
            point[0].min(self.size - 1),
            point[1].min(self.layers - 1),
            point[2].min(self.size - 1),
        ];
        let mut added = 0usize;
        let mut removed = 0usize;
        match pressure_kind {
            "market_surge" => {
                for (dx, dz) in [(0isize, 0isize), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if let Some((nx, nz)) = self.offset_xz(x, z, dx, dz) {
                        let (a, r) = self.set_mutation_cell(nx, nz, y, CellType::Facade, false);
                        added += a;
                        removed += r;
                    }
                }
            }
            "patrol_lockdown" => {
                let (a, r) = self.set_mutation_cell(x, z, y, CellType::Stair, true);
                added += a;
                removed += r;
                if y + 1 < self.layers {
                    let (a, r) = self.set_mutation_cell(x, z, y + 1, CellType::Facade, false);
                    added += a;
                    removed += r;
                }
            }
            "evacuation_flow" => {
                for (dx, dz) in [(0isize, 0isize), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if let Some((nx, nz)) = self.offset_xz(x, z, dx, dz) {
                        let (a, r) = self.set_mutation_cell(nx, nz, y, CellType::Horizontal, true);
                        added += a;
                        removed += r;
                        if y + 1 < self.layers
                            && is_traversal_carveable_cell(self.get(nx, nz, y + 1))
                        {
                            let index = self.idx(nx, nz, y + 1);
                            self.grid[index] = CellType::Empty;
                            self.support_map[index] = false;
                            removed += 1;
                        }
                    }
                }
            }
            "maintenance_crawler" => {
                let (a, r) = self.set_mutation_cell(x, z, y, CellType::Pipe, false);
                added += a;
                removed += r;
                if y > 0 {
                    let (a, r) = self.set_mutation_cell(x, z, y - 1, CellType::Vent, false);
                    added += a;
                    removed += r;
                }
            }
            "builder_swarm" => {
                let (a, r) = self.set_mutation_cell(x, z, y, CellType::Vertical, true);
                added += a;
                removed += r;
                if y + 1 < self.layers {
                    let (a, r) = self.set_mutation_cell(x, z, y + 1, CellType::Cable, false);
                    added += a;
                    removed += r;
                }
            }
            _ => {
                let (a, r) = self.set_mutation_cell(x, z, y, CellType::Debris, false);
                added += a;
                removed += r;
            }
        }
        (added, removed)
    }

    fn set_mutation_cell(
        &mut self,
        x: usize,
        z: usize,
        y: usize,
        cell: CellType,
        supported: bool,
    ) -> (usize, usize) {
        let previous = self.get(x, z, y);
        if previous == cell {
            return (0, 0);
        }
        self.set(x, z, y, cell, supported);
        (
            usize::from(previous == CellType::Empty && cell != CellType::Empty),
            usize::from(previous != CellType::Empty && cell == CellType::Empty),
        )
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
            let occupied_ratio = occupied[index] as f32 / total as f32;
            let lifecycle = self.lifecycle_for_district(district, occupied_ratio);
            DistrictRecord {
                id,
                kind: district.name().to_owned(),
                bounds_min: min_bounds[index],
                bounds_max: max_bounds[index],
                footprint_cells: footprint[index],
                occupied_cells: occupied[index],
                occupied_ratio,
                age_years: lifecycle.age_years,
                maintenance_level: lifecycle.maintenance_level,
                occupancy_pressure: lifecycle.occupancy_pressure,
                control_stability: lifecycle.control_stability,
                dominant_grammar: district_grammar(district).to_owned(),
                generated_features: district_feature_names(district, &self.pattern_counts),
            }
        })
        .collect()
    }

    fn district_lifecycle_records(
        &self,
        districts: &[DistrictRecord],
    ) -> Vec<DistrictLifecycleRecord> {
        districts
            .iter()
            .map(|district| {
                let district_type = district_type_from_name(&district.kind);
                self.lifecycle_for_district(district_type, district.occupied_ratio)
            })
            .collect()
    }

    fn lifecycle_for_district(
        &self,
        district: DistrictType,
        occupied_ratio: f32,
    ) -> DistrictLifecycleRecord {
        let index = district as usize;
        let roll = hash_noise(self.seed_hash, index + 17, self.size, self.layers);
        let (base_age, base_maintenance, base_control, base_occupancy) = match district {
            DistrictType::Industrial => (72.0, 0.46, 0.55, 0.58),
            DistrictType::Residential => (44.0, 0.62, 0.64, 0.68),
            DistrictType::Commercial => (31.0, 0.70, 0.58, 0.74),
            DistrictType::Slum => (96.0, 0.24, 0.32, 0.92),
            DistrictType::Elite => (18.0, 0.86, 0.82, 0.42),
        };
        let age_years = (base_age + roll * 80.0).round() as usize;
        let maintenance_level = (base_maintenance + hash_noise(self.seed_hash, index, 7, 1) * 0.22
            - occupied_ratio * 0.12)
            .clamp(0.05, 1.0);
        let occupancy_pressure = (base_occupancy
            + occupied_ratio * 0.55
            + hash_noise(self.seed_hash, index, 11, 2) * 0.16)
            .clamp(0.0, 1.0);
        let control_stability = (base_control + maintenance_level * 0.22
            - occupancy_pressure * 0.18
            + hash_noise(self.seed_hash, index, 13, 3) * 0.10)
            .clamp(0.0, 1.0);
        let normalized_age = (age_years as f32 / 160.0).clamp(0.0, 1.0);
        let decay_bias =
            (0.65 + normalized_age * 0.65 + (1.0 - maintenance_level) * 0.65).clamp(0.25, 2.2);
        let repair_bias =
            (0.35 + maintenance_level * 0.85 + control_stability * 0.25).clamp(0.1, 1.6);
        let security_bias =
            (0.30 + control_stability * 0.75 + maintenance_level * 0.25).clamp(0.1, 1.6);
        let density_bias =
            (0.55 + occupancy_pressure * 0.75 + normalized_age * 0.15).clamp(0.35, 1.7);
        DistrictLifecycleRecord {
            district: district.name().to_owned(),
            age_years,
            maintenance_level,
            occupancy_pressure,
            control_stability,
            decay_bias,
            repair_bias,
            security_bias,
            density_bias,
        }
    }

    fn rule_pack_records(&self) -> Vec<RulePackRecord> {
        let mut records = Vec::new();
        for district in [
            DistrictType::Industrial,
            DistrictType::Residential,
            DistrictType::Commercial,
            DistrictType::Slum,
            DistrictType::Elite,
        ] {
            for stratum in [
                BiomeStratum::Underground,
                BiomeStratum::Surface,
                BiomeStratum::Midrise,
                BiomeStratum::Skyline,
            ] {
                let mut record = self.rule_pack_for(district, stratum);
                record.id = records.len();
                records.push(record);
            }
        }
        records
    }

    fn rule_pack_for(&self, district: DistrictType, stratum: BiomeStratum) -> RulePackRecord {
        if let Some(pack) = self.rule_packs.find(
            self.config.profile,
            Some(self.config.typology),
            district.name(),
            stratum.name(),
        ) {
            return RulePackRecord {
                id: 0,
                name: pack.name.clone(),
                typology: pack.typology.map(|typology| typology.as_str().to_owned()),
                district: pack.district.clone(),
                stratum: pack.stratum.clone(),
                profile: pack.profile.to_string(),
                density_weight: pack.density_weight,
                route_weight: pack.route_weight,
                decay_weight: pack.decay_weight,
                detail_weight: pack.detail_weight.clamp(0.1, 1.5),
                entity_density_weight: pack.entity_density_weight,
                entity_layout_weight: pack.entity_layout_weight,
                patrol_weight: pack.patrol_weight,
                crowd_weight: pack.crowd_weight,
                builder_weight: pack.builder_weight,
            };
        }
        let mut density_weight: f32 = match district {
            DistrictType::Slum => 1.35,
            DistrictType::Commercial => 1.18,
            DistrictType::Industrial => 1.05,
            DistrictType::Residential => 1.0,
            DistrictType::Elite => 0.72,
        };
        let mut route_weight: f32 = match stratum {
            BiomeStratum::Underground => 1.22,
            BiomeStratum::Surface => 1.10,
            BiomeStratum::Midrise => 1.0,
            BiomeStratum::Skyline => 0.86,
        };
        let mut decay_weight: f32 = match district {
            DistrictType::Slum | DistrictType::Industrial => 1.24,
            DistrictType::Elite => 0.62,
            _ => 1.0,
        };
        let mut detail_weight: f32 = match self.config.profile {
            crate::config::GenerationProfile::Dense => 0.86,
            crate::config::GenerationProfile::Decayed => 0.78,
            crate::config::GenerationProfile::Neon => 0.92,
            _ => 0.70,
        };
        if stratum == BiomeStratum::Skyline {
            detail_weight += 0.08;
            route_weight *= 0.92;
        }
        if self.config.profile == crate::config::GenerationProfile::Decayed {
            decay_weight *= 1.25;
            density_weight *= 0.95;
        }
        RulePackRecord {
            id: 0,
            name: format!(
                "{}_{}_{}",
                self.config.profile,
                district.name().to_lowercase(),
                stratum.name().to_lowercase()
            ),
            typology: Some(self.config.typology.as_str().to_owned()),
            district: district.name().to_owned(),
            stratum: stratum.name().to_owned(),
            profile: self.config.profile.to_string(),
            density_weight,
            route_weight,
            decay_weight,
            detail_weight: detail_weight.clamp(0.1, 1.0),
            entity_density_weight: 1.0,
            entity_layout_weight: 1.0,
            patrol_weight: 1.0,
            crowd_weight: 1.0,
            builder_weight: 1.0,
        }
    }

    fn rule_pack_grammar(&self, rule_pack: &RulePackRecord) -> Vec<String> {
        self.rule_packs
            .find(
                self.config.profile,
                Some(self.config.typology),
                &rule_pack.district,
                &rule_pack.stratum,
            )
            .map(|pack| pack.grammar.clone())
            .unwrap_or_default()
    }

    fn rule_influence_records(
        &self,
        rule_packs: &[RulePackRecord],
        districts: &[DistrictRecord],
        room_clusters: &[RoomClusterRecord],
        hazards: &[HazardZoneRecord],
        landmarks: &[NarrativeLandmarkRecord],
    ) -> Vec<RuleInfluenceRecord> {
        let mut influences = Vec::new();
        for district in districts {
            if let Some(influence) = self.rule_influence_for(
                "district",
                district.id.to_string(),
                self.district_from_name(&district.kind),
                BiomeStratum::Surface,
                rule_packs,
                "district dominant grammar",
            ) {
                influences.push(influence);
            }
        }
        for edge in &self.transit_edges {
            if let Some(point) = edge.points.get(edge.points.len() / 2) {
                if let Some(influence) = self.rule_influence_for(
                    "route",
                    edge.id.to_string(),
                    self.district_at(point[0].min(self.size - 1), point[2].min(self.size - 1)),
                    self.stratum_at(point[1].min(self.layers - 1)),
                    rule_packs,
                    "route role and density",
                ) {
                    influences.push(influence);
                }
            }
        }
        for cluster in room_clusters {
            let anchor = cluster.anchor_position;
            if let Some(influence) = self.rule_influence_for(
                "cluster",
                cluster.id.to_string(),
                self.district_at(anchor[0].min(self.size - 1), anchor[2].min(self.size - 1)),
                self.stratum_at(anchor[1].min(self.layers - 1)),
                rule_packs,
                "room cluster semantics",
            ) {
                influences.push(influence);
            }
        }
        for hazard in hazards {
            if let Some(influence) = self.rule_influence_for(
                "hazard",
                hazard.id.to_string(),
                self.district_at(
                    hazard.bounds_min[0].min(self.size - 1),
                    hazard.bounds_min[2].min(self.size - 1),
                ),
                self.stratum_at(hazard.bounds_min[1].min(self.layers - 1)),
                rule_packs,
                "hazard and decay pressure",
            ) {
                influences.push(influence);
            }
        }
        for landmark in landmarks {
            if let Some(influence) = self.rule_influence_for(
                "landmark",
                landmark.id.to_string(),
                self.district_at(
                    landmark.position[0].min(self.size - 1),
                    landmark.position[2].min(self.size - 1),
                ),
                self.stratum_at(landmark.position[1].min(self.layers - 1)),
                rule_packs,
                "landmark grammar",
            ) {
                influences.push(influence);
            }
        }
        for (id, influence) in influences.iter_mut().enumerate() {
            influence.id = id;
        }
        influences
    }

    fn rule_influence_for(
        &self,
        target_type: &str,
        target_id: String,
        district: DistrictType,
        stratum: BiomeStratum,
        rule_packs: &[RulePackRecord],
        reason: &str,
    ) -> Option<RuleInfluenceRecord> {
        let rule_pack = self.rule_pack_for(district, stratum);
        let exported = rule_packs.iter().find(|pack| {
            pack.name == rule_pack.name
                && pack.district == rule_pack.district
                && pack.stratum == rule_pack.stratum
        })?;
        Some(RuleInfluenceRecord {
            id: 0,
            target_type: target_type.to_owned(),
            target_id,
            rule_pack_id: exported.id,
            rule_pack_name: exported.name.clone(),
            district: exported.district.clone(),
            stratum: exported.stratum.clone(),
            grammar: self.rule_pack_grammar(exported),
            reason: reason.to_owned(),
        })
    }

    fn district_from_name(&self, name: &str) -> DistrictType {
        match name {
            "INDUSTRIAL" => DistrictType::Industrial,
            "COMMERCIAL" => DistrictType::Commercial,
            "SLUM" => DistrictType::Slum,
            "ELITE" => DistrictType::Elite,
            _ => DistrictType::Residential,
        }
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

    fn macro_massing_records(&self) -> Vec<MacroMassingRecord> {
        self.connections
            .iter()
            .filter(|connection| {
                matches!(
                    connection.kind.as_str(),
                    "macro_void" | "macro_density_spine"
                ) || connection.kind.starts_with("typology_")
            })
            .enumerate()
            .map(|(id, connection)| {
                let district = self.district_at(connection.start[0], connection.start[2]);
                let height = connection.start[1].abs_diff(connection.end[1]).max(1);
                MacroMassingRecord {
                    id,
                    kind: connection.kind.clone(),
                    bounds_min: [
                        connection.start[0].saturating_sub(1),
                        connection.start[1].min(connection.end[1]),
                        connection.start[2].saturating_sub(1),
                    ],
                    bounds_max: [
                        (connection.start[0] + 1).min(self.size - 1),
                        connection.start[1].max(connection.end[1]),
                        (connection.start[2] + 1).min(self.size - 1),
                    ],
                    district: district.name().to_owned(),
                    void_ratio: if connection.kind == "macro_void" {
                        (height as f32 / self.layers.max(1) as f32).clamp(0.0, 1.0)
                    } else {
                        0.0
                    },
                }
            })
            .collect()
    }

    fn meso_placement_records(
        &self,
        room_clusters: &[RoomClusterRecord],
    ) -> Vec<MesoPlacementRecord> {
        let mut records = Vec::new();
        for cluster in room_clusters.iter().take(64) {
            records.push(MesoPlacementRecord {
                id: records.len(),
                kind: format!("cluster_{}", cluster.kind),
                route_id: cluster.route_ids.first().copied(),
                cluster_id: Some(cluster.id),
                anchor: cluster.anchor_position,
                influence_radius: cluster
                    .bounds_max
                    .iter()
                    .zip(cluster.bounds_min.iter())
                    .map(|(max, min)| max.saturating_sub(*min))
                    .max()
                    .unwrap_or(1)
                    .max(1),
            });
        }
        for edge in self.transit_edges.iter().take(64) {
            records.push(MesoPlacementRecord {
                id: records.len(),
                kind: format!("route_{}", edge.role),
                route_id: Some(edge.id),
                cluster_id: None,
                anchor: edge
                    .points
                    .get(edge.points.len() / 2)
                    .copied()
                    .unwrap_or([0, 0, 0]),
                influence_radius: (edge.length / 6).clamp(1, 12),
            });
        }
        records
    }

    fn micro_detail_records(&self) -> Vec<MicroDetailRecord> {
        let mut records = Vec::new();
        for edge in &self.transit_edges {
            let stride = (edge.points.len() / 4).max(2);
            for (index, point) in edge.points.iter().enumerate().step_by(stride).take(4) {
                records.push(MicroDetailRecord {
                    id: records.len(),
                    kind: micro_detail_kind(edge, index).to_owned(),
                    position: *point,
                    route_id: Some(edge.id),
                    intensity: (0.35 + edge.length as f32 / 80.0).clamp(0.0, 1.0),
                });
            }
        }
        records
    }

    fn failure_propagation_records(&self) -> Vec<FailurePropagationRecord> {
        let mut records = Vec::new();
        for connection in self
            .connections
            .iter()
            .filter(|connection| connection.kind == "collapse_scar")
        {
            let origin = connection.start;
            let radius = (3.0 + self.config.erosion_strength * 2.0).round() as usize;
            let affected_route_ids = self.nearby_route_ids(origin, radius + 3);
            let affected_deck_count = self
                .transit_edges
                .iter()
                .flat_map(|edge| edge.points.iter())
                .filter(|point| {
                    point[0].abs_diff(origin[0])
                        + point[1].abs_diff(origin[1])
                        + point[2].abs_diff(origin[2])
                        <= radius
                })
                .count();
            records.push(FailurePropagationRecord {
                id: records.len(),
                origin,
                radius,
                severity: (0.35
                    + self.config.erosion_strength * 0.18
                    + affected_route_ids.len() as f32 * 0.04)
                    .clamp(0.0, 1.0),
                affected_route_ids,
                affected_deck_count,
            });
        }
        for hazard in self
            .hazard_zones
            .iter()
            .filter(|hazard| hazard.kind == "unstable_span")
        {
            let origin = [
                (hazard.bounds_min[0] + hazard.bounds_max[0]) / 2,
                (hazard.bounds_min[1] + hazard.bounds_max[1]) / 2,
                (hazard.bounds_min[2] + hazard.bounds_max[2]) / 2,
            ];
            records.push(FailurePropagationRecord {
                id: records.len(),
                origin,
                radius: 4,
                severity: hazard.severity,
                affected_route_ids: hazard.route_ids.clone(),
                affected_deck_count: hazard.room_ids.len(),
            });
        }
        records
    }

    fn failure_pressure_at(&self, point: [usize; 3]) -> f32 {
        self.connections
            .iter()
            .filter(|connection| connection.kind == "collapse_scar")
            .map(|connection| {
                let distance = connection.start[0].abs_diff(point[0])
                    + connection.start[1].abs_diff(point[1])
                    + connection.start[2].abs_diff(point[2]);
                (1.0 - distance as f32 / 10.0).clamp(0.0, 1.0)
            })
            .fold(0.0, f32::max)
    }

    fn room_clusters(&self) -> (Vec<RoomClusterRecord>, Vec<Option<usize>>) {
        #[derive(Default)]
        struct ClusterBuilder {
            kind: String,
            owner_district: String,
            stratum: String,
            room_ids: Vec<usize>,
            route_ids: BTreeSet<usize>,
            bounds_min: [usize; 3],
            bounds_max: [usize; 3],
            anchor_position: [usize; 3],
        }

        let mut route_by_room = BTreeMap::new();
        for attachment in &self.transit_attachments {
            route_by_room.insert(attachment.room_id, attachment.route_id);
        }

        let mut builders: BTreeMap<(String, String, String, usize), ClusterBuilder> =
            BTreeMap::new();
        let mut room_cluster_ids = vec![None; self.rooms.len()];
        for room in &self.rooms {
            let route_id = route_by_room.get(&room.id).copied().unwrap_or(usize::MAX);
            let route_bucket = if route_id == usize::MAX {
                usize::MAX
            } else {
                route_id / 2
            };
            let kind = cluster_kind_for_room(&room.label).to_owned();
            let stratum = self.stratum_at(room.position[1]).name().to_owned();
            let key = (
                kind.clone(),
                room.district.clone(),
                stratum.clone(),
                route_bucket,
            );
            let entry = builders.entry(key).or_insert_with(|| ClusterBuilder {
                kind,
                owner_district: room.district.clone(),
                stratum,
                room_ids: Vec::new(),
                route_ids: BTreeSet::new(),
                bounds_min: room.position,
                bounds_max: room.position,
                anchor_position: room.position,
            });
            entry.room_ids.push(room.id);
            if route_id != usize::MAX {
                entry.route_ids.insert(route_id);
            }
            for axis in 0..3 {
                entry.bounds_min[axis] = entry.bounds_min[axis].min(room.position[axis]);
                entry.bounds_max[axis] = entry.bounds_max[axis].max(room.position[axis]);
            }
        }

        let clusters: Vec<_> = builders
            .into_values()
            .enumerate()
            .map(|(id, builder)| {
                for room_id in &builder.room_ids {
                    room_cluster_ids[*room_id] = Some(id);
                }
                RoomClusterRecord {
                    id,
                    kind: builder.kind,
                    owner_district: builder.owner_district,
                    stratum: builder.stratum,
                    bounds_min: builder.bounds_min,
                    bounds_max: builder.bounds_max,
                    anchor_position: builder.anchor_position,
                    room_ids: builder.room_ids,
                    route_ids: builder.route_ids.into_iter().collect(),
                }
            })
            .collect();

        (clusters, room_cluster_ids)
    }

    fn path_analysis(&self) -> PathAnalysisRecord {
        let edge_count = self.transit_edges.len();
        if edge_count == 0 || self.transit_nodes.is_empty() {
            return PathAnalysisRecord {
                connected_component_count: 0,
                largest_component_edges: 0,
                dead_end_count: 0,
                chokepoint_count: 0,
                reachable_room_count: 0,
                alternate_path_count: 0,
                vertical_transfer_count: 0,
                guaranteed_service_to_skyline: false,
                route_redundancy_score: 0.0,
                reachable_landmark_count: 0,
                faction_territory_connectivity: 0.0,
                main_path_room_reachability: 0.0,
                quality_score: 0.0,
                high_centrality_route_ids: Vec::new(),
                main_path: None,
            };
        }

        let mut edge_parent: Vec<_> = (0..edge_count).collect();
        let mut point_owner = BTreeMap::new();
        for edge in &self.transit_edges {
            for point in &edge.points {
                if let Some(previous_edge) = point_owner.insert(*point, edge.id) {
                    union(&mut edge_parent, previous_edge, edge.id);
                }
            }
        }

        let mut component_edges = BTreeMap::new();
        for edge in &self.transit_edges {
            let component = find_root(&mut edge_parent, edge.id);
            *component_edges.entry(component).or_insert(0usize) += 1;
        }
        let connected_component_count = component_edges.len();
        let largest_component_edges = component_edges.values().copied().max().unwrap_or_default();

        let mut room_ids = BTreeSet::new();
        let mut attachment_counts = vec![0usize; edge_count];
        for attachment in &self.transit_attachments {
            room_ids.insert(attachment.room_id);
            if let Some(count) = attachment_counts.get_mut(attachment.route_id) {
                *count += 1;
            }
        }
        let mut degree_counts = vec![0usize; self.transit_nodes.len()];
        for edge in &self.transit_edges {
            degree_counts[edge.start_node] += 1;
            degree_counts[edge.end_node] += 1;
        }

        let mut scored_routes: Vec<_> = self
            .transit_edges
            .iter()
            .map(|edge| {
                let score = attachment_counts[edge.id]
                    + degree_counts[edge.start_node]
                    + degree_counts[edge.end_node]
                    + usize::from(edge.role == "primary_artery") * 3
                    + usize::from(edge.role == "restricted_spine") * 2;
                (score, edge.id)
            })
            .collect();
        scored_routes.sort_by(|a, b| b.cmp(a));
        let high_centrality_route_ids = scored_routes
            .into_iter()
            .take(5)
            .map(|(_, route_id)| route_id)
            .collect();

        PathAnalysisRecord {
            connected_component_count,
            largest_component_edges,
            dead_end_count: self
                .transit_nodes
                .iter()
                .filter(|node| node.kind == "dead_end")
                .count(),
            chokepoint_count: self
                .rooms
                .iter()
                .filter(|room| room.label == "ROUTE_CHOKEPOINT")
                .count(),
            reachable_room_count: room_ids.len(),
            alternate_path_count: self
                .transit_edges
                .iter()
                .filter(|edge| edge.kind == "ring_route")
                .count(),
            vertical_transfer_count: self
                .transit_edges
                .iter()
                .filter(|edge| edge.kind.contains("vertical_transfer"))
                .count(),
            guaranteed_service_to_skyline: self.has_service_to_skyline_path(),
            route_redundancy_score: 0.0,
            reachable_landmark_count: 0,
            faction_territory_connectivity: 0.0,
            main_path_room_reachability: 0.0,
            quality_score: 0.0,
            high_centrality_route_ids,
            main_path: self.main_mission_path(),
        }
    }

    fn finalize_topology_quality(
        &self,
        mut analysis: PathAnalysisRecord,
        rooms: &[RoomRecord],
        territories: &[TerritoryRecord],
        landmarks: &[NarrativeLandmarkRecord],
        failure_zones: &[FailurePropagationRecord],
    ) -> PathAnalysisRecord {
        let valid_route_ids: BTreeSet<_> = self.transit_edges.iter().map(|edge| edge.id).collect();
        analysis.reachable_landmark_count = landmarks
            .iter()
            .filter(|landmark| {
                landmark
                    .route_id
                    .is_some_and(|route_id| valid_route_ids.contains(&route_id))
            })
            .count();

        let routed_territories = territories
            .iter()
            .filter(|territory| !territory.route_ids.is_empty())
            .count();
        analysis.faction_territory_connectivity = if territories.is_empty() {
            0.0
        } else {
            routed_territories as f32 / territories.len() as f32
        };

        analysis.main_path_room_reachability = analysis
            .main_path
            .as_ref()
            .map(|path| {
                let valid_room_count = path
                    .room_ids
                    .iter()
                    .filter(|room_id| **room_id < rooms.len())
                    .count();
                if path.room_ids.is_empty() {
                    0.0
                } else {
                    valid_room_count as f32 / path.room_ids.len() as f32
                }
            })
            .unwrap_or(0.0);

        analysis.route_redundancy_score = (analysis.alternate_path_count as f32 / 3.0)
            .min(1.0)
            .min(analysis.largest_component_edges as f32 / self.transit_edges.len().max(1) as f32);
        let component_score =
            analysis.largest_component_edges as f32 / self.transit_edges.len().max(1) as f32;
        let vertical_score = (analysis.vertical_transfer_count as f32 / 3.0).min(1.0);
        let service_score = if analysis.guaranteed_service_to_skyline {
            1.0
        } else {
            0.0
        };
        let landmark_score = (analysis.reachable_landmark_count as f32 / 8.0).min(1.0);
        analysis.quality_score = ((component_score
            + analysis.route_redundancy_score
            + vertical_score
            + service_score
            + landmark_score
            + analysis.faction_territory_connectivity
            + analysis.main_path_room_reachability)
            / 7.0)
            .clamp(0.0, 1.0);
        let failure_penalty = (failure_zones
            .iter()
            .map(|failure| failure.severity)
            .sum::<f32>()
            * 0.015)
            .clamp(0.0, 0.18);
        analysis.quality_score = (analysis.quality_score - failure_penalty).clamp(0.0, 1.0);
        analysis
    }

    fn has_service_to_skyline_path(&self) -> bool {
        self.transit_edges
            .iter()
            .any(|edge| edge.kind == "service_tunnel")
            && self
                .transit_edges
                .iter()
                .any(|edge| edge.kind == "skybridge" && edge.stratum == "SKYLINE")
            && self
                .transit_edges
                .iter()
                .any(|edge| edge.kind == "mission_vertical_transfer")
    }

    fn main_mission_path(&self) -> Option<MissionPathRecord> {
        let start = self
            .transit_edges
            .iter()
            .find(|edge| edge.role == "maintenance_backbone")
            .or_else(|| self.transit_edges.first())?;
        let end = self
            .transit_edges
            .iter()
            .rev()
            .find(|edge| matches!(edge.role.as_str(), "restricted_spine" | "primary_artery"))
            .unwrap_or(start);
        let route_ids: Vec<_> = self
            .transit_edges
            .iter()
            .filter(|edge| {
                edge.id == start.id
                    || edge.id == end.id
                    || matches!(edge.role.as_str(), "primary_artery" | "restricted_spine")
                    || matches!(
                        edge.kind.as_str(),
                        "ring_route" | "vertical_transfer" | "mission_vertical_transfer"
                    )
            })
            .take(12)
            .map(|edge| edge.id)
            .collect();
        let route_set: BTreeSet<_> = route_ids.iter().copied().collect();
        let room_ids = self
            .transit_attachments
            .iter()
            .filter(|attachment| route_set.contains(&attachment.route_id))
            .map(|attachment| attachment.room_id)
            .collect();
        Some(MissionPathRecord {
            label: "service tunnel to skyline vault route".to_owned(),
            route_ids,
            room_ids,
            start: start.points.first().copied().unwrap_or([0, 0, 0]),
            end: end.points.last().copied().unwrap_or([0, 0, 0]),
        })
    }

    fn structural_system(&self) -> StructuralSystemRecord {
        let mut frames = Vec::new();
        let mut foundations = Vec::new();
        let mut suspended_decks = Vec::new();
        let mut dependency_summary = BTreeMap::new();
        let failure_zones = self.failure_propagation_records();

        for x in 0..self.size {
            for z in 0..self.size {
                let mut vertical_run = 0usize;
                for y in 0..self.layers {
                    match self.get(x, z, y) {
                        CellType::Vertical | CellType::Elevator | CellType::Stair => {
                            vertical_run += 1;
                            if vertical_run >= 4 && y % 3 == 0 {
                                frames.push([x, y, z]);
                            }
                        }
                        CellType::Horizontal | CellType::Bridge
                            if y > 2 && !self.has_support(x, z, y) =>
                        {
                            suspended_decks.push([x, y, z]);
                        }
                        _ => vertical_run = 0,
                    }
                }
                if matches!(
                    self.get(x, z, 0),
                    CellType::Vertical
                        | CellType::Elevator
                        | CellType::Stair
                        | CellType::Horizontal
                ) {
                    foundations.push([x, z]);
                }
            }
        }

        dependency_summary.insert("foundation_zones".to_owned(), foundations.len());
        dependency_summary.insert("load_bearing_frames".to_owned(), frames.len());
        dependency_summary.insert("suspended_decks".to_owned(), suspended_decks.len());
        dependency_summary.insert("hazard_zones".to_owned(), self.hazard_zones.len());
        dependency_summary.insert("failure_zones".to_owned(), failure_zones.len());
        let stress_fields = self.stress_fields(&frames, &foundations, &suspended_decks);
        let load_paths = self.load_paths(&stress_fields);
        dependency_summary.insert("stress_fields".to_owned(), stress_fields.len());
        dependency_summary.insert("load_paths".to_owned(), load_paths.len());

        StructuralSystemRecord {
            stability_ratings: self.stability_ratings(
                &frames,
                &foundations,
                &suspended_decks,
                &failure_zones,
            ),
            load_bearing_frames: frames,
            foundation_zones: foundations,
            suspended_decks,
            support_dependency_summary: dependency_summary,
            stress_fields,
            load_paths,
        }
    }

    fn stress_fields(
        &self,
        frames: &[[usize; 3]],
        foundations: &[[usize; 2]],
        suspended_decks: &[[usize; 3]],
    ) -> Vec<StressFieldRecord> {
        let mut fields = Vec::new();
        for edge in self.transit_edges.iter().filter(|edge| {
            matches!(
                edge.kind.as_str(),
                "void_bridge"
                    | "marine_causeway"
                    | "pylon_service"
                    | "rim_loop"
                    | "spoke_transfer"
                    | "cliff_gallery"
                    | "dam_wall_spine"
                    | "drydock_spine"
                    | "runway_spine"
                    | "caldera_ring"
                    | "geothermal_shaft"
                    | "crevasse_bridge"
                    | "canopy_walk"
                    | "tether_core"
                    | "cargo_ring"
                    | "crawler_track"
                    | "reef_ring"
                    | "pressure_deck"
                    | "sinkhole_ring"
            )
        }) {
            let Some(anchor) = edge.points.get(edge.points.len() / 2).copied() else {
                continue;
            };
            let nearby_frames: Vec<_> = frames
                .iter()
                .copied()
                .filter(|point| manhattan(*point, anchor) <= 8)
                .take(8)
                .collect();
            let deck_pressure = suspended_decks
                .iter()
                .filter(|point| manhattan(**point, anchor) <= 6)
                .count() as f32
                / 12.0;
            let foundation_pressure = if foundations
                .iter()
                .any(|point| point[0].abs_diff(anchor[0]) + point[1].abs_diff(anchor[2]) <= 5)
            {
                0.0
            } else {
                0.25
            };
            let route_pressure = self.route_stress_score_for_edge(edge, anchor) * 0.42;
            let stress = (0.24 + deck_pressure + foundation_pressure + route_pressure
                - nearby_frames.len() as f32 * 0.025)
                .clamp(0.0, 1.0);
            let radius = 2usize;
            fields.push(StressFieldRecord {
                id: fields.len(),
                kind: stress_kind_for_route(&edge.kind).to_owned(),
                stress,
                bounds_min: [
                    anchor[0].saturating_sub(radius),
                    anchor[1].saturating_sub(1),
                    anchor[2].saturating_sub(radius),
                ],
                bounds_max: [
                    (anchor[0] + radius).min(self.size - 1),
                    (anchor[1] + 1).min(self.layers - 1),
                    (anchor[2] + radius).min(self.size - 1),
                ],
                route_ids: vec![edge.id],
                support_points: nearby_frames,
            });
        }
        if fields.is_empty() {
            let anchor = [self.size / 2, (self.layers / 2).max(1), self.size / 2];
            fields.push(StressFieldRecord {
                id: 0,
                kind: "baseline_frame_stress".to_owned(),
                stress: 0.25,
                bounds_min: anchor,
                bounds_max: anchor,
                route_ids: self
                    .transit_edges
                    .first()
                    .map(|edge| vec![edge.id])
                    .unwrap_or_default(),
                support_points: frames.iter().copied().take(4).collect(),
            });
        }
        fields
    }

    fn route_stress_score_for_edge(&self, edge: &TransitEdgeRecord, anchor: [usize; 3]) -> f32 {
        let min_x = anchor[0].saturating_sub(8);
        let max_x = (anchor[0] + 8).min(self.size - 1);
        let min_z = anchor[2].saturating_sub(8);
        let max_z = (anchor[2] + 8).min(self.size - 1);
        let min_y = anchor[1].saturating_sub(4);
        let max_y = (anchor[1] + 4).min(self.layers - 1);
        let mut nearby_frames = 0.0f32;
        let mut nearby_decks = 0.0f32;
        let mut foundation_nearby = false;
        for x in min_x..=max_x {
            for z in min_z..=max_z {
                if x.abs_diff(anchor[0]) + z.abs_diff(anchor[2]) <= 5
                    && matches!(
                        self.get(x, z, 0),
                        CellType::Vertical
                            | CellType::Elevator
                            | CellType::Stair
                            | CellType::Horizontal
                    )
                {
                    foundation_nearby = true;
                }
                for y in min_y..=max_y {
                    if manhattan([x, y, z], anchor) > 8 {
                        continue;
                    }
                    match self.get(x, z, y) {
                        CellType::Vertical | CellType::Elevator | CellType::Stair => {
                            nearby_frames += 1.0;
                        }
                        CellType::Horizontal | CellType::Bridge
                            if y > 2 && !self.has_support(x, z, y) =>
                        {
                            nearby_decks += 1.0;
                        }
                        _ => {}
                    }
                }
            }
        }
        let foundation_gap = if foundation_nearby { 0.0 } else { 0.22 };
        let route_span = (edge.length as f32 / self.size.max(1) as f32 * 0.30).min(0.32);
        let typology_pressure = match edge.kind.as_str() {
            "void_bridge" | "skybridge" => 0.28,
            "marine_causeway" | "pylon_service" => 0.24,
            "rim_loop" | "spoke_transfer" => 0.22,
            "dam_wall_spine" | "turbine_gallery" => 0.20,
            "drydock_spine" | "gantry_loop" => 0.20,
            "runway_spine" | "terminal_loop" => 0.18,
            "cavern_loop" | "hive_gallery" | "hive_trunk" => 0.18,
            "cliff_gallery" | "burrow_spine" => 0.18,
            "caldera_ring" | "geothermal_shaft" => 0.24,
            "crevasse_bridge" | "meltwater_spine" => 0.22,
            "canopy_walk" | "root_service" => 0.20,
            "tether_core" | "cargo_ring" | "ground_anchor" => 0.28,
            "crawler_track" | "engine_spine" | "convoy_deck" => 0.20,
            "reef_ring" | "lagoon_causeway" => 0.22,
            "pressure_deck" | "lift_cell_spine" => 0.26,
            "sinkhole_ring" | "descent_shaft" => 0.22,
            _ => 0.08,
        };
        let deck_pressure = (nearby_decks / 14.0f32).min(0.26);
        let support_relief = (nearby_frames * 0.026f32).min(0.22);
        (0.16
            + typology_pressure
            + route_span
            + deck_pressure
            + foundation_gap
            + self.config.erosion_strength * 0.08
            - support_relief)
            .clamp(0.0, 1.0)
    }

    fn load_paths(&self, stress_fields: &[StressFieldRecord]) -> Vec<LoadPathRecord> {
        stress_fields
            .iter()
            .take(12)
            .map(|field| {
                let from = field
                    .support_points
                    .first()
                    .copied()
                    .unwrap_or(field.bounds_min);
                LoadPathRecord {
                    id: field.id,
                    kind: format!("{}_load_path", field.kind),
                    from,
                    to: field.bounds_max,
                    route_ids: field.route_ids.clone(),
                    stress: field.stress,
                }
            })
            .collect()
    }

    fn construction_history_records(
        &self,
        districts: &[DistrictRecord],
        failure_zones: &[FailurePropagationRecord],
    ) -> Vec<ConstructionEraRecord> {
        let mut eras = Vec::new();
        let all_districts: Vec<_> = districts
            .iter()
            .map(|district| district.kind.clone())
            .collect();
        let service_routes: Vec<_> = self
            .transit_edges
            .iter()
            .filter(|edge| matches!(edge.role.as_str(), "maintenance_backbone" | "service_loop"))
            .map(|edge| edge.id)
            .take(8)
            .collect();
        let primary_routes: Vec<_> = self
            .transit_edges
            .iter()
            .filter(|edge| matches!(edge.role.as_str(), "primary_artery" | "evacuation_route"))
            .map(|edge| edge.id)
            .take(10)
            .collect();
        eras.push(ConstructionEraRecord {
            id: eras.len(),
            era: "foundation".to_owned(),
            age_years: 120,
            material_bias: "heavy concrete and service cores".to_owned(),
            decay_bias: 0.45,
            affected_districts: all_districts.iter().take(2).cloned().collect(),
            affected_route_ids: service_routes.clone(),
            affected_room_ids: self.rooms.iter().take(16).map(|room| room.id).collect(),
            generated_scars: self
                .connections
                .iter()
                .filter(|connection| connection.kind.contains("typology_"))
                .map(|connection| connection.kind.clone())
                .take(4)
                .collect(),
        });
        eras.push(ConstructionEraRecord {
            id: eras.len(),
            era: "expansion".to_owned(),
            age_years: 72,
            material_bias: "habitat decks and commercial routes".to_owned(),
            decay_bias: 0.62,
            affected_districts: all_districts.iter().skip(1).take(3).cloned().collect(),
            affected_route_ids: primary_routes.clone(),
            affected_room_ids: self
                .rooms
                .iter()
                .skip(16)
                .take(24)
                .map(|room| room.id)
                .collect(),
            generated_scars: vec![format!("{}_route_layering", self.config.typology.as_str())],
        });
        eras.push(ConstructionEraRecord {
            id: eras.len(),
            era: "retrofit".to_owned(),
            age_years: 34,
            material_bias: "cables, pumps, gantries, patched lifts".to_owned(),
            decay_bias: 0.78,
            affected_districts: all_districts.iter().rev().take(3).cloned().collect(),
            affected_route_ids: self
                .transit_edges
                .iter()
                .filter(|edge| edge.kind.contains("vertical") || edge.role == "restricted_spine")
                .map(|edge| edge.id)
                .take(8)
                .collect(),
            affected_room_ids: self
                .rooms
                .iter()
                .rev()
                .take(20)
                .map(|room| room.id)
                .collect(),
            generated_scars: self
                .hazard_zones
                .iter()
                .map(|hazard| hazard.kind.clone())
                .take(5)
                .collect(),
        });
        if !failure_zones.is_empty() || self.config.erosion_strength > 0.6 {
            eras.push(ConstructionEraRecord {
                id: eras.len(),
                era: "collapse_and_informal_occupation".to_owned(),
                age_years: 12,
                material_bias: "scrap infill and emergency bypasses".to_owned(),
                decay_bias: 1.0,
                affected_districts: all_districts,
                affected_route_ids: failure_zones
                    .iter()
                    .flat_map(|failure| failure.affected_route_ids.iter().copied())
                    .take(12)
                    .collect(),
                affected_room_ids: self
                    .hazard_zones
                    .iter()
                    .flat_map(|hazard| hazard.room_ids.iter().copied())
                    .take(24)
                    .collect(),
                generated_scars: failure_zones
                    .iter()
                    .map(|failure| format!("failure_zone_{}", failure.id))
                    .collect(),
            });
        }
        eras
    }

    fn section_quality_record(
        &self,
        structural_system: &StructuralSystemRecord,
    ) -> SectionQualityRecord {
        let route_count = self.transit_edges.len().max(1) as f32;
        let vertical_routes = self
            .transit_edges
            .iter()
            .filter(|edge| edge.kind.contains("vertical"))
            .count() as f32;
        let vertical_continuity = (vertical_routes / 3.0).clamp(0.0, 1.0);
        let void_exposure = (self.typology_frame.void_bands.len() as f32 / 2.0)
            .max(
                structural_system.suspended_decks.len() as f32
                    / (self.size * self.layers).max(1) as f32,
            )
            .clamp(0.0, 1.0);
        let service_routes = self
            .transit_edges
            .iter()
            .filter(|edge| matches!(edge.role.as_str(), "maintenance_backbone" | "service_loop"))
            .count() as f32;
        let service_separation = (service_routes / route_count * 2.0).clamp(0.0, 1.0);
        let evacuation_shaft_coverage = (vertical_routes / 4.0).clamp(0.0, 1.0);
        let occupied_layers = (0..self.layers)
            .filter(|y| {
                (0..self.size)
                    .any(|x| (0..self.size).any(|z| self.get(x, z, *y) != CellType::Empty))
            })
            .count() as f32;
        let habitable_layer_ratio = (occupied_layers / self.layers.max(1) as f32).clamp(0.0, 1.0);
        let roof_deck_access = if self
            .transit_edges
            .iter()
            .any(|edge| edge.points.iter().any(|point| point[1] + 2 >= self.layers))
        {
            1.0
        } else {
            0.55
        };
        let cross_section_route_density =
            (route_count / self.layers.max(1) as f32 / 2.0).clamp(0.0, 1.0);
        let mut missing_contracts = Vec::new();
        for (label, value) in [
            ("vertical_continuity", vertical_continuity),
            ("service_separation", service_separation),
            ("evacuation_shaft_coverage", evacuation_shaft_coverage),
            ("habitable_layer_ratio", habitable_layer_ratio),
            ("cross_section_route_density", cross_section_route_density),
        ] {
            if value < 0.35 {
                missing_contracts.push(label.to_owned());
            }
        }
        let score = ((vertical_continuity
            + service_separation
            + evacuation_shaft_coverage
            + habitable_layer_ratio
            + roof_deck_access
            + cross_section_route_density
            + (1.0 - void_exposure * 0.25))
            / 7.0)
            .clamp(0.0, 1.0);
        SectionQualityRecord {
            score,
            vertical_continuity,
            void_exposure,
            service_separation,
            evacuation_shaft_coverage,
            habitable_layer_ratio,
            roof_deck_access,
            cross_section_route_density,
            missing_contracts,
        }
    }

    fn stability_ratings(
        &self,
        frames: &[[usize; 3]],
        foundations: &[[usize; 2]],
        suspended_decks: &[[usize; 3]],
        failure_zones: &[FailurePropagationRecord],
    ) -> Vec<StabilityRatingRecord> {
        let mut ratings = Vec::new();
        for district in [
            DistrictType::Industrial,
            DistrictType::Residential,
            DistrictType::Commercial,
            DistrictType::Slum,
            DistrictType::Elite,
        ] {
            let footprint = (0..self.size)
                .flat_map(|x| (0..self.size).map(move |z| (x, z)))
                .filter(|(x, z)| self.district_at(*x, *z) == district)
                .count()
                .max(1);
            let frame_count = frames
                .iter()
                .filter(|point| self.district_at(point[0], point[2]) == district)
                .count();
            let foundation_count = foundations
                .iter()
                .filter(|point| self.district_at(point[0], point[1]) == district)
                .count();
            let suspended_count = suspended_decks
                .iter()
                .filter(|point| self.district_at(point[0], point[2]) == district)
                .count();
            let hazard_count = self
                .hazard_zones
                .iter()
                .filter(|hazard| {
                    self.district_at(hazard.bounds_min[0], hazard.bounds_min[2]) == district
                })
                .count();
            let failure_count = failure_zones
                .iter()
                .filter(|failure| {
                    self.district_at(failure.origin[0], failure.origin[2]) == district
                })
                .count();
            let cantilever_risk = (suspended_count as f32
                / (frame_count + foundation_count + 1) as f32)
                .clamp(0.0, 1.0);
            let rating = structural_rating(
                frame_count,
                foundation_count,
                footprint,
                cantilever_risk,
                hazard_count + failure_count,
            );
            ratings.push(StabilityRatingRecord {
                target_type: "district".to_owned(),
                target_id: district.name().to_owned(),
                rating,
                load_bearing_frames: frame_count,
                foundation_cells: foundation_count,
                suspended_decks: suspended_count,
                cantilever_risk,
                support_dependency: support_dependency_label(rating).to_owned(),
            });
        }

        for stratum in [
            BiomeStratum::Underground,
            BiomeStratum::Surface,
            BiomeStratum::Midrise,
            BiomeStratum::Skyline,
        ] {
            let ys: Vec<_> = (0..self.layers)
                .filter(|y| self.stratum_at(*y) == stratum)
                .collect();
            if ys.is_empty() {
                continue;
            }
            let frame_count = frames
                .iter()
                .filter(|point| self.stratum_at(point[1]) == stratum)
                .count();
            let foundation_count =
                foundations.len() * usize::from(stratum == BiomeStratum::Underground);
            let suspended_count = suspended_decks
                .iter()
                .filter(|point| self.stratum_at(point[1]) == stratum)
                .count();
            let footprint = ys.len() * self.size * self.size;
            let hazard_count = self
                .hazard_zones
                .iter()
                .filter(|hazard| self.stratum_at(hazard.bounds_min[1]) == stratum)
                .count();
            let failure_count = failure_zones
                .iter()
                .filter(|failure| self.stratum_at(failure.origin[1]) == stratum)
                .count();
            let cantilever_risk = (suspended_count as f32
                / (frame_count + foundation_count + 1) as f32)
                .clamp(0.0, 1.0);
            let rating = structural_rating(
                frame_count,
                foundation_count,
                footprint,
                cantilever_risk,
                hazard_count + failure_count,
            );
            ratings.push(StabilityRatingRecord {
                target_type: "stratum".to_owned(),
                target_id: stratum.name().to_owned(),
                rating,
                load_bearing_frames: frame_count,
                foundation_cells: foundation_count,
                suspended_decks: suspended_count,
                cantilever_risk,
                support_dependency: support_dependency_label(rating).to_owned(),
            });
        }

        for edge in &self.transit_edges {
            let frame_count = frames
                .iter()
                .filter(|frame| {
                    edge.points
                        .iter()
                        .any(|point| point[0].abs_diff(frame[0]) + point[2].abs_diff(frame[2]) <= 2)
                })
                .count();
            let foundation_count = foundations
                .iter()
                .filter(|foundation| {
                    edge.points.iter().any(|point| {
                        point[0].abs_diff(foundation[0]) + point[2].abs_diff(foundation[1]) <= 2
                    })
                })
                .count();
            let suspended_count = edge
                .points
                .iter()
                .filter(|point| suspended_decks.contains(point))
                .count();
            let hazard_count = self
                .hazard_zones
                .iter()
                .filter(|hazard| hazard.route_ids.contains(&edge.id))
                .count();
            let failure_count = failure_zones
                .iter()
                .filter(|failure| failure.affected_route_ids.contains(&edge.id))
                .count();
            let cantilever_risk =
                (suspended_count as f32 / edge.points.len().max(1) as f32).clamp(0.0, 1.0);
            let rating = structural_rating(
                frame_count,
                foundation_count,
                edge.points.len().max(1),
                cantilever_risk,
                hazard_count + failure_count,
            );
            ratings.push(StabilityRatingRecord {
                target_type: "route".to_owned(),
                target_id: edge.id.to_string(),
                rating,
                load_bearing_frames: frame_count,
                foundation_cells: foundation_count,
                suspended_decks: suspended_count,
                cantilever_risk,
                support_dependency: support_dependency_label(rating).to_owned(),
            });
        }

        ratings
    }

    fn ownership_layer(
        &self,
        room_clusters: &[RoomClusterRecord],
    ) -> (
        Vec<FactionRecord>,
        Vec<TerritoryRecord>,
        Vec<ContestedBorderRecord>,
    ) {
        let mut factions: Vec<_> = faction_templates()
            .into_iter()
            .enumerate()
            .map(|(id, (name, agenda))| FactionRecord {
                id,
                name: name.to_owned(),
                agenda: agenda.to_owned(),
                influence: 0.0,
                controlled_districts: Vec::new(),
                controlled_cluster_ids: Vec::new(),
                controlled_route_ids: Vec::new(),
            })
            .collect();
        let mut territories = Vec::new();

        for district in [
            DistrictType::Industrial,
            DistrictType::Residential,
            DistrictType::Commercial,
            DistrictType::Slum,
            DistrictType::Elite,
        ] {
            let faction_id = faction_for_district(district);
            factions[faction_id]
                .controlled_districts
                .push(district.name().to_owned());
            territories.push(TerritoryRecord {
                id: territories.len(),
                faction_id,
                kind: "district".to_owned(),
                district: Some(district.name().to_owned()),
                cluster_id: None,
                route_ids: self
                    .transit_edges
                    .iter()
                    .filter(|edge| {
                        edge.points.iter().any(|point| {
                            self.district_at(
                                point[0].min(self.size - 1),
                                point[2].min(self.size - 1),
                            ) == district
                        })
                    })
                    .map(|edge| edge.id)
                    .take(8)
                    .collect(),
                hazard_pressure: self.hazard_pressure_for_district(district),
            });
        }

        for cluster in room_clusters {
            let faction_id = faction_for_cluster(&cluster.kind, &cluster.owner_district);
            factions[faction_id].controlled_cluster_ids.push(cluster.id);
            for route_id in &cluster.route_ids {
                if !factions[faction_id].controlled_route_ids.contains(route_id) {
                    factions[faction_id].controlled_route_ids.push(*route_id);
                }
            }
            territories.push(TerritoryRecord {
                id: territories.len(),
                faction_id,
                kind: "cluster".to_owned(),
                district: Some(cluster.owner_district.clone()),
                cluster_id: Some(cluster.id),
                route_ids: cluster.route_ids.clone(),
                hazard_pressure: self
                    .hazard_pressure_for_bounds(cluster.bounds_min, cluster.bounds_max),
            });
        }

        for faction in &mut factions {
            let footprint = faction.controlled_districts.len()
                + faction.controlled_cluster_ids.len()
                + faction.controlled_route_ids.len();
            let control_bonus = faction
                .controlled_districts
                .iter()
                .map(|district| {
                    let district_type = district_type_from_name(district);
                    self.lifecycle_for_district(district_type, 0.5)
                        .control_stability
                })
                .sum::<f32>()
                / faction.controlled_districts.len().max(1) as f32;
            faction.influence = (footprint as f32
                / (room_clusters.len() + self.transit_edges.len() + 1) as f32)
                * (0.75 + control_bonus * 0.50).clamp(0.0, 1.0);
        }

        let contested_borders = self
            .district_borders
            .iter()
            .filter_map(|border| {
                let left = faction_for_district_name(&border.from_district)?;
                let right = faction_for_district_name(&border.to_district)?;
                (left != right).then(|| ContestedBorderRecord {
                    border_id: border.id,
                    faction_ids: vec![left, right],
                    intensity: contested_intensity(border, &self.hazard_zones),
                    reason: contested_reason(&border.feature).to_owned(),
                })
            })
            .collect();

        (factions, territories, contested_borders)
    }

    fn hazard_pressure_for_district(&self, district: DistrictType) -> f32 {
        let hazards = self
            .hazard_zones
            .iter()
            .filter(|hazard| {
                self.district_at(hazard.bounds_min[0], hazard.bounds_min[2]) == district
            })
            .count();
        (hazards as f32 * 0.16).clamp(0.0, 1.0)
    }

    fn hazard_pressure_for_bounds(&self, bounds_min: [usize; 3], bounds_max: [usize; 3]) -> f32 {
        let hazards = self
            .hazard_zones
            .iter()
            .filter(|hazard| {
                hazard.bounds_min[0] <= bounds_max[0]
                    && hazard.bounds_max[0] >= bounds_min[0]
                    && hazard.bounds_min[2] <= bounds_max[2]
                    && hazard.bounds_max[2] >= bounds_min[2]
            })
            .count();
        (hazards as f32 * 0.18).clamp(0.0, 1.0)
    }

    fn temporal_state(
        &self,
        factions: &[FactionRecord],
        contested_borders: &[ContestedBorderRecord],
        resource_networks: &[ResourceNetworkRecord],
    ) -> TemporalStateRecord {
        let phase_templates = [
            (
                "blackout",
                2usize,
                "unlit routes and scavenger movement intensify",
            ),
            (
                "market_peak",
                9,
                "markets, border stalls, and commercial arteries surge",
            ),
            (
                "patrol_cycle",
                14,
                "corp security closes gates and sweeps restricted spines",
            ),
            (
                "ventilation_surge",
                18,
                "maintenance fans and heat plumes pulse through loops",
            ),
            (
                "rain_ingress",
                22,
                "water ingress floods low service tunnels and sumps",
            ),
        ];
        let phases = phase_templates
            .into_iter()
            .enumerate()
            .map(|(id, (name, base_hour, description))| {
                let cycle_hour = (base_hour + (self.seed_hash as usize % 3)) % 24;
                let mut active_route_ids = temporal_routes(name, &self.transit_edges);
                if matches!(name, "blackout" | "rain_ingress" | "ventilation_surge") {
                    for network in resource_networks.iter().filter(|network| network.outage) {
                        for route_id in &network.route_ids {
                            if !active_route_ids.contains(route_id) {
                                active_route_ids.push(*route_id);
                            }
                        }
                    }
                }
                TemporalPhaseRecord {
                    id,
                    name: name.to_owned(),
                    cycle_hour,
                    active_route_ids,
                    active_flow_ids: temporal_flows(name, &self.infrastructure_flows),
                    affected_hazard_ids: temporal_hazards(name, &self.hazard_zones),
                    active_faction_ids: temporal_factions(name, factions, contested_borders),
                    description: description.to_owned(),
                }
            })
            .collect();
        TemporalStateRecord {
            cycle_seed: self.seed_hash,
            phases,
        }
    }

    fn resource_networks(&self) -> Vec<ResourceNetworkRecord> {
        let mut grouped: BTreeMap<String, Vec<&InfrastructureFlowRecord>> = BTreeMap::new();
        for flow in &self.infrastructure_flows {
            grouped.entry(flow.kind.clone()).or_default().push(flow);
        }
        grouped
            .into_iter()
            .enumerate()
            .map(|(id, (kind, flows))| {
                let route_ids: Vec<_> = flows.iter().map(|flow| flow.route_id).collect();
                let load = flows.iter().map(|flow| flow.intensity).sum::<f32>();
                let capacity = (flows.len() as f32 * resource_capacity_for_kind(&kind)).max(0.1);
                let overloaded = load > capacity;
                let outage = overloaded
                    || route_ids.iter().any(|route_id| {
                        self.hazard_zones.iter().any(|hazard| {
                            hazard.route_ids.contains(route_id) && hazard.severity > 0.65
                        })
                    });
                let reroute_route_ids = self
                    .transit_edges
                    .iter()
                    .filter(|edge| !route_ids.contains(&edge.id) && edge.kind == "ring_route")
                    .map(|edge| edge.id)
                    .take(4)
                    .collect();
                ResourceNetworkRecord {
                    id,
                    kind,
                    source: flows.first().map(|flow| flow.source).unwrap_or([0, 0, 0]),
                    sink: flows.last().map(|flow| flow.sink).unwrap_or([0, 0, 0]),
                    route_ids,
                    capacity,
                    load,
                    overloaded,
                    outage,
                    reroute_route_ids,
                }
            })
            .collect()
    }

    fn route_simulation(
        &self,
        temporal_state: &TemporalStateRecord,
        resource_networks: &[ResourceNetworkRecord],
        failure_zones: &[FailurePropagationRecord],
    ) -> Vec<RouteSimulationRecord> {
        self.transit_edges
            .iter()
            .map(|edge| {
                let attachment_count = self
                    .transit_attachments
                    .iter()
                    .filter(|attachment| attachment.route_id == edge.id)
                    .count();
                let hazard_pressure: f32 = self
                    .hazard_zones
                    .iter()
                    .filter(|hazard| hazard.route_ids.contains(&edge.id))
                    .map(|hazard| hazard.severity)
                    .sum::<f32>()
                    .clamp(0.0, 1.0);
                let active_phase_ids: Vec<_> = temporal_state
                    .phases
                    .iter()
                    .filter(|phase| phase.active_route_ids.contains(&edge.id))
                    .map(|phase| phase.id)
                    .collect();
                let civilian_density = route_civilian_density(edge, attachment_count);
                let security_pressure = route_security_pressure(edge, hazard_pressure);
                let outage_pressure = resource_outage_pressure(edge.id, resource_networks);
                let failure_pressure = route_failure_pressure(edge.id, failure_zones);
                let stress_pressure = edge
                    .points
                    .get(edge.points.len() / 2)
                    .map(|anchor| self.route_stress_score_for_edge(edge, *anchor))
                    .unwrap_or(0.0);
                let blackout_risk = (route_blackout_risk(edge, hazard_pressure, temporal_state)
                    + outage_pressure * 0.24
                    + failure_pressure * 0.12
                    + stress_pressure * 0.16)
                    .clamp(0.0, 1.0);
                let market_congestion = route_market_congestion(edge, attachment_count);
                let evacuation_viability = (1.0
                    - hazard_pressure * 0.38
                    - security_pressure * 0.18
                    - outage_pressure * 0.18
                    - failure_pressure * 0.22
                    - stress_pressure * 0.20
                    + if edge.role == "evacuation_route" {
                        0.28
                    } else {
                        0.0
                    }
                    + if edge.kind == "ring_route" { 0.14 } else { 0.0 })
                .clamp(0.0, 1.0);
                RouteSimulationRecord {
                    route_id: edge.id,
                    civilian_density,
                    security_pressure,
                    blackout_risk,
                    market_congestion,
                    evacuation_viability,
                    active_phase_ids,
                }
            })
            .collect()
    }

    fn narrative_landmarks(
        &self,
        room_clusters: &[RoomClusterRecord],
        factions: &[FactionRecord],
        contested_borders: &[ContestedBorderRecord],
    ) -> Vec<NarrativeLandmarkRecord> {
        let mut landmarks = Vec::new();
        for edge in self.transit_edges.iter().take(8) {
            let position = edge
                .points
                .get(edge.points.len() / 2)
                .copied()
                .unwrap_or([0, 0, 0]);
            landmarks.push(NarrativeLandmarkRecord {
                id: landmarks.len(),
                name: named_place(self.seed_hash, "route", edge.id, &edge.role, position),
                kind: "route".to_owned(),
                position,
                route_id: Some(edge.id),
                cluster_id: None,
                hazard_id: None,
                border_id: None,
                faction_id: None,
                description: format!("{} controlling {}", edge.role, edge.kind),
            });
        }
        for cluster in room_clusters.iter().take(8) {
            landmarks.push(NarrativeLandmarkRecord {
                id: landmarks.len(),
                name: named_place(
                    self.seed_hash,
                    "cluster",
                    cluster.id,
                    &cluster.kind,
                    cluster.anchor_position,
                ),
                kind: "cluster".to_owned(),
                position: cluster.anchor_position,
                route_id: cluster.route_ids.first().copied(),
                cluster_id: Some(cluster.id),
                hazard_id: None,
                border_id: None,
                faction_id: faction_id_by_name(factions, &cluster.owner_district),
                description: format!("{} held in {}", cluster.kind, cluster.owner_district),
            });
        }
        for hazard in self.hazard_zones.iter().take(6) {
            let position = [
                (hazard.bounds_min[0] + hazard.bounds_max[0]) / 2,
                (hazard.bounds_min[1] + hazard.bounds_max[1]) / 2,
                (hazard.bounds_min[2] + hazard.bounds_max[2]) / 2,
            ];
            landmarks.push(NarrativeLandmarkRecord {
                id: landmarks.len(),
                name: named_place(self.seed_hash, "hazard", hazard.id, &hazard.kind, position),
                kind: "hazard".to_owned(),
                position,
                route_id: hazard.route_ids.first().copied(),
                cluster_id: None,
                hazard_id: Some(hazard.id),
                border_id: None,
                faction_id: None,
                description: format!("{} severity {:.2}", hazard.kind, hazard.severity),
            });
        }
        for contested in contested_borders.iter().take(6) {
            if let Some(border) = self
                .district_borders
                .iter()
                .find(|border| border.id == contested.border_id)
            {
                let position = [
                    (border.bounds_min[0] + border.bounds_max[0]) / 2,
                    border.y,
                    (border.bounds_min[1] + border.bounds_max[1]) / 2,
                ];
                landmarks.push(NarrativeLandmarkRecord {
                    id: landmarks.len(),
                    name: named_place(
                        self.seed_hash,
                        "border",
                        border.id,
                        &border.feature,
                        position,
                    ),
                    kind: "border".to_owned(),
                    position,
                    route_id: border.route_ids.first().copied(),
                    cluster_id: None,
                    hazard_id: None,
                    border_id: Some(border.id),
                    faction_id: contested.faction_ids.first().copied(),
                    description: contested.reason.clone(),
                });
            }
        }
        landmarks
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
        let district_lifecycle = self.district_lifecycle_records(&districts);
        let rule_packs = self.rule_pack_records();
        let strata = self.stratum_records();
        let (room_clusters, room_cluster_ids) = self.room_clusters();
        let macro_massing = self.macro_massing_records();
        let meso_placements = self.meso_placement_records(&room_clusters);
        let micro_details = self.micro_detail_records();
        let failure_zones = self.failure_propagation_records();
        let structural_system = self.structural_system();
        let construction_history = self.construction_history_records(&districts, &failure_zones);
        let section_quality = self.section_quality_record(&structural_system);
        let (factions, territories, contested_borders) = self.ownership_layer(&room_clusters);
        let resource_networks = self.resource_networks();
        let temporal_state = self.temporal_state(&factions, &contested_borders, &resource_networks);
        let route_simulation =
            self.route_simulation(&temporal_state, &resource_networks, &failure_zones);
        let narrative_landmarks =
            self.narrative_landmarks(&room_clusters, &factions, &contested_borders);
        let rule_influences = self.rule_influence_records(
            &rule_packs,
            &districts,
            &room_clusters,
            &self.hazard_zones,
            &narrative_landmarks,
        );
        let rooms: Vec<_> = self
            .rooms
            .iter()
            .cloned()
            .map(|mut room| {
                room.cluster_id = room_cluster_ids.get(room.id).copied().flatten();
                room
            })
            .collect();
        let path_analysis = self.finalize_topology_quality(
            self.path_analysis(),
            &rooms,
            &territories,
            &narrative_landmarks,
            &failure_zones,
        );
        let typology_quality = typology_quality_record(
            self.config.typology,
            &self.typology_frame,
            &self.transit_edges,
        );
        let metadata = StructureMetadata {
            schema_version: STRUCTURE_SCHEMA_VERSION.to_owned(),
            profile: self.config.profile.to_string(),
            typology: self.config.typology.to_string(),
            config: self.config.clone(),
            district_counts: district_counts_map(district_counts),
            stratum_counts: stratum_counts_map(stratum_counts),
            cell_counts: cell_counts_map(cell_counts),
            material_counts: material_counts_map(material_counts),
            connection_counts: connection_counts_map(&self.connections),
            room_counts: room_counts_map(&self.rooms),
            pattern_counts: self.pattern_counts.clone(),
            room_count: rooms.len(),
            connection_count: self.connections.len(),
            transit_node_count: self.transit_nodes.len(),
            transit_edge_count: self.transit_edges.len(),
            transit_attachment_count: self.transit_attachments.len(),
            district_record_count: districts.len(),
            district_lifecycle_count: district_lifecycle.len(),
            stratum_record_count: strata.len(),
            macro_massing_count: macro_massing.len(),
            meso_placement_count: meso_placements.len(),
            micro_detail_count: micro_details.len(),
            district_border_count: self.district_borders.len(),
            room_cluster_count: room_clusters.len(),
            infrastructure_flow_count: self.infrastructure_flows.len(),
            route_simulation_count: route_simulation.len(),
            resource_network_count: resource_networks.len(),
            hazard_zone_count: self.hazard_zones.len(),
            structural_rating_count: structural_system.stability_ratings.len(),
            load_bearing_frame_count: structural_system.load_bearing_frames.len(),
            suspended_deck_count: structural_system.suspended_decks.len(),
            stress_field_count: structural_system.stress_fields.len(),
            load_path_count: structural_system.load_paths.len(),
            failure_zone_count: failure_zones.len(),
            construction_era_count: construction_history.len(),
            rule_pack_count: rule_packs.len(),
            rule_influence_count: rule_influences.len(),
            faction_count: factions.len(),
            territory_count: territories.len(),
            contested_border_count: contested_borders.len(),
            temporal_phase_count: temporal_state.phases.len(),
            narrative_landmark_count: narrative_landmarks.len(),
            entity_count: self.entities.len(),
            entity_path_count: self.entity_paths.len(),
            entity_pressure_field_count: self.entity_pressure_fields.len(),
            layout_mutation_count: self.layout_mutations.len(),
            occupied_cell_ratio: occupied as f32 / total_cells as f32,
        };
        SavedStructure {
            seed: self.seed.clone(),
            size: self.size,
            layers: self.layers,
            metadata,
            typology_frame: self.typology_frame.clone(),
            typology_quality,
            construction_history,
            section_quality,
            grid,
            connections: self.connections.clone(),
            rooms,
            transit_graph: self.transit_graph(),
            districts,
            district_lifecycle,
            strata,
            macro_massing,
            meso_placements,
            micro_details,
            district_borders: self.district_borders.clone(),
            room_clusters,
            path_analysis,
            infrastructure_flows: self.infrastructure_flows.clone(),
            route_simulation,
            resource_networks,
            hazard_zones: self.hazard_zones.clone(),
            structural_system,
            failure_zones,
            rule_packs,
            rule_influences,
            factions,
            territories,
            contested_borders,
            temporal_state,
            narrative_landmarks,
            entities: self.entities.clone(),
            entity_paths: self.entity_paths.clone(),
            entity_pressure_fields: self.entity_pressure_fields.clone(),
            layout_mutations: self.layout_mutations.clone(),
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
    generate_saved_structure_with_rules(seed, config, CompiledRulePackSet::default())
}

pub fn generate_saved_structure_with_rules(
    seed: String,
    config: GenerationConfig,
    rule_packs: CompiledRulePackSet,
) -> StructureResult<SavedStructure> {
    config
        .validate()
        .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { error.into() })?;
    let mut generator = MegaStructureGenerator::with_config_and_rules(seed, config, rule_packs);
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

fn build_typology_frame(
    typology: MegastructureTypology,
    size: usize,
    layers: usize,
) -> TypologyFrameRecord {
    let anchors = typology_anchor_points(typology, size, layers);
    let center = size / 2;
    let mid = layers / 2;
    let band = |kind: &str, min: [usize; 3], max: [usize; 3]| TypologyBandRecord {
        kind: kind.to_owned(),
        bounds_min: min,
        bounds_max: max,
    };
    let (primary_axes, void_bands, habitat_bands, traversal_contract) = match typology {
        MegastructureTypology::DenseEnclave => (
            vec!["x".to_owned(), "z".to_owned(), "y".to_owned()],
            Vec::new(),
            vec![band(
                "dense_enclave_field",
                [0, 0, 0],
                [size - 1, layers - 1, size - 1],
            )],
            vec![
                "service-to-skyline route".to_owned(),
                "ring route redundancy".to_owned(),
            ],
        ),
        MegastructureTypology::ArcologySpire => (
            vec!["y".to_owned(), "radial".to_owned()],
            vec![band(
                "upper_light_well",
                [center.saturating_sub(2), mid, center.saturating_sub(2)],
                [
                    (center + 2).min(size - 1),
                    layers - 1,
                    (center + 2).min(size - 1),
                ],
            )],
            vec![band(
                "stacked_habitat_shell",
                [
                    center.saturating_sub(size / 4),
                    0,
                    center.saturating_sub(size / 4),
                ],
                [
                    (center + size / 4).min(size - 1),
                    layers - 1,
                    (center + size / 4).min(size - 1),
                ],
            )],
            vec![
                "central evacuation core".to_owned(),
                "stacked station loops".to_owned(),
            ],
        ),
        MegastructureTypology::LinearCity => (
            vec!["x".to_owned()],
            vec![band(
                "preserved_flank_voids",
                [0, 0, 0],
                [size - 1, layers - 1, center.saturating_sub(size / 8)],
            )],
            vec![band(
                "linear_habitat_ribbon",
                [0, 0, center.saturating_sub(size / 8)],
                [size - 1, layers - 1, (center + size / 8).min(size - 1)],
            )],
            vec![
                "linear express spine".to_owned(),
                "station loop ribs".to_owned(),
            ],
        ),
        MegastructureTypology::BridgeVoid => (
            vec!["diagonal".to_owned(), "skybridge".to_owned()],
            vec![band(
                "central_void",
                [
                    center.saturating_sub(size / 7),
                    0,
                    center.saturating_sub(size / 7),
                ],
                [
                    (center + size / 7).min(size - 1),
                    layers - 1,
                    (center + size / 7).min(size - 1),
                ],
            )],
            vec![band(
                "tower_islands",
                [0, 0, 0],
                [size - 1, layers - 1, size - 1],
            )],
            vec![
                "void bridges".to_owned(),
                "redundant vertical tower cores".to_owned(),
            ],
        ),
        MegastructureTypology::MarinePlatform => (
            vec!["deck".to_owned(), "pylon_grid".to_owned()],
            vec![band(
                "tidal_underdeck",
                [0, 0, 0],
                [size - 1, layers / 5, size - 1],
            )],
            vec![band(
                "artificial_land_deck",
                [0, layers / 5, 0],
                [size - 1, (layers / 3).max(1), size - 1],
            )],
            vec![
                "marine causeways".to_owned(),
                "pylon service grid".to_owned(),
            ],
        ),
        MegastructureTypology::OrbitalRing => (
            vec!["rim".to_owned(), "spokes".to_owned()],
            vec![band(
                "central_zero_g_void",
                [
                    center.saturating_sub(size / 5),
                    0,
                    center.saturating_sub(size / 5),
                ],
                [
                    (center + size / 5).min(size - 1),
                    layers - 1,
                    (center + size / 5).min(size - 1),
                ],
            )],
            vec![band(
                "rim_habitat_tube",
                [0, mid.saturating_sub(2), 0],
                [size - 1, (mid + 2).min(layers - 1), size - 1],
            )],
            vec!["rim loop".to_owned(), "spoke transfers".to_owned()],
        ),
        MegastructureTypology::UndergroundHive => (
            vec!["subgrade".to_owned(), "gallery_grid".to_owned()],
            vec![band(
                "surface_overburden",
                [0, layers / 2, 0],
                [size - 1, layers - 1, size - 1],
            )],
            vec![band(
                "cavern_habitat",
                [0, 0, 0],
                [size - 1, layers / 2, size - 1],
            )],
            vec!["hive trunk".to_owned(), "cavern loops".to_owned()],
        ),
        MegastructureTypology::MountainBurrow => (
            vec!["cliff".to_owned(), "depth".to_owned()],
            vec![band(
                "exposed_cliff_face",
                [size / 2, 0, 0],
                [size - 1, layers - 1, size - 1],
            )],
            vec![band(
                "cut_mountain_rooms",
                [0, 0, 0],
                [size / 2, layers - 1, size - 1],
            )],
            vec!["cliff galleries".to_owned(), "burrow spines".to_owned()],
        ),
        MegastructureTypology::DesertArcology => (
            vec!["y".to_owned(), "solar_field".to_owned()],
            vec![band(
                "heat_buffer_void",
                [0, layers / 2, 0],
                [size - 1, layers - 1, size - 1],
            )],
            vec![band(
                "sealed_habitat_core",
                [
                    center.saturating_sub(size / 4),
                    0,
                    center.saturating_sub(size / 4),
                ],
                [
                    (center + size / 4).min(size - 1),
                    layers - 1,
                    (center + size / 4).min(size - 1),
                ],
            )],
            vec!["climate spine".to_owned(), "solar service ring".to_owned()],
        ),
        MegastructureTypology::AirportCity => (
            vec!["runway".to_owned(), "terminal".to_owned()],
            vec![band(
                "runway_clearance",
                [0, 0, center.saturating_sub(size / 12)],
                [size - 1, layers / 4, (center + size / 12).min(size - 1)],
            )],
            vec![band(
                "terminal_habitat",
                [0, layers / 4, 0],
                [size - 1, layers - 1, size - 1],
            )],
            vec!["runway spine".to_owned(), "terminal loops".to_owned()],
        ),
        MegastructureTypology::DamCity => (
            vec!["dam_wall".to_owned(), "reservoir".to_owned()],
            vec![band(
                "reservoir_face",
                [0, 0, 0],
                [center.saturating_sub(size / 10), layers - 1, size - 1],
            )],
            vec![band(
                "inhabited_dam_wall",
                [center.saturating_sub(size / 10), 0, 0],
                [(center + size / 10).min(size - 1), layers - 1, size - 1],
            )],
            vec!["dam wall spine".to_owned(), "turbine galleries".to_owned()],
        ),
        MegastructureTypology::ShipyardStack => (
            vec!["drydock".to_owned(), "gantry".to_owned()],
            vec![band(
                "drydock_void",
                [0, 0, center.saturating_sub(size / 10)],
                [size - 1, layers / 3, (center + size / 10).min(size - 1)],
            )],
            vec![band(
                "shipyard_stack",
                [0, layers / 3, 0],
                [size - 1, layers - 1, size - 1],
            )],
            vec!["drydock spine".to_owned(), "gantry loops".to_owned()],
        ),
        MegastructureTypology::VolcanicCaldera => (
            vec!["caldera".to_owned(), "geothermal".to_owned()],
            vec![band(
                "lava_crater_void",
                [
                    center.saturating_sub(size / 6),
                    0,
                    center.saturating_sub(size / 6),
                ],
                [
                    (center + size / 6).min(size - 1),
                    layers - 1,
                    (center + size / 6).min(size - 1),
                ],
            )],
            vec![band("caldera_habitat_ring", [0, 0, 0], [size - 1, mid, size - 1])],
            vec!["caldera ring".to_owned(), "geothermal shaft".to_owned()],
        ),
        MegastructureTypology::IceShelfCity => (
            vec!["shelf".to_owned(), "crevasse".to_owned()],
            vec![band("crevasse_field", [0, 0, 0], [size - 1, layers / 4, size - 1])],
            vec![band("ice_shelf_habitat", [0, layers / 5, 0], [size - 1, mid, size - 1])],
            vec!["meltwater spine".to_owned(), "crevasse bridges".to_owned()],
        ),
        MegastructureTypology::CanopyBabel => (
            vec!["trunk".to_owned(), "canopy".to_owned()],
            vec![band("forest_understory", [0, 0, 0], [size - 1, mid, size - 1])],
            vec![band("upper_canopy_habitat", [0, mid, 0], [size - 1, layers - 1, size - 1])],
            vec!["canopy walks".to_owned(), "root service".to_owned()],
        ),
        MegastructureTypology::SpaceElevatorAnchor => (
            vec!["tether".to_owned(), "cargo_ring".to_owned()],
            vec![band(
                "launch_exclusion_zone",
                [
                    center.saturating_sub(size / 5),
                    0,
                    center.saturating_sub(size / 5),
                ],
                [
                    (center + size / 5).min(size - 1),
                    layers / 4,
                    (center + size / 5).min(size - 1),
                ],
            )],
            vec![band(
                "anchor_habitat",
                [
                    center.saturating_sub(size / 4),
                    0,
                    center.saturating_sub(size / 4),
                ],
                [
                    (center + size / 4).min(size - 1),
                    layers - 1,
                    (center + size / 4).min(size - 1),
                ],
            )],
            vec!["tether core".to_owned(), "cargo ring".to_owned()],
        ),
        MegastructureTypology::CrawlerCity => (
            vec!["track".to_owned(), "convoy".to_owned()],
            vec![band(
                "track_clearance",
                [0, 0, center.saturating_sub(size / 8)],
                [size - 1, layers / 4, (center + size / 8).min(size - 1)],
            )],
            vec![band("crawler_deck_habitat", [0, 0, 0], [size - 1, mid, size - 1])],
            vec!["crawler track".to_owned(), "engine spine".to_owned()],
        ),
        MegastructureTypology::ReefAtollArcology => (
            vec!["reef_ring".to_owned(), "lagoon".to_owned()],
            vec![band(
                "lagoon_void",
                [
                    center.saturating_sub(size / 5),
                    0,
                    center.saturating_sub(size / 5),
                ],
                [
                    (center + size / 5).min(size - 1),
                    layers / 3,
                    (center + size / 5).min(size - 1),
                ],
            )],
            vec![band("reef_habitat_ring", [0, 0, 0], [size - 1, mid, size - 1])],
            vec!["reef ring".to_owned(), "lagoon causeway".to_owned()],
        ),
        MegastructureTypology::StratospherePlatform => (
            vec!["lift_cell".to_owned(), "pressure_deck".to_owned()],
            vec![band("open_air_below", [0, 0, 0], [size - 1, mid, size - 1])],
            vec![band("stratosphere_deck", [0, mid, 0], [size - 1, layers - 1, size - 1])],
            vec!["lift-cell spine".to_owned(), "pressure deck".to_owned()],
        ),
        MegastructureTypology::SinkholeCitadel => (
            vec!["rim".to_owned(), "descent".to_owned()],
            vec![band(
                "sinkhole_void",
                [
                    center.saturating_sub(size / 6),
                    0,
                    center.saturating_sub(size / 6),
                ],
                [
                    (center + size / 6).min(size - 1),
                    layers - 1,
                    (center + size / 6).min(size - 1),
                ],
            )],
            vec![band("sinkhole_rim_habitat", [0, 0, 0], [size - 1, layers - 1, size - 1])],
            vec!["sinkhole ring".to_owned(), "descent shaft".to_owned()],
        ),
    };
    TypologyFrameRecord {
        typology: typology.as_str().to_owned(),
        primary_axes,
        primary_spines: anchors.clone(),
        void_bands,
        habitat_bands,
        service_anchors: anchors,
        traversal_contract,
    }
}

fn typology_quality_record(
    typology: MegastructureTypology,
    frame: &TypologyFrameRecord,
    edges: &[TransitEdgeRecord],
) -> TypologyQualityRecord {
    let route_count = |kind: &str| edges.iter().filter(|edge| edge.kind == kind).count();
    let route_length = |kind: &str| {
        edges
            .iter()
            .filter(|edge| edge.kind == kind)
            .map(|edge| edge.length)
            .sum::<usize>() as f32
    };
    let mut contract_scores = BTreeMap::new();
    let mut required_route_kinds = Vec::new();
    let mut missing_contracts = Vec::new();

    let mut add_route_contract = |label: &str, route_kind: &str, actual: f32, required: f32| {
        required_route_kinds.push(route_kind.to_owned());
        let score = if required <= 0.0 {
            1.0
        } else {
            (actual / required).clamp(0.0, 1.0)
        };
        contract_scores.insert(label.to_owned(), score);
        if score < 1.0 {
            missing_contracts.push(label.to_owned());
        }
    };

    match typology {
        MegastructureTypology::DenseEnclave => {
            add_route_contract(
                "legacy_dense_connectivity",
                "ring_route",
                route_count("ring_route") as f32,
                1.0,
            );
        }
        MegastructureTypology::ArcologySpire => {
            add_route_contract(
                "stacked_station_loops",
                "station_loop",
                route_count("station_loop") as f32,
                2.0,
            );
            add_route_contract(
                "central_vertical_core",
                "vertical_transit_core",
                route_count("vertical_transit_core") as f32,
                1.0,
            );
        }
        MegastructureTypology::LinearCity => {
            add_route_contract(
                "continuous_express_spine",
                "linear_express",
                route_length("linear_express"),
                20.0,
            );
            add_route_contract(
                "station_loop_ribs",
                "station_loop",
                route_count("station_loop") as f32,
                2.0,
            );
        }
        MegastructureTypology::BridgeVoid => {
            add_route_contract(
                "redundant_void_bridges",
                "void_bridge",
                route_count("void_bridge") as f32,
                2.0,
            );
            contract_scores.insert(
                "tower_islands".to_owned(),
                (frame.service_anchors.len() as f32 / 4.0).clamp(0.0, 1.0),
            );
            if frame.service_anchors.len() < 4 {
                missing_contracts.push("tower_islands".to_owned());
            }
        }
        MegastructureTypology::MarinePlatform => {
            add_route_contract(
                "marine_causeway_access",
                "marine_causeway",
                route_count("marine_causeway") as f32,
                2.0,
            );
            add_route_contract(
                "pylon_service_grid",
                "pylon_service",
                route_count("pylon_service") as f32,
                2.0,
            );
        }
        MegastructureTypology::OrbitalRing => {
            add_route_contract(
                "rim_continuity",
                "rim_loop",
                route_count("rim_loop") as f32,
                6.0,
            );
            add_route_contract(
                "spoke_redundancy",
                "spoke_transfer",
                route_count("spoke_transfer") as f32,
                2.0,
            );
        }
        MegastructureTypology::UndergroundHive => {
            add_route_contract(
                "hive_trunk",
                "hive_trunk",
                route_count("hive_trunk") as f32,
                1.0,
            );
            add_route_contract(
                "cavern_loops",
                "cavern_loop",
                route_count("cavern_loop") as f32,
                2.0,
            );
        }
        MegastructureTypology::MountainBurrow => {
            add_route_contract(
                "cliff_gallery",
                "cliff_gallery",
                route_count("cliff_gallery") as f32,
                1.0,
            );
            add_route_contract(
                "burrow_spine",
                "burrow_spine",
                route_count("burrow_spine") as f32,
                1.0,
            );
        }
        MegastructureTypology::DesertArcology => {
            add_route_contract(
                "climate_spine",
                "climate_spine",
                route_count("climate_spine") as f32,
                1.0,
            );
            add_route_contract(
                "solar_service_ring",
                "solar_service_ring",
                route_count("solar_service_ring") as f32,
                1.0,
            );
        }
        MegastructureTypology::AirportCity => {
            add_route_contract(
                "runway_spine",
                "runway_spine",
                route_count("runway_spine") as f32,
                1.0,
            );
            add_route_contract(
                "terminal_loops",
                "terminal_loop",
                route_count("terminal_loop") as f32,
                2.0,
            );
        }
        MegastructureTypology::DamCity => {
            add_route_contract(
                "dam_wall_spine",
                "dam_wall_spine",
                route_count("dam_wall_spine") as f32,
                1.0,
            );
            add_route_contract(
                "turbine_gallery",
                "turbine_gallery",
                route_count("turbine_gallery") as f32,
                1.0,
            );
        }
        MegastructureTypology::ShipyardStack => {
            add_route_contract(
                "drydock_spine",
                "drydock_spine",
                route_count("drydock_spine") as f32,
                1.0,
            );
            add_route_contract(
                "gantry_loops",
                "gantry_loop",
                route_count("gantry_loop") as f32,
                2.0,
            );
        }
        MegastructureTypology::VolcanicCaldera => {
            add_route_contract(
                "caldera_ring",
                "caldera_ring",
                route_count("caldera_ring") as f32,
                4.0,
            );
            add_route_contract(
                "geothermal_shaft",
                "geothermal_shaft",
                route_count("geothermal_shaft") as f32,
                1.0,
            );
        }
        MegastructureTypology::IceShelfCity => {
            add_route_contract(
                "meltwater_spine",
                "meltwater_spine",
                route_count("meltwater_spine") as f32,
                1.0,
            );
            add_route_contract(
                "crevasse_bridges",
                "crevasse_bridge",
                route_count("crevasse_bridge") as f32,
                2.0,
            );
        }
        MegastructureTypology::CanopyBabel => {
            add_route_contract(
                "canopy_walks",
                "canopy_walk",
                route_count("canopy_walk") as f32,
                2.0,
            );
            add_route_contract(
                "root_service",
                "root_service",
                route_count("root_service") as f32,
                1.0,
            );
        }
        MegastructureTypology::SpaceElevatorAnchor => {
            add_route_contract(
                "tether_core",
                "tether_core",
                route_count("tether_core") as f32,
                1.0,
            );
            add_route_contract(
                "cargo_ring",
                "cargo_ring",
                route_count("cargo_ring") as f32,
                1.0,
            );
        }
        MegastructureTypology::CrawlerCity => {
            add_route_contract(
                "crawler_track",
                "crawler_track",
                route_count("crawler_track") as f32,
                1.0,
            );
            add_route_contract(
                "engine_spine",
                "engine_spine",
                route_count("engine_spine") as f32,
                1.0,
            );
        }
        MegastructureTypology::ReefAtollArcology => {
            add_route_contract(
                "reef_ring",
                "reef_ring",
                route_count("reef_ring") as f32,
                4.0,
            );
            add_route_contract(
                "lagoon_causeway",
                "lagoon_causeway",
                route_count("lagoon_causeway") as f32,
                1.0,
            );
        }
        MegastructureTypology::StratospherePlatform => {
            add_route_contract(
                "pressure_deck",
                "pressure_deck",
                route_count("pressure_deck") as f32,
                1.0,
            );
            add_route_contract(
                "lift_cell_spine",
                "lift_cell_spine",
                route_count("lift_cell_spine") as f32,
                1.0,
            );
        }
        MegastructureTypology::SinkholeCitadel => {
            add_route_contract(
                "sinkhole_ring",
                "sinkhole_ring",
                route_count("sinkhole_ring") as f32,
                4.0,
            );
            add_route_contract(
                "descent_shaft",
                "descent_shaft",
                route_count("descent_shaft") as f32,
                1.0,
            );
        }
    }

    if !frame.void_bands.is_empty() {
        contract_scores.insert("void_band_declared".to_owned(), 1.0);
    }
    if !frame.habitat_bands.is_empty() {
        contract_scores.insert("habitat_band_declared".to_owned(), 1.0);
    }
    if !frame.traversal_contract.is_empty() {
        contract_scores.insert("traversal_contract_declared".to_owned(), 1.0);
    }
    let score = if contract_scores.is_empty() {
        0.0
    } else {
        contract_scores.values().sum::<f32>() / contract_scores.len() as f32
    };
    TypologyQualityRecord {
        typology: typology.as_str().to_owned(),
        score: score.clamp(0.0, 1.0),
        contract_scores,
        required_route_kinds,
        missing_contracts,
    }
}

fn typology_anchor_points(
    typology: MegastructureTypology,
    size: usize,
    layers: usize,
) -> Vec<[usize; 3]> {
    let c = size / 2;
    let y = (layers / 2).max(1).min(layers.saturating_sub(1));
    match typology {
        MegastructureTypology::DenseEnclave | MegastructureTypology::ArcologySpire => {
            vec![[c, y, c]]
        }
        MegastructureTypology::LinearCity => (1..size.saturating_sub(1))
            .step_by((size / 5).max(4))
            .map(|x| [x, y, c])
            .collect(),
        MegastructureTypology::BridgeVoid => {
            let lo = (size / 5).max(2);
            let hi = size.saturating_sub(lo + 1).max(lo);
            vec![[lo, y, lo], [hi, y, lo], [hi, y, hi], [lo, y, hi]]
        }
        MegastructureTypology::MarinePlatform => {
            let step = (size / 4).max(4);
            let mut points = Vec::new();
            for x in (step / 2..size).step_by(step) {
                for z in (step / 2..size).step_by(step) {
                    points.push([x, (layers / 4).max(1), z]);
                }
            }
            points
        }
        MegastructureTypology::OrbitalRing => {
            let r = (size as f32 * 0.34).round() as isize;
            let c = c as isize;
            [
                (0, -r),
                (r / 2, -r / 2),
                (r, 0),
                (r / 2, r / 2),
                (0, r),
                (-r / 2, r / 2),
                (-r, 0),
                (-r / 2, -r / 2),
            ]
            .into_iter()
            .map(|(dx, dz)| {
                [
                    (c + dx).clamp(1, size.saturating_sub(2) as isize) as usize,
                    y,
                    (c + dz).clamp(1, size.saturating_sub(2) as isize) as usize,
                ]
            })
            .collect()
        }
        MegastructureTypology::UndergroundHive => vec![
            [c, layers / 4, c],
            [c / 2, layers / 4, c],
            [(c + c / 2).min(size - 1), layers / 4, c],
        ],
        MegastructureTypology::MountainBurrow => vec![[size / 4, y, c], [c, y, c]],
        MegastructureTypology::DesertArcology => vec![
            [c, y, c],
            [c, (layers / 5).max(1), c / 3],
            [c, (layers / 5).max(1), (c + size / 3).min(size - 1)],
        ],
        MegastructureTypology::AirportCity => (1..size.saturating_sub(1))
            .step_by((size / 5).max(4))
            .map(|x| [x, (layers / 5).max(1), c])
            .collect(),
        MegastructureTypology::DamCity => vec![
            [c, y, 1],
            [c, y, size.saturating_sub(2)],
            [c, layers / 3, c],
        ],
        MegastructureTypology::ShipyardStack => vec![
            [1, layers / 4, c],
            [size.saturating_sub(2), layers / 4, c],
            [c, layers / 3, c],
        ],
        MegastructureTypology::VolcanicCaldera
        | MegastructureTypology::ReefAtollArcology
        | MegastructureTypology::SinkholeCitadel => {
            let r = (size as f32 * 0.34).round() as isize;
            let c = c as isize;
            [
                (0, -r),
                (r / 2, -r / 2),
                (r, 0),
                (r / 2, r / 2),
                (0, r),
                (-r / 2, r / 2),
                (-r, 0),
                (-r / 2, -r / 2),
            ]
            .into_iter()
            .map(|(dx, dz)| {
                [
                    (c + dx).clamp(1, size.saturating_sub(2) as isize) as usize,
                    y,
                    (c + dz).clamp(1, size.saturating_sub(2) as isize) as usize,
                ]
            })
            .collect()
        }
        MegastructureTypology::IceShelfCity => (1..size.saturating_sub(1))
            .step_by((size / 5).max(4))
            .map(|x| [x, (layers / 5).max(1), c])
            .collect(),
        MegastructureTypology::CanopyBabel => vec![
            [c, y, c],
            [c / 2, y, c],
            [(c + c / 2).min(size - 1), y, c],
            [c, y, c / 2],
            [c, y, (c + c / 2).min(size - 1)],
        ],
        MegastructureTypology::SpaceElevatorAnchor => vec![
            [c, y, c],
            [c, layers.saturating_sub(2), c],
            [c / 2, layers / 4, c],
            [(c + c / 2).min(size - 1), layers / 4, c],
        ],
        MegastructureTypology::CrawlerCity => vec![
            [1, layers / 5, c],
            [c, layers / 5, c],
            [size.saturating_sub(2), layers / 5, c],
        ],
        MegastructureTypology::StratospherePlatform => vec![
            [c, y, c],
            [c / 2, y, c],
            [(c + c / 2).min(size - 1), y, c],
            [c, y, c / 2],
            [c, y, (c + c / 2).min(size - 1)],
        ],
    }
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

fn district_type_from_name(name: &str) -> DistrictType {
    match name {
        "INDUSTRIAL" => DistrictType::Industrial,
        "RESIDENTIAL" => DistrictType::Residential,
        "COMMERCIAL" => DistrictType::Commercial,
        "SLUM" => DistrictType::Slum,
        "ELITE" => DistrictType::Elite,
        _ => DistrictType::Residential,
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

fn cluster_kind_for_room(label: &str) -> &'static str {
    match label {
        label if label.contains("HABITATION") => "habitation_block",
        label if label.contains("MARKET") || label.contains("BAZAAR") => "market_strip",
        label
            if label.contains("MACHINE") || label.contains("PIPE") || label.contains("SERVICE") =>
        {
            "machine_complex"
        }
        label if label.contains("SHRINE") => "shrine_pocket",
        label if label.contains("VAULT") || label.contains("DATA") => "data_vault_compound",
        label if label.contains("TRANSIT") || label.contains("CHOKEPOINT") => "transit_cluster",
        _ => "mixed_room_cluster",
    }
}

fn route_role_for(
    kind: &str,
    start: Option<[usize; 3]>,
    end: Option<[usize; 3]>,
    generator: &MegaStructureGenerator,
) -> &'static str {
    if kind == "vertical_transit_core" {
        return "evacuation_route";
    }
    if kind == "service_tunnel" {
        return "maintenance_backbone";
    }
    if kind == "express_spine" {
        return "restricted_spine";
    }
    if kind == "ring_route" {
        return "primary_artery";
    }
    match kind {
        "linear_express" | "rim_loop" | "hive_trunk" | "runway_spine" | "dam_wall_spine"
        | "drydock_spine" => return "primary_artery",
        "station_loop" | "terminal_loop" | "gantry_loop" | "cavern_loop" => return "market_run",
        "void_bridge" | "spoke_transfer" | "cliff_gallery" | "burrow_spine" => {
            return "evacuation_route"
        }
        "marine_causeway" | "climate_spine" | "solar_service_ring" | "turbine_gallery" => {
            return "service_loop"
        }
        "pylon_service" | "hive_gallery" => return "maintenance_backbone",
        _ => {}
    }
    if kind.contains("vertical_transfer") {
        return "evacuation_route";
    }
    let Some(start) = start else {
        return "primary_artery";
    };
    let Some(end) = end else {
        return "primary_artery";
    };
    let start_district = generator.district_at(
        start[0].min(generator.size - 1),
        start[2].min(generator.size - 1),
    );
    let end_district = generator.district_at(
        end[0].min(generator.size - 1),
        end[2].min(generator.size - 1),
    );
    if kind == "skybridge"
        && (start_district == DistrictType::Elite || end_district == DistrictType::Elite)
    {
        "restricted_spine"
    } else if start_district == DistrictType::Commercial || end_district == DistrictType::Commercial
    {
        "market_run"
    } else if start_district == DistrictType::Industrial || end_district == DistrictType::Industrial
    {
        "service_loop"
    } else {
        "primary_artery"
    }
}

fn border_feature_name(a: DistrictType, b: DistrictType) -> &'static str {
    if same_pair(a, b, DistrictType::Slum, DistrictType::Commercial) {
        "BORDER_MARKET"
    } else if same_pair(a, b, DistrictType::Industrial, DistrictType::Slum) {
        "SCRAP_ZONE"
    } else if same_pair(a, b, DistrictType::Elite, DistrictType::Commercial) {
        "SECURITY_THRESHOLD"
    } else if a == DistrictType::Residential || b == DistrictType::Residential {
        "SURFACE_COMMONS"
    } else {
        "SCRAP_MARKET"
    }
}

fn border_owner_district(a: DistrictType, b: DistrictType) -> DistrictType {
    if a == DistrictType::Commercial || b == DistrictType::Commercial {
        DistrictType::Commercial
    } else if a == DistrictType::Industrial || b == DistrictType::Industrial {
        DistrictType::Industrial
    } else {
        a
    }
}

fn border_feature_y(a: DistrictType, b: DistrictType, layers: usize) -> usize {
    let y = if a == DistrictType::Elite || b == DistrictType::Elite {
        layers * 2 / 3
    } else if a == DistrictType::Industrial || b == DistrictType::Industrial {
        layers / 4
    } else {
        layers / 3
    };
    y.min(layers.saturating_sub(1))
}

fn same_pair(a: DistrictType, b: DistrictType, left: DistrictType, right: DistrictType) -> bool {
    (a == left && b == right) || (a == right && b == left)
}

fn find_root(parent: &mut [usize], value: usize) -> usize {
    if parent[value] != value {
        parent[value] = find_root(parent, parent[value]);
    }
    parent[value]
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let root_a = find_root(parent, a);
    let root_b = find_root(parent, b);
    if root_a != root_b {
        parent[root_b] = root_a;
    }
}

fn structural_rating(
    frame_count: usize,
    foundation_count: usize,
    footprint: usize,
    cantilever_risk: f32,
    hazard_count: usize,
) -> f32 {
    let support_score = ((frame_count * 2 + foundation_count) as f32 / footprint.max(1) as f32)
        .sqrt()
        .clamp(0.0, 1.0);
    let hazard_penalty = (hazard_count as f32 * 0.06).clamp(0.0, 0.35);
    (0.35 + support_score * 0.55 - cantilever_risk * 0.28 - hazard_penalty).clamp(0.0, 1.0)
}

fn support_dependency_label(rating: f32) -> &'static str {
    if rating >= 0.72 {
        "redundant frame grid"
    } else if rating >= 0.48 {
        "localized support dependency"
    } else {
        "critical cantilever dependency"
    }
}

fn faction_templates() -> [(&'static str, &'static str); 5] {
    [
        (
            "Civic Maintenance",
            "keep core utilities and evacuation routes alive",
        ),
        (
            "Market Syndicates",
            "control trade arteries and border markets",
        ),
        (
            "Shrine Communes",
            "hold habitation shrines and repair pockets",
        ),
        (
            "Corp Security",
            "lock down skyline vaults and restricted spines",
        ),
        (
            "Scavenger Crews",
            "strip scrap zones, blackouts, and collapsed decks",
        ),
    ]
}

fn faction_for_district(district: DistrictType) -> usize {
    match district {
        DistrictType::Industrial => 0,
        DistrictType::Residential => 2,
        DistrictType::Commercial => 1,
        DistrictType::Slum => 4,
        DistrictType::Elite => 3,
    }
}

fn faction_for_district_name(name: &str) -> Option<usize> {
    match name {
        "INDUSTRIAL" => Some(0),
        "RESIDENTIAL" => Some(2),
        "COMMERCIAL" => Some(1),
        "SLUM" => Some(4),
        "ELITE" => Some(3),
        _ => None,
    }
}

fn faction_for_cluster(kind: &str, district: &str) -> usize {
    match kind {
        "market_strip" => 1,
        "shrine_pocket" | "habitation_block" => 2,
        "data_vault_compound" => 3,
        "machine_complex" | "transit_cluster" => 0,
        _ if district == "SLUM" => 4,
        _ if district == "ELITE" => 3,
        _ => 1,
    }
}

fn contested_intensity(border: &DistrictBorderRecord, hazards: &[HazardZoneRecord]) -> f32 {
    let nearby_hazards = hazards
        .iter()
        .filter(|hazard| {
            hazard.bounds_min[0] <= border.bounds_max[0] + 2
                && hazard.bounds_max[0] + 2 >= border.bounds_min[0]
                && hazard.bounds_min[2] <= border.bounds_max[1] + 2
                && hazard.bounds_max[2] + 2 >= border.bounds_min[1]
        })
        .count();
    (0.35 + border.route_ids.len() as f32 * 0.08 + nearby_hazards as f32 * 0.12).clamp(0.0, 1.0)
}

fn contested_reason(feature: &str) -> &'static str {
    match feature {
        "SECURITY_THRESHOLD" => "restricted access checkpoint",
        "BORDER_MARKET" | "SCRAP_MARKET" => "trade revenue and salvage rights",
        "SCRAP_ZONE" => "scrap extraction and collapse access",
        "SURFACE_COMMONS" => "shared civic corridor",
        _ => "overlapping district claims",
    }
}

fn temporal_routes(phase: &str, routes: &[TransitEdgeRecord]) -> Vec<usize> {
    routes
        .iter()
        .filter(|route| match phase {
            "blackout" => matches!(route.role.as_str(), "service_loop" | "maintenance_backbone"),
            "market_peak" => route.role == "market_run" || route.kind == "ring_route",
            "patrol_cycle" => {
                matches!(route.role.as_str(), "restricted_spine" | "evacuation_route")
            }
            "ventilation_surge" => route.role == "service_loop",
            "rain_ingress" => {
                route.kind == "service_tunnel" || route.role == "maintenance_backbone"
            }
            _ => false,
        })
        .map(|route| route.id)
        .take(12)
        .collect()
}

fn temporal_flows(phase: &str, flows: &[InfrastructureFlowRecord]) -> Vec<usize> {
    flows
        .iter()
        .filter(|flow| match phase {
            "blackout" => matches!(flow.kind.as_str(), "power_bus" | "data_spine"),
            "market_peak" => flow.kind == "power_bus",
            "patrol_cycle" => flow.kind == "data_spine",
            "ventilation_surge" => flow.kind == "ventilation_loop",
            "rain_ingress" => matches!(flow.kind.as_str(), "water_reclamation" | "waste_chute"),
            _ => false,
        })
        .map(|flow| flow.id)
        .take(12)
        .collect()
}

fn temporal_hazards(phase: &str, hazards: &[HazardZoneRecord]) -> Vec<usize> {
    hazards
        .iter()
        .filter(|hazard| match phase {
            "blackout" => hazard.kind == "blackout_pocket",
            "market_peak" => hazard.kind == "unstable_span",
            "patrol_cycle" => hazard.kind == "security_sweep",
            "ventilation_surge" => hazard.kind == "vent_heat_plume",
            "rain_ingress" => hazard.kind == "flood_sump",
            _ => false,
        })
        .map(|hazard| hazard.id)
        .collect()
}

fn temporal_factions(
    phase: &str,
    factions: &[FactionRecord],
    contested_borders: &[ContestedBorderRecord],
) -> Vec<usize> {
    let preferred = match phase {
        "blackout" => "Scavenger Crews",
        "market_peak" => "Market Syndicates",
        "patrol_cycle" => "Corp Security",
        "ventilation_surge" | "rain_ingress" => "Civic Maintenance",
        _ => "Civic Maintenance",
    };
    let mut ids: Vec<_> = factions
        .iter()
        .filter(|faction| faction.name == preferred)
        .map(|faction| faction.id)
        .collect();
    if phase == "market_peak" {
        for border in contested_borders.iter().take(3) {
            for faction_id in &border.faction_ids {
                if !ids.contains(faction_id) {
                    ids.push(*faction_id);
                }
            }
        }
    }
    ids
}

fn route_civilian_density(edge: &TransitEdgeRecord, attachment_count: usize) -> f32 {
    let base = match edge.role.as_str() {
        "market_run" => 0.82,
        "primary_artery" => 0.70,
        "evacuation_route" => 0.58,
        "service_loop" => 0.34,
        "maintenance_backbone" => 0.22,
        "restricted_spine" => 0.16,
        _ => 0.40,
    };
    (base + attachment_count as f32 * 0.025 + edge.length as f32 * 0.002).clamp(0.0, 1.0)
}

fn route_security_pressure(edge: &TransitEdgeRecord, hazard_pressure: f32) -> f32 {
    let base = match edge.role.as_str() {
        "restricted_spine" => 0.88,
        "evacuation_route" => 0.62,
        "primary_artery" => 0.44,
        "market_run" => 0.34,
        _ => 0.22,
    };
    (base + hazard_pressure * 0.28).clamp(0.0, 1.0)
}

fn route_blackout_risk(
    edge: &TransitEdgeRecord,
    hazard_pressure: f32,
    temporal_state: &TemporalStateRecord,
) -> f32 {
    let blackout_phase_bonus = temporal_state
        .phases
        .iter()
        .any(|phase| phase.name == "blackout" && phase.active_route_ids.contains(&edge.id));
    let base = match edge.role.as_str() {
        "service_loop" | "maintenance_backbone" => 0.48,
        "market_run" => 0.36,
        _ => 0.20,
    };
    (base + hazard_pressure * 0.36 + if blackout_phase_bonus { 0.18 } else { 0.0 }).clamp(0.0, 1.0)
}

fn route_market_congestion(edge: &TransitEdgeRecord, attachment_count: usize) -> f32 {
    let base = match edge.role.as_str() {
        "market_run" => 0.82,
        "primary_artery" => 0.42,
        "service_loop" => 0.18,
        _ if edge.kind == "ring_route" => 0.40,
        _ => 0.24,
    };
    (base + attachment_count as f32 * 0.035).clamp(0.0, 1.0)
}

fn named_place(seed: u64, scope: &str, id: usize, kind: &str, position: [usize; 3]) -> String {
    let prefixes = [
        "West", "Blue", "K", "Low", "Cinder", "Glass", "Rain", "North",
    ];
    let nouns = match scope {
        "route" => [
            "Spine", "Run", "Lift", "Chain", "Artery", "Loop", "Crossing", "Rail",
        ],
        "cluster" => [
            "Stack", "Shrine", "Block", "Market", "Vault", "Commons", "Nest", "Court",
        ],
        "hazard" => [
            "Sump", "Scar", "Blackout", "Plume", "Break", "Sink", "Rift", "Drip",
        ],
        _ => [
            "Gate",
            "Border",
            "Seam",
            "Threshold",
            "Mouth",
            "Exchange",
            "Pass",
            "Line",
        ],
    };
    let prefix = prefixes[(seed as usize + id + position[0]) % prefixes.len()];
    let noun = nouns[(seed as usize + id * 3 + position[2]) % nouns.len()];
    let code = (position[0] * 3 + position[1] * 5 + position[2] * 7 + id) % 97;
    format!("{prefix} {noun} {code:02} {}", kind.to_ascii_lowercase())
}

fn faction_id_by_name(factions: &[FactionRecord], district: &str) -> Option<usize> {
    let target = faction_for_district_name(district)?;
    factions.get(target).map(|faction| faction.id)
}

fn entity_kind_for_cluster(cluster: &RoomClusterRecord) -> &'static str {
    match (cluster.kind.as_str(), cluster.owner_district.as_str()) {
        (kind, _) if kind.contains("market") || kind.contains("border") => "market_crowd",
        (_, "ELITE" | "COMMERCIAL") if cluster.kind.contains("data") => "corp_patrol",
        (_, "INDUSTRIAL") => "maintenance_crawler",
        (_, "SLUM") if cluster.kind.contains("shrine") => "scavenger_drift",
        (_, "SLUM") => "builder_swarm",
        (_, _) if cluster.kind.contains("transit") || cluster.kind.contains("route") => {
            "evacuee_flow"
        }
        _ => "market_crowd",
    }
}

fn movement_profile_for_entity(kind: &str) -> &'static str {
    match kind {
        "corp_patrol" => "looped_patrol",
        "evacuee_flow" => "fastest_safe_route",
        "maintenance_crawler" => "service_inspection",
        "builder_swarm" => "incremental_construction",
        "scavenger_drift" => "risk_tolerant_drift",
        _ => "attractor_weighted_flow",
    }
}

fn pressure_kind_for_entity(kind: &str) -> &'static str {
    match kind {
        "corp_patrol" => "patrol_lockdown",
        "evacuee_flow" => "evacuation_flow",
        "maintenance_crawler" => "maintenance_crawler",
        "builder_swarm" => "builder_swarm",
        "scavenger_drift" => "scavenger_drift",
        _ => "market_surge",
    }
}

fn layout_mutation_kind(pressure_kind: &str) -> &'static str {
    match pressure_kind {
        "market_surge" => "entity_market_widening",
        "patrol_lockdown" => "entity_security_lockdown",
        "evacuation_flow" => "entity_evacuation_bypass",
        "maintenance_crawler" => "entity_service_retrofit",
        "builder_swarm" => "entity_builder_expansion",
        _ => "entity_scavenger_scarring",
    }
}

fn active_phases_for_entity(kind: &str, temporal_state: &TemporalStateRecord) -> Vec<usize> {
    let preferred = match kind {
        "corp_patrol" => "patrol_cycle",
        "evacuee_flow" => "rain_ingress",
        "maintenance_crawler" => "ventilation_surge",
        "builder_swarm" => "market_peak",
        "scavenger_drift" => "blackout",
        _ => "market_peak",
    };
    let mut ids: Vec<_> = temporal_state
        .phases
        .iter()
        .filter(|phase| phase.name == preferred)
        .map(|phase| phase.id)
        .collect();
    if ids.is_empty() {
        ids.extend(temporal_state.phases.first().map(|phase| phase.id));
    }
    ids
}

fn phase_for_pressure(pressure_kind: &str, temporal_state: &TemporalStateRecord) -> Option<usize> {
    let preferred = match pressure_kind {
        "patrol_lockdown" => "patrol_cycle",
        "evacuation_flow" => "rain_ingress",
        "maintenance_crawler" => "ventilation_surge",
        "builder_swarm" | "market_surge" => "market_peak",
        _ => "blackout",
    };
    temporal_state
        .phases
        .iter()
        .find(|phase| phase.name == preferred)
        .map(|phase| phase.id)
}

fn entity_layout_influence(kind: &str, cluster: &RoomClusterRecord, route_ids: &[usize]) -> f32 {
    let base = match kind {
        "builder_swarm" => 0.75,
        "market_crowd" => 0.68,
        "evacuee_flow" => 0.62,
        "maintenance_crawler" => 0.58,
        "corp_patrol" => 0.54,
        _ => 0.46,
    };
    (base + cluster.room_ids.len() as f32 * 0.012 + route_ids.len() as f32 * 0.05).clamp(0.0, 1.0)
}

fn entity_path_pressure(
    route_ids: &[usize],
    route_simulation: &[RouteSimulationRecord],
) -> (f32, f32) {
    if route_ids.is_empty() {
        return (0.0, 0.0);
    }
    let mut congestion = 0.0;
    let mut risk = 0.0;
    let mut count = 0usize;
    for route_id in route_ids {
        let Some(simulation) = route_simulation.get(*route_id) else {
            continue;
        };
        congestion += simulation
            .market_congestion
            .max(simulation.civilian_density);
        risk += simulation
            .blackout_risk
            .max(simulation.security_pressure)
            .max(1.0 - simulation.evacuation_viability);
        count += 1;
    }
    if count == 0 {
        (0.0, 0.0)
    } else {
        (
            (congestion / count as f32).clamp(0.0, 1.0),
            (risk / count as f32).clamp(0.0, 1.0),
        )
    }
}

fn shortest_route_path(
    graph: &[Vec<(usize, f32)>],
    start: usize,
    goal: usize,
) -> (Vec<usize>, f32) {
    if start >= graph.len() || goal >= graph.len() {
        return (Vec::new(), f32::MAX);
    }
    if start == goal {
        return (vec![start], 0.0);
    }
    let mut distance = vec![f32::MAX; graph.len()];
    let mut previous = vec![None; graph.len()];
    let mut visited = vec![false; graph.len()];
    distance[start] = 0.0;

    for _ in 0..graph.len() {
        let Some(current) = (0..graph.len())
            .filter(|index| !visited[*index])
            .min_by(|a, b| distance[*a].total_cmp(&distance[*b]))
        else {
            break;
        };
        if !distance[current].is_finite() || current == goal {
            break;
        }
        visited[current] = true;
        for (next, cost) in &graph[current] {
            let next_distance = distance[current] + *cost;
            if next_distance < distance[*next] {
                distance[*next] = next_distance;
                previous[*next] = Some(current);
            }
        }
    }

    if !distance[goal].is_finite() {
        return (Vec::new(), f32::MAX);
    }
    let mut path = Vec::new();
    let mut current = goal;
    path.push(current);
    while let Some(prev) = previous[current] {
        current = prev;
        path.push(current);
        if current == start {
            break;
        }
    }
    path.reverse();
    if path.first().copied() == Some(start) {
        (path, distance[goal])
    } else {
        (Vec::new(), f32::MAX)
    }
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
        ("linear_express", _, _) => "LINEAR_STATION",
        ("station_loop", _, _) => "STATION_CONCOURSE",
        ("void_bridge", _, _) => "VOID_BRIDGE_NODE",
        ("marine_causeway", _, _) => "CAUSEWAY_LOCK",
        ("pylon_service", _, _) => "PYLON_PUMP_ROOM",
        ("rim_loop", _, _) => "RIM_HABITAT_NODE",
        ("spoke_transfer", _, _) => "SPOKE_TRANSFER",
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
    role: &str,
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
    match role {
        "maintenance_backbone" => return "PIPE_JUNCTION",
        "market_run" if district == DistrictType::Commercial => return "MARKET_STALL",
        "market_run" if district == DistrictType::Slum => return "PATCH_BAZAAR",
        "restricted_spine" => return "SKY_SECURITY_GATE",
        "evacuation_route" => return "SECURITY_GATE",
        _ => {}
    }
    match (kind, district, stratum) {
        ("linear_express", _, _) => "LINEAR_STATION",
        ("station_loop", _, _) => "STATION_CONCOURSE",
        ("void_bridge", _, _) => "VOID_BRIDGE_NODE",
        ("marine_causeway", _, _) => "CAUSEWAY_LOCK",
        ("pylon_service", _, _) => "PYLON_PUMP_ROOM",
        ("rim_loop", _, _) => "RIM_HABITAT_NODE",
        ("spoke_transfer", _, _) => "SPOKE_TRANSFER",
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

fn flow_kinds_for_role(role: &str) -> &'static [&'static str] {
    match role {
        "maintenance_backbone" => &["water_reclamation", "waste_chute", "ventilation_loop"],
        "restricted_spine" => &["data_spine", "power_bus"],
        "market_run" => &["power_bus", "water_reclamation"],
        "service_loop" => &["waste_chute", "ventilation_loop"],
        "evacuation_route" => &["power_bus", "ventilation_loop"],
        "primary_artery" => &["power_bus", "data_spine", "ventilation_loop"],
        _ => &["power_bus"],
    }
}

fn flow_intensity(kind: &str, role: &str, neon_intensity: f32) -> f32 {
    let base = match kind {
        "data_spine" => 0.85,
        "power_bus" => 0.75,
        "water_reclamation" => 0.55,
        "waste_chute" => 0.45,
        "ventilation_loop" => 0.50,
        _ => 0.40,
    };
    let role_bonus = match role {
        "restricted_spine" | "primary_artery" => 0.15,
        "maintenance_backbone" => 0.10,
        _ => 0.0,
    };
    (base + role_bonus + neon_intensity * 0.025).clamp(0.0, 1.0)
}

fn resource_capacity_for_kind(kind: &str) -> f32 {
    match kind {
        "power_bus" => 0.92,
        "data_spine" => 0.86,
        "water_reclamation" => 0.72,
        "waste_chute" => 0.66,
        "ventilation_loop" => 0.70,
        _ => 0.60,
    }
}

fn resource_outage_pressure(route_id: usize, networks: &[ResourceNetworkRecord]) -> f32 {
    networks
        .iter()
        .filter(|network| network.route_ids.contains(&route_id))
        .map(|network| {
            let overload = (network.load / network.capacity.max(0.1) - 1.0).max(0.0);
            if network.outage {
                0.55 + overload * 0.30
            } else {
                overload * 0.20
            }
        })
        .fold(0.0, f32::max)
        .clamp(0.0, 1.0)
}

fn manhattan(a: [usize; 3], b: [usize; 3]) -> usize {
    a[0].abs_diff(b[0]) + a[1].abs_diff(b[1]) + a[2].abs_diff(b[2])
}

fn stress_kind_for_route(route_kind: &str) -> &'static str {
    match route_kind {
        "void_bridge" => "bridge_span_stress",
        "marine_causeway" | "pylon_service" => "pylon_grid_stress",
        "rim_loop" | "spoke_transfer" => "orbital_frame_stress",
        "cliff_gallery" => "cliff_face_stress",
        "dam_wall_spine" => "dam_wall_stress",
        "drydock_spine" => "drydock_frame_stress",
        "runway_spine" => "runway_deck_stress",
        "caldera_ring" | "geothermal_shaft" => "caldera_thermal_stress",
        "crevasse_bridge" | "meltwater_spine" => "ice_shelf_fracture_stress",
        "canopy_walk" | "root_service" => "canopy_trunk_stress",
        "tether_core" | "cargo_ring" | "ground_anchor" => "tether_anchor_stress",
        "crawler_track" | "engine_spine" | "convoy_deck" => "crawler_chassis_stress",
        "reef_ring" | "lagoon_causeway" => "reef_pylon_stress",
        "pressure_deck" | "lift_cell_spine" => "stratosphere_lift_stress",
        "sinkhole_ring" | "descent_shaft" => "sinkhole_rim_stress",
        _ => "route_frame_stress",
    }
}

fn stress_hazard_kind_for_edge(edge: &TransitEdgeRecord) -> &'static str {
    match edge.kind.as_str() {
        "void_bridge" | "skybridge" => "stress_bridge_failure",
        "marine_causeway" | "pylon_service" => "stress_pylon_failure",
        "rim_loop" | "spoke_transfer" => "stress_spoke_shear",
        "dam_wall_spine" | "turbine_gallery" => "stress_wall_seepage",
        "drydock_spine" | "gantry_loop" => "stress_gantry_failure",
        "runway_spine" | "terminal_loop" => "stress_deck_crack",
        "cavern_loop" | "hive_gallery" | "hive_trunk" => "stress_cavern_shift",
        "cliff_gallery" | "burrow_spine" => "stress_slope_shear",
        "caldera_ring" | "geothermal_shaft" => "stress_geothermal_breach",
        "crevasse_bridge" | "meltwater_spine" => "stress_ice_fracture",
        "canopy_walk" | "root_service" => "stress_canopy_failure",
        "tether_core" | "cargo_ring" | "ground_anchor" => "stress_tether_shear",
        "crawler_track" | "engine_spine" | "convoy_deck" => "stress_track_failure",
        "reef_ring" | "lagoon_causeway" => "stress_pylon_scour",
        "pressure_deck" | "lift_cell_spine" => "stress_lift_cell_failure",
        "sinkhole_ring" | "descent_shaft" => "stress_rim_collapse",
        _ => "stress_frame_overload",
    }
}

fn route_failure_pressure(route_id: usize, failure_zones: &[FailurePropagationRecord]) -> f32 {
    failure_zones
        .iter()
        .filter(|failure| failure.affected_route_ids.contains(&route_id))
        .map(|failure| failure.severity)
        .fold(0.0, f32::max)
}

fn micro_detail_kind(edge: &TransitEdgeRecord, index: usize) -> &'static str {
    match edge.role.as_str() {
        "restricted_spine" => {
            if index.is_multiple_of(2) {
                "signage"
            } else {
                "barricade"
            }
        }
        "market_run" => "signage",
        "maintenance_backbone" => "leak",
        "service_loop" => "vent_cluster",
        "evacuation_route" => "barricade",
        _ if edge.stratum == "SKYLINE" => "antenna_cluster",
        _ => "cable_bundle",
    }
}

fn hazard_probability_for_role(role: &str) -> f32 {
    match role {
        "maintenance_backbone" => 0.50,
        "service_loop" => 0.40,
        "market_run" => 0.30,
        "restricted_spine" => 0.24,
        "primary_artery" => 0.22,
        "evacuation_route" => 0.18,
        _ => 0.20,
    }
}

fn hazard_kind_for_edge(edge: &TransitEdgeRecord) -> &'static str {
    match edge.role.as_str() {
        "maintenance_backbone" => "flood_sump",
        "service_loop" => "vent_heat_plume",
        "restricted_spine" => "security_sweep",
        "market_run" => "blackout_pocket",
        _ if edge.kind == "skybridge" => "unstable_span",
        _ => "blackout_pocket",
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
    fn older_dynamic_exports_migrate_to_current_schema() {
        let saved = generated("ABCD1234").saved_structure();
        let mut value: serde_json::Value =
            serde_json::from_str(&structure::to_json(&saved).unwrap()).unwrap();
        value["metadata"]["schema_version"] = serde_json::json!("gibson.structure.v17");
        if let Some(metadata) = value["metadata"].as_object_mut() {
            metadata.remove("typology");
            metadata.remove("entity_count");
            metadata.remove("entity_path_count");
            metadata.remove("entity_pressure_field_count");
            metadata.remove("layout_mutation_count");
        }
        if let Some(config) = value["metadata"]["config"].as_object_mut() {
            config.remove("typology");
            config.remove("entity_density");
            config.remove("entity_layout_pressure");
            config.remove("advanced_pattern_complexity");
        }
        if let Some(object) = value.as_object_mut() {
            object.remove("typology_frame");
            object.remove("typology_quality");
            object.remove("entities");
            object.remove("entity_paths");
            object.remove("entity_pressure_fields");
            object.remove("layout_mutations");
        }
        if let Some(rule_packs) = value["rule_packs"].as_array_mut() {
            for pack in rule_packs {
                if let Some(pack) = pack.as_object_mut() {
                    pack.remove("entity_density_weight");
                    pack.remove("entity_layout_weight");
                    pack.remove("patrol_weight");
                    pack.remove("crowd_weight");
                    pack.remove("builder_weight");
                }
            }
        }

        let migrated = structure::from_json(&serde_json::to_string(&value).unwrap()).unwrap();
        assert_eq!(migrated.metadata.schema_version, STRUCTURE_SCHEMA_VERSION);
        assert_eq!(
            migrated.metadata.typology,
            MegastructureTypology::DenseEnclave.as_str()
        );
        assert_eq!(
            migrated.typology_frame.typology,
            MegastructureTypology::DenseEnclave.as_str()
        );
        assert_eq!(
            migrated.typology_quality.typology,
            MegastructureTypology::DenseEnclave.as_str()
        );
        assert!(migrated.typology_quality.score >= 0.0);
        assert!(migrated.entities.is_empty());
        assert!(migrated.entity_paths.is_empty());
        assert_eq!(migrated.metadata.entity_count, 0);
        assert!(migrated.rule_packs.iter().all(|pack| {
            pack.entity_density_weight == 1.0
                && pack.entity_layout_weight == 1.0
                && pack.patrol_weight == 1.0
                && pack.crowd_weight == 1.0
                && pack.builder_weight == 1.0
        }));
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
    fn typologies_generate_distinct_valid_macro_forms() {
        for typology in MegastructureTypology::all() {
            let mut config = GenerationConfig::profile(crate::config::GenerationProfile::Balanced);
            config.typology = typology;
            let saved = generated_with("ABCD1234", config).saved_structure();
            assert_eq!(saved.metadata.typology, typology.as_str());
            assert_eq!(saved.typology_frame.typology, typology.as_str());
            assert!(!saved.typology_frame.traversal_contract.is_empty());
            assert!(!saved.transit_graph.edges.is_empty());
            if typology != MegastructureTypology::DenseEnclave {
                assert!(saved
                    .macro_massing
                    .iter()
                    .any(|massing| massing.kind.starts_with("typology_")));
            }
        }
    }

    #[test]
    fn typology_route_contracts_emit_native_route_roles() {
        for (typology, route_kind) in [
            (MegastructureTypology::LinearCity, "linear_express"),
            (MegastructureTypology::BridgeVoid, "void_bridge"),
            (MegastructureTypology::MarinePlatform, "marine_causeway"),
            (MegastructureTypology::OrbitalRing, "rim_loop"),
            (MegastructureTypology::UndergroundHive, "hive_trunk"),
            (MegastructureTypology::MountainBurrow, "cliff_gallery"),
            (MegastructureTypology::DesertArcology, "climate_spine"),
            (MegastructureTypology::AirportCity, "runway_spine"),
            (MegastructureTypology::DamCity, "dam_wall_spine"),
            (MegastructureTypology::ShipyardStack, "drydock_spine"),
            (MegastructureTypology::VolcanicCaldera, "caldera_ring"),
            (MegastructureTypology::IceShelfCity, "meltwater_spine"),
            (MegastructureTypology::CanopyBabel, "canopy_walk"),
            (MegastructureTypology::SpaceElevatorAnchor, "tether_core"),
            (MegastructureTypology::CrawlerCity, "crawler_track"),
            (MegastructureTypology::ReefAtollArcology, "reef_ring"),
            (MegastructureTypology::StratospherePlatform, "pressure_deck"),
            (MegastructureTypology::SinkholeCitadel, "sinkhole_ring"),
        ] {
            let mut config = GenerationConfig::profile(crate::config::GenerationProfile::Balanced);
            config.typology = typology;
            let saved = generated_with("ABCD1234", config).saved_structure();
            assert!(
                saved
                    .transit_graph
                    .edges
                    .iter()
                    .any(|edge| edge.kind == route_kind),
                "{typology} did not emit {route_kind}"
            );
        }
    }

    #[test]
    fn typologies_emit_native_hazards_and_quality_metrics() {
        for (typology, hazard_kind) in [
            (MegastructureTypology::ArcologySpire, "core_lockdown"),
            (MegastructureTypology::LinearCity, "station_crush"),
            (MegastructureTypology::BridgeVoid, "bridge_failure"),
            (MegastructureTypology::MarinePlatform, "storm_surge"),
            (MegastructureTypology::OrbitalRing, "pressure_breach"),
            (MegastructureTypology::UndergroundHive, "cavern_collapse"),
            (MegastructureTypology::MountainBurrow, "rockfall_choke"),
            (MegastructureTypology::DesertArcology, "heat_bloom"),
            (MegastructureTypology::AirportCity, "runway_debris"),
            (MegastructureTypology::DamCity, "spillway_surge"),
            (MegastructureTypology::ShipyardStack, "drydock_flood"),
            (MegastructureTypology::VolcanicCaldera, "lava_tube_breach"),
            (MegastructureTypology::IceShelfCity, "thermal_fracture"),
            (MegastructureTypology::CanopyBabel, "canopy_fire"),
            (MegastructureTypology::SpaceElevatorAnchor, "tether_shear"),
            (MegastructureTypology::CrawlerCity, "track_collapse"),
            (MegastructureTypology::ReefAtollArcology, "reef_bleach"),
            (MegastructureTypology::StratospherePlatform, "lift_cell_leak"),
            (MegastructureTypology::SinkholeCitadel, "rim_rockfall"),
        ] {
            let mut config = GenerationConfig::profile(crate::config::GenerationProfile::Balanced);
            config.typology = typology;
            let saved = generated_with("ABCD1234", config).saved_structure();
            assert_eq!(saved.typology_quality.typology, typology.as_str());
            assert!(saved.typology_quality.score >= 0.65);
            assert!(!saved.construction_history.is_empty());
            assert!(saved.section_quality.score >= 0.45);
            assert!(!saved.structural_system.stress_fields.is_empty());
            assert!(
                saved
                    .hazard_zones
                    .iter()
                    .any(|hazard| hazard.kind == hazard_kind),
                "{typology} did not emit {hazard_kind}"
            );
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
            assert!(matches!(
                edge.role.as_str(),
                "primary_artery"
                    | "service_loop"
                    | "restricted_spine"
                    | "evacuation_route"
                    | "market_run"
                    | "maintenance_backbone"
            ));
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
        assert_eq!(
            saved.metadata.district_lifecycle_count,
            saved.district_lifecycle.len()
        );
        assert_eq!(saved.district_lifecycle.len(), saved.districts.len());
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
            assert!(district.age_years > 0);
            assert!((0.0..=1.0).contains(&district.maintenance_level));
            assert!((0.0..=1.0).contains(&district.occupancy_pressure));
            assert!((0.0..=1.0).contains(&district.control_stability));
            assert!(!district.dominant_grammar.is_empty());
            assert!((0.0..=1.0).contains(&district.occupied_ratio));
        }
        for lifecycle in &saved.district_lifecycle {
            assert!(lifecycle.age_years > 0);
            assert!((0.0..=1.0).contains(&lifecycle.maintenance_level));
            assert!((0.0..=1.0).contains(&lifecycle.occupancy_pressure));
            assert!((0.0..=1.0).contains(&lifecycle.control_stability));
            assert!(lifecycle.decay_bias > 0.0);
            assert!(lifecycle.repair_bias > 0.0);
            assert!(lifecycle.security_bias > 0.0);
            assert!(lifecycle.density_bias > 0.0);
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
    fn semantic_hierarchy_exports_borders_clusters_and_path_analysis() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(
            saved.metadata.district_border_count,
            saved.district_borders.len()
        );
        assert_eq!(saved.metadata.room_cluster_count, saved.room_clusters.len());
        assert!(!saved.district_borders.is_empty());
        assert!(!saved.room_clusters.is_empty());
        assert!(saved.path_analysis.connected_component_count > 0);
        assert!(saved.path_analysis.largest_component_edges > 0);
        assert!(saved.path_analysis.reachable_room_count > 0);
        assert!(saved.path_analysis.alternate_path_count > 0);
        assert!(saved.path_analysis.vertical_transfer_count > 0);
        assert!(saved.path_analysis.guaranteed_service_to_skyline);
        assert!(saved.path_analysis.route_redundancy_score > 0.0);
        assert!(saved.path_analysis.reachable_landmark_count > 0);
        assert!(saved.path_analysis.faction_territory_connectivity > 0.0);
        assert!(saved.path_analysis.main_path_room_reachability > 0.0);
        assert!(saved.path_analysis.quality_score > 0.0);
        assert!(!saved.path_analysis.high_centrality_route_ids.is_empty());
        assert!(saved.path_analysis.main_path.is_some());

        for border in &saved.district_borders {
            assert!(border.bounds_max[0] < saved.size);
            assert!(border.bounds_max[1] < saved.size);
            assert!(border.y < saved.layers);
            assert!(!border.feature.is_empty());
            for room_id in &border.room_ids {
                assert!(*room_id < saved.rooms.len());
            }
            for route_id in &border.route_ids {
                assert!(*route_id < saved.transit_graph.edges.len());
            }
        }

        for cluster in &saved.room_clusters {
            assert!(!cluster.room_ids.is_empty());
            assert!(cluster.bounds_max[0] < saved.size);
            assert!(cluster.bounds_max[1] < saved.layers);
            assert!(cluster.bounds_max[2] < saved.size);
            for room_id in &cluster.room_ids {
                assert_eq!(saved.rooms[*room_id].cluster_id, Some(cluster.id));
            }
            for route_id in &cluster.route_ids {
                assert!(*route_id < saved.transit_graph.edges.len());
            }
        }
    }

    #[test]
    fn topology_quality_pass_exports_strict_generation_guarantees() {
        let saved = generated("ABCD1234").saved_structure();
        assert!(saved.path_analysis.connected_component_count <= 2);
        assert!(saved.path_analysis.alternate_path_count >= 3);
        assert!(saved.path_analysis.vertical_transfer_count >= 3);
        assert!(saved.path_analysis.guaranteed_service_to_skyline);
        assert!(saved.path_analysis.route_redundancy_score >= 0.75);
        assert!(saved.path_analysis.reachable_landmark_count >= 8);
        assert!(saved.path_analysis.faction_territory_connectivity >= 0.5);
        assert_eq!(saved.path_analysis.main_path_room_reachability, 1.0);
        assert!(saved.path_analysis.quality_score >= 0.75);
    }

    #[test]
    fn landmark_aware_generation_shapes_surrounding_geometry() {
        let saved = generated("ABCD1234").saved_structure();
        assert!(saved.metadata.pattern_counts.contains_key("landmark_plaza"));
        assert!(saved
            .metadata
            .pattern_counts
            .contains_key("landmark_hazard_scar"));
        assert!(saved
            .metadata
            .pattern_counts
            .iter()
            .any(|(name, count)| { name.starts_with("landmark_") && *count > 0 }));
    }

    #[test]
    fn multi_scale_generation_exports_macro_meso_and_micro_records() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(
            saved.metadata.macro_massing_count,
            saved.macro_massing.len()
        );
        assert_eq!(
            saved.metadata.meso_placement_count,
            saved.meso_placements.len()
        );
        assert_eq!(saved.metadata.micro_detail_count, saved.micro_details.len());
        assert!(!saved.macro_massing.is_empty());
        assert!(!saved.meso_placements.is_empty());
        assert!(!saved.micro_details.is_empty());
        assert!(saved
            .micro_details
            .iter()
            .all(|detail| (0.0..=1.0).contains(&detail.intensity)));
        assert!(
            saved.metadata.pattern_counts.contains_key("macro_void")
                || saved
                    .metadata
                    .pattern_counts
                    .contains_key("macro_density_spine")
        );
    }

    #[test]
    fn infrastructure_flows_and_hazards_add_generation_depth() {
        let saved = generated_with(
            "ABCD1234",
            GenerationConfig::profile(crate::config::GenerationProfile::Decayed),
        )
        .saved_structure();
        assert_eq!(
            saved.metadata.infrastructure_flow_count,
            saved.infrastructure_flows.len()
        );
        assert_eq!(saved.metadata.hazard_zone_count, saved.hazard_zones.len());
        assert!(!saved.infrastructure_flows.is_empty());
        assert!(!saved.hazard_zones.is_empty());
        assert!(saved.infrastructure_flows.iter().any(|flow| matches!(
            flow.kind.as_str(),
            "power_bus" | "data_spine" | "water_reclamation" | "waste_chute" | "ventilation_loop"
        )));
        for flow in &saved.infrastructure_flows {
            assert!(flow.route_id < saved.transit_graph.edges.len());
            assert!((0.0..=1.0).contains(&flow.intensity));
            assert!(!flow.sample_points.is_empty());
            for point in &flow.sample_points {
                assert!(point[0] < saved.size);
                assert!(point[1] < saved.layers);
                assert!(point[2] < saved.size);
            }
        }
        for hazard in &saved.hazard_zones {
            assert!((0.0..=1.0).contains(&hazard.severity));
            assert!(hazard.bounds_max[0] < saved.size);
            assert!(hazard.bounds_max[1] < saved.layers);
            assert!(hazard.bounds_max[2] < saved.size);
            for route_id in &hazard.route_ids {
                assert!(*route_id < saved.transit_graph.edges.len());
            }
            for room_id in &hazard.room_ids {
                assert!(*room_id < saved.rooms.len());
            }
        }
    }

    #[test]
    fn structural_system_exports_stability_ratings() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(
            saved.metadata.structural_rating_count,
            saved.structural_system.stability_ratings.len()
        );
        assert_eq!(
            saved.metadata.load_bearing_frame_count,
            saved.structural_system.load_bearing_frames.len()
        );
        assert_eq!(
            saved.metadata.suspended_deck_count,
            saved.structural_system.suspended_decks.len()
        );
        assert!(!saved.structural_system.stability_ratings.is_empty());
        assert!(saved
            .structural_system
            .support_dependency_summary
            .contains_key("load_bearing_frames"));
        assert_eq!(saved.metadata.failure_zone_count, saved.failure_zones.len());
        assert!(saved
            .structural_system
            .support_dependency_summary
            .contains_key("failure_zones"));
        assert!(saved
            .structural_system
            .stability_ratings
            .iter()
            .any(|rating| rating.target_type == "route"));
        for rating in &saved.structural_system.stability_ratings {
            assert!((0.0..=1.0).contains(&rating.rating));
            assert!((0.0..=1.0).contains(&rating.cantilever_risk));
            assert!(!rating.support_dependency.is_empty());
        }
    }

    #[test]
    fn structural_failures_propagate_to_routes_and_quality() {
        let saved = generated("ABCD1234").saved_structure();
        assert!(!saved.failure_zones.is_empty());
        assert!(saved
            .failure_zones
            .iter()
            .all(|failure| (0.0..=1.0).contains(&failure.severity)));
        assert!(saved
            .failure_zones
            .iter()
            .any(|failure| !failure.affected_route_ids.is_empty()));
        assert!(saved.path_analysis.quality_score <= 1.0);
    }

    #[test]
    fn procedural_rule_packs_are_exported_and_weight_generation() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(saved.metadata.rule_pack_count, saved.rule_packs.len());
        assert_eq!(saved.rule_packs.len(), 20);
        assert!(saved
            .rule_packs
            .iter()
            .any(|pack| pack.district == "SLUM" && pack.density_weight > 1.0));
        assert!(saved
            .rule_packs
            .iter()
            .all(|pack| pack.detail_weight > 0.0 && pack.decay_weight > 0.0));
        assert_eq!(
            saved.metadata.rule_influence_count,
            saved.rule_influences.len()
        );
        assert!(saved
            .rule_influences
            .iter()
            .any(|influence| influence.target_type == "route"));
    }

    #[test]
    fn external_rule_packs_override_matching_generation_weights() {
        let rules =
            crate::rules::CompiledRulePackSet::from_json_file("rules/kowloon-decay.json").unwrap();
        let saved = generate_saved_structure_with_rules(
            "ABCD1234".to_owned(),
            GenerationConfig::profile(crate::config::GenerationProfile::Decayed),
            rules,
        )
        .unwrap();
        assert!(saved
            .rule_packs
            .iter()
            .any(|pack| pack.name == "kowloon_decay_slum_surface"));
        assert!(saved.rule_influences.iter().any(|influence| {
            influence.rule_pack_name == "kowloon_decay_slum_surface"
                && influence
                    .grammar
                    .iter()
                    .any(|rule| rule.contains("corridors"))
        }));
    }

    #[test]
    fn ownership_layer_exports_factions_and_contested_borders() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(saved.metadata.faction_count, saved.factions.len());
        assert_eq!(saved.metadata.territory_count, saved.territories.len());
        assert_eq!(
            saved.metadata.contested_border_count,
            saved.contested_borders.len()
        );
        assert_eq!(saved.factions.len(), 5);
        assert!(!saved.territories.is_empty());
        assert!(!saved.contested_borders.is_empty());
        assert!(saved
            .factions
            .iter()
            .any(|faction| faction.name == "Corp Security"));
        for faction in &saved.factions {
            assert!((0.0..=1.0).contains(&faction.influence));
            assert!(!faction.agenda.is_empty());
        }
        for territory in &saved.territories {
            assert!(territory.faction_id < saved.factions.len());
            assert!((0.0..=1.0).contains(&territory.hazard_pressure));
            if let Some(cluster_id) = territory.cluster_id {
                assert!(cluster_id < saved.room_clusters.len());
            }
        }
        for border in &saved.contested_borders {
            assert!(border.border_id < saved.district_borders.len());
            assert!((0.0..=1.0).contains(&border.intensity));
            assert!(border
                .faction_ids
                .iter()
                .all(|faction_id| *faction_id < saved.factions.len()));
        }
    }

    #[test]
    fn temporal_state_exports_power_cycle_phases() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(
            saved.metadata.temporal_phase_count,
            saved.temporal_state.phases.len()
        );
        assert_eq!(saved.temporal_state.phases.len(), 5);
        for expected in [
            "blackout",
            "market_peak",
            "patrol_cycle",
            "ventilation_surge",
            "rain_ingress",
        ] {
            assert!(saved
                .temporal_state
                .phases
                .iter()
                .any(|phase| phase.name == expected));
        }
        for phase in &saved.temporal_state.phases {
            assert!(phase.cycle_hour < 24);
            assert!(!phase.description.is_empty());
            assert!(phase
                .active_route_ids
                .iter()
                .all(|route_id| *route_id < saved.transit_graph.edges.len()));
            assert!(phase
                .active_flow_ids
                .iter()
                .all(|flow_id| *flow_id < saved.infrastructure_flows.len()));
            assert!(phase
                .affected_hazard_ids
                .iter()
                .all(|hazard_id| *hazard_id < saved.hazard_zones.len()));
            assert!(phase
                .active_faction_ids
                .iter()
                .all(|faction_id| *faction_id < saved.factions.len()));
        }
    }

    #[test]
    fn route_simulation_exports_flow_pressure_and_viability() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(
            saved.metadata.route_simulation_count,
            saved.route_simulation.len()
        );
        assert_eq!(
            saved.route_simulation.len(),
            saved.transit_graph.edges.len()
        );
        assert!(saved
            .route_simulation
            .iter()
            .any(|simulation| simulation.market_congestion > 0.5));
        assert!(saved
            .route_simulation
            .iter()
            .any(|simulation| !simulation.active_phase_ids.is_empty()));
        for simulation in &saved.route_simulation {
            assert!(simulation.route_id < saved.transit_graph.edges.len());
            assert!((0.0..=1.0).contains(&simulation.civilian_density));
            assert!((0.0..=1.0).contains(&simulation.security_pressure));
            assert!((0.0..=1.0).contains(&simulation.blackout_risk));
            assert!((0.0..=1.0).contains(&simulation.market_congestion));
            assert!((0.0..=1.0).contains(&simulation.evacuation_viability));
        }
    }

    #[test]
    fn resource_networks_export_load_outage_and_reroute_state() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(
            saved.metadata.resource_network_count,
            saved.resource_networks.len()
        );
        assert!(!saved.resource_networks.is_empty());
        assert!(saved
            .resource_networks
            .iter()
            .any(|network| network.overloaded || network.outage));
        for network in &saved.resource_networks {
            assert!(network.capacity > 0.0);
            assert!(network.load >= 0.0);
            assert!(network
                .route_ids
                .iter()
                .all(|route_id| *route_id < saved.transit_graph.edges.len()));
        }
    }

    #[test]
    fn entity_dynamics_export_paths_pressure_and_layout_mutations() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(saved.metadata.entity_count, saved.entities.len());
        assert_eq!(saved.metadata.entity_path_count, saved.entity_paths.len());
        assert_eq!(
            saved.metadata.entity_pressure_field_count,
            saved.entity_pressure_fields.len()
        );
        assert_eq!(
            saved.metadata.layout_mutation_count,
            saved.layout_mutations.len()
        );
        assert!(!saved.entities.is_empty());
        assert!(!saved.entity_paths.is_empty());
        assert!(!saved.entity_pressure_fields.is_empty());
        assert!(!saved.layout_mutations.is_empty());
        assert!(saved
            .metadata
            .pattern_counts
            .contains_key("cellular_activity_field"));
        assert!(saved
            .layout_mutations
            .iter()
            .any(|mutation| mutation.kind.starts_with("entity_")));
        for entity in &saved.entities {
            assert_eq!(entity.id, saved.entities[entity.id].id);
            assert!(entity.origin[0] < saved.size);
            assert!(entity.destination[1] < saved.layers);
            assert!(!entity.route_ids.is_empty());
            assert!((0.0..=1.0).contains(&entity.layout_influence));
        }
        for path in &saved.entity_paths {
            assert!(path.entity_id < saved.entities.len());
            assert!(!path.sample_points.is_empty());
            assert!(path.reaches_destination);
            assert!((0.0..=1.0).contains(&path.congestion));
            assert!((0.0..=1.0).contains(&path.risk));
        }
        for field in &saved.entity_pressure_fields {
            assert!((0.0..=1.0).contains(&field.intensity));
            assert!(!field.source_entity_ids.is_empty());
            assert!(field.bounds_max[0] < saved.size);
            assert!(field.bounds_max[1] < saved.layers);
            assert!(field.bounds_max[2] < saved.size);
        }
    }

    #[test]
    fn entity_controls_can_disable_dynamic_sections_independently() {
        let no_entities = generated_with(
            "ABCD1234",
            GenerationConfig {
                entity_density: 0.0,
                ..GenerationConfig::default()
            },
        )
        .saved_structure();
        assert!(no_entities.entities.is_empty());
        assert!(no_entities.entity_paths.is_empty());
        assert!(no_entities.entity_pressure_fields.is_empty());
        assert!(no_entities.layout_mutations.is_empty());

        let no_mutations = generated_with(
            "ABCD1234",
            GenerationConfig {
                entity_layout_pressure: 0.0,
                ..GenerationConfig::default()
            },
        )
        .saved_structure();
        assert!(!no_mutations.entities.is_empty());
        assert!(!no_mutations.entity_pressure_fields.is_empty());
        assert!(no_mutations.layout_mutations.is_empty());
    }

    #[test]
    fn narrative_landmarks_name_generated_places() {
        let saved = generated("ABCD1234").saved_structure();
        assert_eq!(
            saved.metadata.narrative_landmark_count,
            saved.narrative_landmarks.len()
        );
        assert!(!saved.narrative_landmarks.is_empty());
        assert!(saved
            .narrative_landmarks
            .iter()
            .any(|landmark| landmark.route_id.is_some()));
        assert!(saved
            .narrative_landmarks
            .iter()
            .any(|landmark| landmark.cluster_id.is_some()));
        for landmark in &saved.narrative_landmarks {
            assert!(!landmark.name.is_empty());
            assert!(!landmark.description.is_empty());
            assert!(landmark.position[0] < saved.size);
            assert!(landmark.position[1] < saved.layers);
            assert!(landmark.position[2] < saved.size);
            if let Some(route_id) = landmark.route_id {
                assert!(route_id < saved.transit_graph.edges.len());
            }
            if let Some(cluster_id) = landmark.cluster_id {
                assert!(cluster_id < saved.room_clusters.len());
            }
            if let Some(hazard_id) = landmark.hazard_id {
                assert!(hazard_id < saved.hazard_zones.len());
            }
            if let Some(border_id) = landmark.border_id {
                assert!(border_id < saved.district_borders.len());
            }
            if let Some(faction_id) = landmark.faction_id {
                assert!(faction_id < saved.factions.len());
            }
        }
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
            "typology_frame",
            "typology_quality",
            "grid",
            "connections",
            "rooms",
            "transit_graph",
            "districts",
            "district_lifecycle",
            "strata",
            "macro_massing",
            "meso_placements",
            "micro_details",
            "district_borders",
            "room_clusters",
            "path_analysis",
            "infrastructure_flows",
            "route_simulation",
            "resource_networks",
            "hazard_zones",
            "structural_system",
            "failure_zones",
            "rule_packs",
            "rule_influences",
            "factions",
            "territories",
            "contested_borders",
            "temporal_state",
            "narrative_landmarks",
            "entities",
            "entity_paths",
            "entity_pressure_fields",
            "layout_mutations",
        ] {
            assert!(value.get(key).is_some(), "missing top-level key {key}");
        }
        for key in [
            "typology",
            "route_density",
            "landmark_frequency",
            "decay_story_density",
            "district_contrast",
            "strata_separation",
            "entity_density",
            "entity_layout_pressure",
            "advanced_pattern_complexity",
        ] {
            assert!(
                value["metadata"]["config"].get(key).is_some(),
                "missing config key {key}"
            );
        }
        for key in [
            "typology",
            "transit_node_count",
            "transit_edge_count",
            "transit_attachment_count",
            "district_record_count",
            "district_lifecycle_count",
            "stratum_record_count",
            "macro_massing_count",
            "meso_placement_count",
            "micro_detail_count",
            "district_border_count",
            "room_cluster_count",
            "infrastructure_flow_count",
            "route_simulation_count",
            "resource_network_count",
            "hazard_zone_count",
            "structural_rating_count",
            "load_bearing_frame_count",
            "suspended_deck_count",
            "failure_zone_count",
            "rule_pack_count",
            "rule_influence_count",
            "faction_count",
            "territory_count",
            "contested_border_count",
            "temporal_phase_count",
            "narrative_landmark_count",
            "entity_count",
            "entity_path_count",
            "entity_pressure_field_count",
            "layout_mutation_count",
        ] {
            assert!(
                value["metadata"].get(key).is_some(),
                "missing metadata key {key}"
            );
        }
        assert!(value["transit_graph"]["edges"][0].get("role").is_some());
        assert!(value["rooms"][0].get("cluster_id").is_some());
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
