use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::config::GenerationConfig;

pub const CURRENT_SEED_FILE: &str = "current_seed.txt";
pub const STRUCTURE_FILE: &str = "structure.json";
pub const STRUCTURE_SCHEMA_VERSION: &str = "gibson.structure.v5";

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
pub struct SavedStructure {
    pub seed: String,
    pub size: usize,
    pub layers: usize,
    pub metadata: StructureMetadata,
    pub grid: Vec<Vec<Vec<u8>>>,
    pub connections: Vec<ConnectionRecord>,
    pub rooms: Vec<RoomRecord>,
    pub transit_graph: TransitGraphRecord,
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
