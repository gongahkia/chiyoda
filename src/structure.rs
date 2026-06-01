use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::config::{GenerationConfig, MegastructureTypology};

pub const CURRENT_SEED_FILE: &str = "current_seed.txt";
pub const STRUCTURE_FILE: &str = "structure.json";
pub const STRUCTURE_SCHEMA_VERSION: &str = "gibson.structure.v22";

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
    pub age_years: usize,
    pub maintenance_level: f32,
    pub occupancy_pressure: f32,
    pub control_stability: f32,
    pub dominant_grammar: String,
    pub generated_features: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DistrictLifecycleRecord {
    pub district: String,
    pub age_years: usize,
    pub maintenance_level: f32,
    pub occupancy_pressure: f32,
    pub control_stability: f32,
    pub decay_bias: f32,
    pub repair_bias: f32,
    pub security_bias: f32,
    pub density_bias: f32,
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
pub struct MacroMassingRecord {
    pub id: usize,
    pub kind: String,
    pub bounds_min: [usize; 3],
    pub bounds_max: [usize; 3],
    pub district: String,
    pub void_ratio: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TypologyBandRecord {
    pub kind: String,
    pub bounds_min: [usize; 3],
    pub bounds_max: [usize; 3],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TypologyFrameRecord {
    pub typology: String,
    pub primary_axes: Vec<String>,
    pub primary_spines: Vec<[usize; 3]>,
    pub void_bands: Vec<TypologyBandRecord>,
    pub habitat_bands: Vec<TypologyBandRecord>,
    pub service_anchors: Vec<[usize; 3]>,
    pub traversal_contract: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MesoPlacementRecord {
    pub id: usize,
    pub kind: String,
    pub route_id: Option<usize>,
    pub cluster_id: Option<usize>,
    pub anchor: [usize; 3],
    pub influence_radius: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MicroDetailRecord {
    pub id: usize,
    pub kind: String,
    pub position: [usize; 3],
    pub route_id: Option<usize>,
    pub intensity: f32,
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
    pub alternate_path_count: usize,
    pub vertical_transfer_count: usize,
    pub guaranteed_service_to_skyline: bool,
    pub route_redundancy_score: f32,
    pub reachable_landmark_count: usize,
    pub faction_territory_connectivity: f32,
    pub main_path_room_reachability: f32,
    pub quality_score: f32,
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
pub struct RouteSimulationRecord {
    pub route_id: usize,
    pub civilian_density: f32,
    pub security_pressure: f32,
    pub blackout_risk: f32,
    pub market_congestion: f32,
    pub evacuation_viability: f32,
    pub active_phase_ids: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ResourceNetworkRecord {
    pub id: usize,
    pub kind: String,
    pub source: [usize; 3],
    pub sink: [usize; 3],
    pub route_ids: Vec<usize>,
    pub capacity: f32,
    pub load: f32,
    pub overloaded: bool,
    pub outage: bool,
    pub reroute_route_ids: Vec<usize>,
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
pub struct TypologyQualityRecord {
    pub typology: String,
    pub score: f32,
    pub contract_scores: BTreeMap<String, f32>,
    pub required_route_kinds: Vec<String>,
    pub missing_contracts: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConstructionEraRecord {
    pub id: usize,
    pub era: String,
    pub age_years: usize,
    pub material_bias: String,
    pub decay_bias: f32,
    pub affected_districts: Vec<String>,
    pub affected_route_ids: Vec<usize>,
    pub affected_room_ids: Vec<usize>,
    pub generated_scars: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StabilityRatingRecord {
    pub target_type: String,
    pub target_id: String,
    pub rating: f32,
    pub load_bearing_frames: usize,
    pub foundation_cells: usize,
    pub suspended_decks: usize,
    pub cantilever_risk: f32,
    pub support_dependency: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StructuralSystemRecord {
    pub load_bearing_frames: Vec<[usize; 3]>,
    pub foundation_zones: Vec<[usize; 2]>,
    pub suspended_decks: Vec<[usize; 3]>,
    pub support_dependency_summary: BTreeMap<String, usize>,
    pub stress_fields: Vec<StressFieldRecord>,
    pub load_paths: Vec<LoadPathRecord>,
    pub stability_ratings: Vec<StabilityRatingRecord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StressFieldRecord {
    pub id: usize,
    pub kind: String,
    pub stress: f32,
    pub bounds_min: [usize; 3],
    pub bounds_max: [usize; 3],
    pub route_ids: Vec<usize>,
    pub support_points: Vec<[usize; 3]>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LoadPathRecord {
    pub id: usize,
    pub kind: String,
    pub from: [usize; 3],
    pub to: [usize; 3],
    pub route_ids: Vec<usize>,
    pub stress: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FailurePropagationRecord {
    pub id: usize,
    pub origin: [usize; 3],
    pub radius: usize,
    pub severity: f32,
    pub affected_route_ids: Vec<usize>,
    pub affected_deck_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RulePackRecord {
    pub id: usize,
    pub name: String,
    #[serde(default)]
    pub typology: Option<String>,
    pub district: String,
    pub stratum: String,
    pub profile: String,
    pub density_weight: f32,
    pub route_weight: f32,
    pub decay_weight: f32,
    pub detail_weight: f32,
    pub entity_density_weight: f32,
    pub entity_layout_weight: f32,
    pub patrol_weight: f32,
    pub crowd_weight: f32,
    pub builder_weight: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RuleInfluenceRecord {
    pub id: usize,
    pub target_type: String,
    pub target_id: String,
    pub rule_pack_id: usize,
    pub rule_pack_name: String,
    pub district: String,
    pub stratum: String,
    pub grammar: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FactionRecord {
    pub id: usize,
    pub name: String,
    pub agenda: String,
    pub influence: f32,
    pub controlled_districts: Vec<String>,
    pub controlled_cluster_ids: Vec<usize>,
    pub controlled_route_ids: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerritoryRecord {
    pub id: usize,
    pub faction_id: usize,
    pub kind: String,
    pub district: Option<String>,
    pub cluster_id: Option<usize>,
    pub route_ids: Vec<usize>,
    pub hazard_pressure: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContestedBorderRecord {
    pub border_id: usize,
    pub faction_ids: Vec<usize>,
    pub intensity: f32,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TemporalPhaseRecord {
    pub id: usize,
    pub name: String,
    pub cycle_hour: usize,
    pub active_route_ids: Vec<usize>,
    pub active_flow_ids: Vec<usize>,
    pub affected_hazard_ids: Vec<usize>,
    pub active_faction_ids: Vec<usize>,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TemporalStateRecord {
    pub cycle_seed: u64,
    pub phases: Vec<TemporalPhaseRecord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NarrativeLandmarkRecord {
    pub id: usize,
    pub name: String,
    pub kind: String,
    pub position: [usize; 3],
    pub route_id: Option<usize>,
    pub cluster_id: Option<usize>,
    pub hazard_id: Option<usize>,
    pub border_id: Option<usize>,
    pub faction_id: Option<usize>,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EntityRecord {
    pub id: usize,
    pub kind: String,
    pub faction_id: Option<usize>,
    pub home_cluster_id: Option<usize>,
    pub origin: [usize; 3],
    pub destination: [usize; 3],
    pub route_ids: Vec<usize>,
    pub active_phase_ids: Vec<usize>,
    pub movement_profile: String,
    pub layout_influence: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EntityPathRecord {
    pub id: usize,
    pub entity_id: usize,
    pub sample_points: Vec<[usize; 3]>,
    pub route_ids: Vec<usize>,
    pub travel_cost: f32,
    pub congestion: f32,
    pub risk: f32,
    pub reaches_destination: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EntityPressureFieldRecord {
    pub id: usize,
    pub kind: String,
    pub bounds_min: [usize; 3],
    pub bounds_max: [usize; 3],
    pub intensity: f32,
    pub source_entity_ids: Vec<usize>,
    pub affected_route_ids: Vec<usize>,
    pub affected_room_ids: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LayoutMutationRecord {
    pub id: usize,
    pub kind: String,
    pub phase_id: Option<usize>,
    pub bounds_min: [usize; 3],
    pub bounds_max: [usize; 3],
    pub source_pressure_field_id: usize,
    pub affected_route_ids: Vec<usize>,
    pub affected_room_ids: Vec<usize>,
    pub added_cell_count: usize,
    pub removed_cell_count: usize,
    pub sample_points: Vec<[usize; 3]>,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SectionQualityRecord {
    pub score: f32,
    pub vertical_continuity: f32,
    pub void_exposure: f32,
    pub service_separation: f32,
    pub evacuation_shaft_coverage: f32,
    pub habitable_layer_ratio: f32,
    pub roof_deck_access: f32,
    pub cross_section_route_density: f32,
    pub missing_contracts: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SavedStructure {
    pub seed: String,
    pub size: usize,
    pub layers: usize,
    pub metadata: StructureMetadata,
    pub typology_frame: TypologyFrameRecord,
    pub typology_quality: TypologyQualityRecord,
    pub construction_history: Vec<ConstructionEraRecord>,
    pub section_quality: SectionQualityRecord,
    pub grid: Vec<Vec<Vec<u8>>>,
    pub connections: Vec<ConnectionRecord>,
    pub rooms: Vec<RoomRecord>,
    pub transit_graph: TransitGraphRecord,
    pub districts: Vec<DistrictRecord>,
    pub district_lifecycle: Vec<DistrictLifecycleRecord>,
    pub strata: Vec<StratumRecord>,
    pub macro_massing: Vec<MacroMassingRecord>,
    pub meso_placements: Vec<MesoPlacementRecord>,
    pub micro_details: Vec<MicroDetailRecord>,
    pub district_borders: Vec<DistrictBorderRecord>,
    pub room_clusters: Vec<RoomClusterRecord>,
    pub path_analysis: PathAnalysisRecord,
    pub infrastructure_flows: Vec<InfrastructureFlowRecord>,
    pub route_simulation: Vec<RouteSimulationRecord>,
    pub resource_networks: Vec<ResourceNetworkRecord>,
    pub hazard_zones: Vec<HazardZoneRecord>,
    pub structural_system: StructuralSystemRecord,
    pub failure_zones: Vec<FailurePropagationRecord>,
    pub rule_packs: Vec<RulePackRecord>,
    pub rule_influences: Vec<RuleInfluenceRecord>,
    pub factions: Vec<FactionRecord>,
    pub territories: Vec<TerritoryRecord>,
    pub contested_borders: Vec<ContestedBorderRecord>,
    pub temporal_state: TemporalStateRecord,
    pub narrative_landmarks: Vec<NarrativeLandmarkRecord>,
    pub entities: Vec<EntityRecord>,
    pub entity_paths: Vec<EntityPathRecord>,
    pub entity_pressure_fields: Vec<EntityPressureFieldRecord>,
    pub layout_mutations: Vec<LayoutMutationRecord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StructureMetadata {
    pub schema_version: String,
    pub profile: String,
    pub typology: String,
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
    pub district_lifecycle_count: usize,
    pub stratum_record_count: usize,
    pub macro_massing_count: usize,
    pub meso_placement_count: usize,
    pub micro_detail_count: usize,
    pub district_border_count: usize,
    pub room_cluster_count: usize,
    pub infrastructure_flow_count: usize,
    pub route_simulation_count: usize,
    pub resource_network_count: usize,
    pub hazard_zone_count: usize,
    pub structural_rating_count: usize,
    pub load_bearing_frame_count: usize,
    pub suspended_deck_count: usize,
    pub stress_field_count: usize,
    pub load_path_count: usize,
    pub failure_zone_count: usize,
    pub construction_era_count: usize,
    pub rule_pack_count: usize,
    pub rule_influence_count: usize,
    pub faction_count: usize,
    pub territory_count: usize,
    pub contested_border_count: usize,
    pub temporal_phase_count: usize,
    pub narrative_landmark_count: usize,
    pub entity_count: usize,
    pub entity_path_count: usize,
    pub entity_pressure_field_count: usize,
    pub layout_mutation_count: usize,
    pub occupied_cell_ratio: f32,
}

pub fn to_json(structure: &SavedStructure) -> serde_json::Result<String> {
    serde_json::to_string_pretty(structure)
}

pub fn from_json(json: &str) -> serde_json::Result<SavedStructure> {
    let mut value: serde_json::Value = serde_json::from_str(json)?;
    migrate_structure_value(&mut value);
    serde_json::from_value(value)
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

fn migrate_structure_value(value: &mut serde_json::Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let schema_version = object
        .get("metadata")
        .and_then(|metadata| metadata.get("schema_version"))
        .and_then(|version| version.as_str())
        .unwrap_or_default()
        .to_owned();
    if !matches!(
        schema_version.as_str(),
        "gibson.structure.v17"
            | "gibson.structure.v18"
            | "gibson.structure.v19"
            | "gibson.structure.v20"
            | "gibson.structure.v21"
            | STRUCTURE_SCHEMA_VERSION
    ) {
        return;
    }

    object
        .entry("entities".to_owned())
        .or_insert_with(|| serde_json::json!([]));
    object
        .entry("entity_paths".to_owned())
        .or_insert_with(|| serde_json::json!([]));
    object
        .entry("entity_pressure_fields".to_owned())
        .or_insert_with(|| serde_json::json!([]));
    object
        .entry("layout_mutations".to_owned())
        .or_insert_with(|| serde_json::json!([]));

    if let Some(metadata) = object
        .get_mut("metadata")
        .and_then(serde_json::Value::as_object_mut)
    {
        metadata.insert(
            "schema_version".to_owned(),
            serde_json::json!(STRUCTURE_SCHEMA_VERSION),
        );
        metadata
            .entry("typology".to_owned())
            .or_insert_with(|| serde_json::json!(MegastructureTypology::DenseEnclave.as_str()));
        metadata
            .entry("entity_count".to_owned())
            .or_insert_with(|| serde_json::json!(0));
        metadata
            .entry("entity_path_count".to_owned())
            .or_insert_with(|| serde_json::json!(0));
        metadata
            .entry("entity_pressure_field_count".to_owned())
            .or_insert_with(|| serde_json::json!(0));
        metadata
            .entry("layout_mutation_count".to_owned())
            .or_insert_with(|| serde_json::json!(0));
        metadata
            .entry("stress_field_count".to_owned())
            .or_insert_with(|| serde_json::json!(0));
        metadata
            .entry("load_path_count".to_owned())
            .or_insert_with(|| serde_json::json!(0));
        metadata
            .entry("construction_era_count".to_owned())
            .or_insert_with(|| serde_json::json!(0));
        if let Some(config) = metadata
            .get_mut("config")
            .and_then(serde_json::Value::as_object_mut)
        {
            config
                .entry("entity_density".to_owned())
                .or_insert_with(|| serde_json::json!(1.0));
            config
                .entry("entity_layout_pressure".to_owned())
                .or_insert_with(|| serde_json::json!(1.0));
            config
                .entry("advanced_pattern_complexity".to_owned())
                .or_insert_with(|| serde_json::json!(1.0));
            config
                .entry("typology".to_owned())
                .or_insert_with(|| serde_json::json!(MegastructureTypology::DenseEnclave));
        }
    }

    object
        .entry("typology_frame".to_owned())
        .or_insert_with(default_typology_frame_value);
    let typology = object
        .get("metadata")
        .and_then(|metadata| metadata.get("typology"))
        .and_then(|typology| typology.as_str())
        .unwrap_or_else(|| MegastructureTypology::DenseEnclave.as_str())
        .to_owned();
    object
        .entry("typology_quality".to_owned())
        .or_insert_with(|| default_typology_quality_value(&typology));
    object
        .entry("construction_history".to_owned())
        .or_insert_with(|| serde_json::json!([]));
    object
        .entry("section_quality".to_owned())
        .or_insert_with(default_section_quality_value);

    if let Some(structural_system) = object
        .get_mut("structural_system")
        .and_then(serde_json::Value::as_object_mut)
    {
        structural_system
            .entry("stress_fields".to_owned())
            .or_insert_with(|| serde_json::json!([]));
        structural_system
            .entry("load_paths".to_owned())
            .or_insert_with(|| serde_json::json!([]));
    }

    if let Some(rule_packs) = object
        .get_mut("rule_packs")
        .and_then(serde_json::Value::as_array_mut)
    {
        for rule_pack in rule_packs {
            if let Some(rule_pack) = rule_pack.as_object_mut() {
                insert_neutral_rule_weights(rule_pack);
                rule_pack
                    .entry("typology".to_owned())
                    .or_insert(serde_json::Value::Null);
            }
        }
    }
}

fn default_section_quality_value() -> serde_json::Value {
    serde_json::json!({
        "score": 1.0,
        "vertical_continuity": 1.0,
        "void_exposure": 0.0,
        "service_separation": 1.0,
        "evacuation_shaft_coverage": 1.0,
        "habitable_layer_ratio": 1.0,
        "roof_deck_access": 1.0,
        "cross_section_route_density": 1.0,
        "missing_contracts": []
    })
}

fn default_typology_quality_value(typology: &str) -> serde_json::Value {
    serde_json::json!({
        "typology": typology,
        "score": 1.0,
        "contract_scores": {},
        "required_route_kinds": [],
        "missing_contracts": []
    })
}

fn default_typology_frame_value() -> serde_json::Value {
    serde_json::json!({
        "typology": MegastructureTypology::DenseEnclave.as_str(),
        "primary_axes": ["x", "z", "y"],
        "primary_spines": [],
        "void_bands": [],
        "habitat_bands": [],
        "service_anchors": [],
        "traversal_contract": ["legacy dense enclave traversal"]
    })
}

fn insert_neutral_rule_weights(rule_pack: &mut serde_json::Map<String, serde_json::Value>) {
    for key in [
        "entity_density_weight",
        "entity_layout_weight",
        "patrol_weight",
        "crowd_weight",
        "builder_weight",
    ] {
        rule_pack
            .entry(key.to_owned())
            .or_insert_with(|| serde_json::json!(1.0));
    }
}
