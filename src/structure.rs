use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::config::GenerationConfig;

pub const CURRENT_SEED_FILE: &str = "current_seed.txt";
pub const STRUCTURE_FILE: &str = "structure.json";
pub const STRUCTURE_SCHEMA_VERSION: &str = "gibson.structure.v8";

pub type StructureResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConnectionRecord {
    pub kind: String,
    pub start: [usize; 3],
    pub end: [usize; 3],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RoomRecord {
    pub id: usize,
    pub cluster_id: Option<usize>,
    pub position: [usize; 3],
    pub district: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TransitGraphRecord {
    pub nodes: Vec<TransitNodeRecord>,
    pub edges: Vec<TransitEdgeRecord>,
    pub attachments: Vec<TransitAttachmentRecord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TransitNodeRecord {
    pub id: usize,
    pub kind: String,
    pub position: [usize; 3],
    pub district: String,
    pub stratum: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TransitEdgeRecord {
    pub id: usize,
    pub kind: String,
    pub role: String,
    pub start_node: usize,
    pub end_node: usize,
    pub points: Vec<[usize; 3]>,
    pub length: usize,
    pub stratum: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TransitAttachmentRecord {
    pub route_id: usize,
    pub room_id: usize,
    pub attachment_kind: String,
    pub position: [usize; 3],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DistrictRecord {
    pub id: usize,
    pub kind: String,
    pub bounds_min: [usize; 2],
    pub bounds_max: [usize; 2],
    pub footprint_cells: usize,
    pub occupied_cells: usize,
    pub occupied_ratio: f32,
    pub dominant_grammar: String,
    pub generated_features: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StratumRecord {
    pub id: usize,
    pub name: String,
    pub y_min: usize,
    pub y_max: usize,
    pub cell_count: usize,
    pub occupied_cells: usize,
    pub occupied_ratio: f32,
    pub dominant_grammar: String,
    pub generated_features: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DistrictBorderRecord {
    pub id: usize,
    pub from_district: String,
    pub to_district: String,
    pub bounds_min: [usize; 2],
    pub bounds_max: [usize; 2],
    pub y: usize,
    pub feature: String,
    pub route_ids: Vec<usize>,
    pub room_ids: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RoomClusterRecord {
    pub id: usize,
    pub kind: String,
    pub owner_district: String,
    pub stratum: String,
    pub bounds_min: [usize; 3],
    pub bounds_max: [usize; 3],
    pub anchor_position: [usize; 3],
    pub room_ids: Vec<usize>,
    pub route_ids: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MissionPathRecord {
    pub label: String,
    pub route_ids: Vec<usize>,
    pub room_ids: Vec<usize>,
    pub start: [usize; 3],
    pub end: [usize; 3],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PathAnalysisRecord {
    pub connected_component_count: usize,
    pub largest_component_edges: usize,
    pub dead_end_count: usize,
    pub chokepoint_count: usize,
    pub reachable_room_count: usize,
    pub high_centrality_route_ids: Vec<usize>,
    pub main_path: Option<MissionPathRecord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InfrastructureFlowRecord {
    pub id: usize,
    pub kind: String,
    pub route_id: usize,
    pub intensity: f32,
    pub source: [usize; 3],
    pub sink: [usize; 3],
    pub sample_points: Vec<[usize; 3]>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HazardZoneRecord {
    pub id: usize,
    pub kind: String,
    pub severity: f32,
    pub bounds_min: [usize; 3],
    pub bounds_max: [usize; 3],
    pub route_ids: Vec<usize>,
    pub room_ids: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SavedStructure {
    pub seed: String,
    pub size: usize,
    pub layers: usize,
    pub metadata: StructureMetadata,
    pub grid: Vec<Vec<Vec<u8>>>,
    pub connections: Vec<ConnectionRecord>,
    pub rooms: Vec<RoomRecord>,
    pub transit_graph: TransitGraphRecord,
    pub districts: Vec<DistrictRecord>,
    pub strata: Vec<StratumRecord>,
    pub district_borders: Vec<DistrictBorderRecord>,
    pub room_clusters: Vec<RoomClusterRecord>,
    pub path_analysis: PathAnalysisRecord,
    pub infrastructure_flows: Vec<InfrastructureFlowRecord>,
    pub hazard_zones: Vec<HazardZoneRecord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StructureMetadata {
    pub schema_version: String,
    pub profile: String,
    pub config: GenerationConfig,
    pub district_counts: BTreeMap<String, usize>,
    pub stratum_counts: BTreeMap<String, usize>,
    pub cell_counts: BTreeMap<String, usize>,
    pub material_counts: BTreeMap<String, usize>,
    pub connection_counts: BTreeMap<String, usize>,
    pub room_counts: BTreeMap<String, usize>,
    pub pattern_counts: BTreeMap<String, usize>,
    pub room_count: usize,
    pub connection_count: usize,
    pub transit_node_count: usize,
    pub transit_edge_count: usize,
    pub transit_attachment_count: usize,
    pub district_record_count: usize,
    pub stratum_record_count: usize,
    pub district_border_count: usize,
    pub room_cluster_count: usize,
    pub infrastructure_flow_count: usize,
    pub hazard_zone_count: usize,
    pub occupied_cell_ratio: f32,
}

pub fn to_json(structure: &SavedStructure) -> serde_json::Result<String> {
    serde_json::to_string_pretty(structure)
}

pub fn from_json(json: &str) -> serde_json::Result<SavedStructure> {
    serde_json::from_str(json)
}

pub fn save_structure(path: impl AsRef<Path>, structure: &SavedStructure) -> StructureResult<()> {
    let json = to_json(structure)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn load_structure(path: impl AsRef<Path>) -> StructureResult<SavedStructure> {
    let json = fs::read_to_string(path)?;
    Ok(from_json(&json)?)
}

pub fn save_outputs(
    directory: impl AsRef<Path>,
    seed: &str,
    structure: &SavedStructure,
) -> StructureResult<()> {
    let directory = directory.as_ref();
    fs::write(directory.join(CURRENT_SEED_FILE), seed)?;
    save_structure(directory.join(STRUCTURE_FILE), structure)
}
