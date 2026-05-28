use std::fs;
use std::path::Path;

use crate::config::MegastructureTypology;
use crate::scenario::{self, ScenarioRecord};
use crate::structure::{self, SavedStructure, StructureResult, STRUCTURE_SCHEMA_VERSION};
use std::str::FromStr;

const SCENARIO_SCHEMA_VERSION: &str = "gibson.scenario.v6";
const MAX_CELL_ID: u8 = 11;

pub fn validate_file(path: impl AsRef<Path>) -> StructureResult<Vec<String>> {
    let path = path.as_ref();
    let json = fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&json)?;

    if value
        .get("metadata")
        .and_then(|metadata| metadata.get("schema_version"))
        .is_some()
    {
        let structure = structure::from_json(&json)?;
        validate_structure(&structure)?;
        return Ok(vec![format!(
            "valid structure {} seed={} profile={} typology={}",
            structure.metadata.schema_version,
            structure.seed,
            structure.metadata.profile,
            structure.metadata.typology
        )]);
    }

    if value
        .get("schema_version")
        .and_then(|version| version.as_str())
        == Some(SCENARIO_SCHEMA_VERSION)
    {
        let scenario = scenario::from_json(&json)?;
        validate_scenario(&scenario)?;
        return Ok(vec![format!(
            "valid scenario {} seed={} title={}",
            scenario.schema_version, scenario.seed, scenario.title
        )]);
    }

    Err("unrecognized Gibson artifact schema".into())
}

pub fn validate_structure(structure: &SavedStructure) -> StructureResult<()> {
    ensure(
        structure.metadata.schema_version == STRUCTURE_SCHEMA_VERSION,
        format!(
            "unsupported structure schema {}, expected {}",
            structure.metadata.schema_version, STRUCTURE_SCHEMA_VERSION
        ),
    )?;
    ensure(
        structure.grid.len() == structure.size,
        "grid x dimension mismatch",
    )?;
    ensure(
        structure.metadata.typology == structure.typology_frame.typology,
        "typology metadata/frame mismatch",
    )?;
    ensure(
        structure.metadata.typology == structure.typology_quality.typology,
        "typology metadata/quality mismatch",
    )?;
    ensure(
        (0.0..=1.0).contains(&structure.typology_quality.score),
        "typology quality score out of range",
    )?;
    ensure(
        structure.typology_quality.score >= 0.65,
        "typology quality score below minimum",
    )?;
    ensure(
        (0.0..=1.0).contains(&structure.section_quality.score),
        "section quality score out of range",
    )?;
    ensure(
        structure.section_quality.score >= 0.45,
        "section quality score below minimum",
    )?;
    ensure(
        !structure.construction_history.is_empty(),
        "construction history is empty",
    )?;
    ensure(
        structure.construction_history.len() == structure.metadata.construction_era_count,
        "construction era count mismatch",
    )?;
    for value in structure.typology_quality.contract_scores.values() {
        ensure(
            (0.0..=1.0).contains(value),
            "typology contract score out of range",
        )?;
    }
    ensure(
        !structure.typology_frame.traversal_contract.is_empty(),
        "typology frame traversal contract is empty",
    )?;
    for point in structure
        .typology_frame
        .primary_spines
        .iter()
        .chain(structure.typology_frame.service_anchors.iter())
    {
        ensure_point(*point, structure.size, structure.layers, "typology frame")?;
    }
    for band in structure
        .typology_frame
        .void_bands
        .iter()
        .chain(structure.typology_frame.habitat_bands.iter())
    {
        ensure(!band.kind.is_empty(), "typology band kind is empty")?;
        ensure_point(
            band.bounds_min,
            structure.size,
            structure.layers,
            "typology band",
        )?;
        ensure_point(
            band.bounds_max,
            structure.size,
            structure.layers,
            "typology band",
        )?;
    }
    for (x, column) in structure.grid.iter().enumerate() {
        ensure(
            column.len() == structure.size,
            format!("grid z dimension mismatch at x={x}"),
        )?;
        for (z, stack) in column.iter().enumerate() {
            ensure(
                stack.len() == structure.layers,
                format!("grid y dimension mismatch at x={x}, z={z}"),
            )?;
            for cell in stack {
                ensure(
                    *cell <= MAX_CELL_ID,
                    format!("invalid cell id {cell} at x={x}, z={z}"),
                )?;
            }
        }
    }

    for room in &structure.rooms {
        ensure(
            room.id < structure.rooms.len(),
            format!("invalid room id {}", room.id),
        )?;
        ensure_point(room.position, structure.size, structure.layers, "room")?;
    }
    ensure(
        structure.entities.len() == structure.metadata.entity_count,
        "entity count mismatch",
    )?;
    ensure(
        structure.entity_paths.len() == structure.metadata.entity_path_count,
        "entity path count mismatch",
    )?;
    ensure(
        structure.entity_pressure_fields.len() == structure.metadata.entity_pressure_field_count,
        "entity pressure field count mismatch",
    )?;
    ensure(
        structure.layout_mutations.len() == structure.metadata.layout_mutation_count,
        "layout mutation count mismatch",
    )?;
    ensure(
        structure.district_lifecycle.len() == structure.districts.len(),
        "district lifecycle count must match district records",
    )?;
    for lifecycle in &structure.district_lifecycle {
        for value in [
            lifecycle.maintenance_level,
            lifecycle.occupancy_pressure,
            lifecycle.control_stability,
        ] {
            ensure(
                (0.0..=1.0).contains(&value),
                "district lifecycle metric out of range",
            )?;
        }
        for value in [
            lifecycle.decay_bias,
            lifecycle.repair_bias,
            lifecycle.security_bias,
            lifecycle.density_bias,
        ] {
            ensure(value > 0.0, "district lifecycle bias must be positive")?;
        }
    }
    for massing in &structure.macro_massing {
        ensure_point(
            massing.bounds_min,
            structure.size,
            structure.layers,
            "macro massing",
        )?;
        ensure_point(
            massing.bounds_max,
            structure.size,
            structure.layers,
            "macro massing",
        )?;
        ensure(
            (0.0..=1.0).contains(&massing.void_ratio),
            "macro void ratio out of range",
        )?;
    }
    for placement in &structure.meso_placements {
        ensure_point(
            placement.anchor,
            structure.size,
            structure.layers,
            "meso placement",
        )?;
        if let Some(route_id) = placement.route_id {
            ensure(
                route_id < structure.transit_graph.edges.len(),
                "meso placement invalid route",
            )?;
        }
        if let Some(cluster_id) = placement.cluster_id {
            ensure(
                cluster_id < structure.room_clusters.len(),
                "meso placement invalid cluster",
            )?;
        }
    }
    for detail in &structure.micro_details {
        ensure_point(
            detail.position,
            structure.size,
            structure.layers,
            "micro detail",
        )?;
        ensure(
            (0.0..=1.0).contains(&detail.intensity),
            "micro detail intensity out of range",
        )?;
    }
    for edge in &structure.transit_graph.edges {
        ensure(
            edge.id < structure.transit_graph.edges.len(),
            format!("invalid route id {}", edge.id),
        )?;
        ensure(
            edge.start_node < structure.transit_graph.nodes.len()
                && edge.end_node < structure.transit_graph.nodes.len(),
            format!("route {} references invalid nodes", edge.id),
        )?;
        ensure(
            !edge.points.is_empty(),
            format!("route {} has no points", edge.id),
        )?;
        for point in &edge.points {
            ensure_point(*point, structure.size, structure.layers, "route")?;
        }
    }
    validate_typology_contracts(structure)?;
    for attachment in &structure.transit_graph.attachments {
        ensure(
            attachment.route_id < structure.transit_graph.edges.len(),
            format!(
                "attachment references invalid route {}",
                attachment.route_id
            ),
        )?;
        ensure(
            attachment.room_id < structure.rooms.len(),
            format!("attachment references invalid room {}", attachment.room_id),
        )?;
    }
    ensure(
        structure.route_simulation.len() == structure.transit_graph.edges.len(),
        "route simulation count must match route edge count",
    )?;
    for simulation in &structure.route_simulation {
        ensure(
            simulation.route_id < structure.transit_graph.edges.len(),
            format!(
                "simulation references invalid route {}",
                simulation.route_id
            ),
        )?;
        for value in [
            simulation.civilian_density,
            simulation.security_pressure,
            simulation.blackout_risk,
            simulation.market_congestion,
            simulation.evacuation_viability,
        ] {
            ensure(
                (0.0..=1.0).contains(&value),
                "simulation metric out of range",
            )?;
        }
    }
    for entity in &structure.entities {
        ensure(entity.id < structure.entities.len(), "invalid entity id")?;
        ensure_point(
            entity.origin,
            structure.size,
            structure.layers,
            "entity origin",
        )?;
        ensure_point(
            entity.destination,
            structure.size,
            structure.layers,
            "entity destination",
        )?;
        ensure(
            (0.0..=1.0).contains(&entity.layout_influence),
            "entity layout influence out of range",
        )?;
        if let Some(faction_id) = entity.faction_id {
            ensure(
                faction_id < structure.factions.len(),
                "entity invalid faction",
            )?;
        }
        if let Some(cluster_id) = entity.home_cluster_id {
            ensure(
                cluster_id < structure.room_clusters.len(),
                "entity invalid home cluster",
            )?;
        }
        for route_id in &entity.route_ids {
            ensure(
                *route_id < structure.transit_graph.edges.len(),
                "entity invalid route",
            )?;
        }
        for phase_id in &entity.active_phase_ids {
            ensure(
                *phase_id < structure.temporal_state.phases.len(),
                "entity invalid phase",
            )?;
        }
    }
    for path in &structure.entity_paths {
        ensure(
            path.id < structure.entity_paths.len(),
            "invalid entity path id",
        )?;
        ensure(
            path.entity_id < structure.entities.len(),
            "entity path invalid entity",
        )?;
        ensure(!path.sample_points.is_empty(), "entity path has no samples")?;
        ensure(
            (0.0..=1.0).contains(&path.congestion) && (0.0..=1.0).contains(&path.risk),
            "entity path pressure out of range",
        )?;
        ensure(path.travel_cost >= 0.0, "entity path travel cost negative")?;
        for point in &path.sample_points {
            ensure_point(*point, structure.size, structure.layers, "entity path")?;
        }
        for route_id in &path.route_ids {
            ensure(
                *route_id < structure.transit_graph.edges.len(),
                "entity path invalid route",
            )?;
        }
    }
    for field in &structure.entity_pressure_fields {
        ensure(
            field.id < structure.entity_pressure_fields.len(),
            "invalid pressure field id",
        )?;
        ensure_point(
            field.bounds_min,
            structure.size,
            structure.layers,
            "pressure field",
        )?;
        ensure_point(
            field.bounds_max,
            structure.size,
            structure.layers,
            "pressure field",
        )?;
        ensure(
            (0.0..=1.0).contains(&field.intensity),
            "pressure field intensity out of range",
        )?;
        for entity_id in &field.source_entity_ids {
            ensure(
                *entity_id < structure.entities.len(),
                "field invalid entity",
            )?;
        }
        for route_id in &field.affected_route_ids {
            ensure(
                *route_id < structure.transit_graph.edges.len(),
                "field invalid route",
            )?;
        }
        for room_id in &field.affected_room_ids {
            ensure(*room_id < structure.rooms.len(), "field invalid room")?;
        }
    }
    for mutation in &structure.layout_mutations {
        ensure(
            mutation.id < structure.layout_mutations.len(),
            "invalid layout mutation id",
        )?;
        ensure_point(
            mutation.bounds_min,
            structure.size,
            structure.layers,
            "layout mutation",
        )?;
        ensure_point(
            mutation.bounds_max,
            structure.size,
            structure.layers,
            "layout mutation",
        )?;
        ensure(
            mutation.source_pressure_field_id < structure.entity_pressure_fields.len(),
            "layout mutation invalid pressure field",
        )?;
        if let Some(phase_id) = mutation.phase_id {
            ensure(
                phase_id < structure.temporal_state.phases.len(),
                "layout mutation invalid phase",
            )?;
        }
        for point in &mutation.sample_points {
            ensure_point(*point, structure.size, structure.layers, "layout mutation")?;
        }
    }
    for network in &structure.resource_networks {
        ensure(
            network.capacity > 0.0,
            "resource network capacity must be positive",
        )?;
        ensure(
            network.load >= 0.0,
            "resource network load cannot be negative",
        )?;
        ensure_point(
            network.source,
            structure.size,
            structure.layers,
            "resource source",
        )?;
        ensure_point(
            network.sink,
            structure.size,
            structure.layers,
            "resource sink",
        )?;
        for route_id in network
            .route_ids
            .iter()
            .chain(network.reroute_route_ids.iter())
        {
            ensure(
                *route_id < structure.transit_graph.edges.len(),
                "resource network references invalid route",
            )?;
        }
    }
    for failure in &structure.failure_zones {
        ensure_point(
            failure.origin,
            structure.size,
            structure.layers,
            "failure origin",
        )?;
        ensure(
            (0.0..=1.0).contains(&failure.severity),
            "failure severity out of range",
        )?;
        for route_id in &failure.affected_route_ids {
            ensure(
                *route_id < structure.transit_graph.edges.len(),
                "failure zone references invalid route",
            )?;
        }
    }
    ensure(
        structure.structural_system.stress_fields.len() == structure.metadata.stress_field_count,
        "stress field count mismatch",
    )?;
    ensure(
        structure.structural_system.load_paths.len() == structure.metadata.load_path_count,
        "load path count mismatch",
    )?;
    for field in &structure.structural_system.stress_fields {
        ensure(
            field.id < structure.structural_system.stress_fields.len(),
            "invalid stress field id",
        )?;
        ensure(
            (0.0..=1.0).contains(&field.stress),
            "stress field out of range",
        )?;
        ensure_point(
            field.bounds_min,
            structure.size,
            structure.layers,
            "stress field",
        )?;
        ensure_point(
            field.bounds_max,
            structure.size,
            structure.layers,
            "stress field",
        )?;
        for route_id in &field.route_ids {
            ensure(
                *route_id < structure.transit_graph.edges.len(),
                "stress field invalid route",
            )?;
        }
    }
    for path in &structure.structural_system.load_paths {
        ensure(
            path.id < structure.structural_system.load_paths.len(),
            "invalid load path id",
        )?;
        ensure(
            (0.0..=1.0).contains(&path.stress),
            "load path stress out of range",
        )?;
        ensure_point(path.from, structure.size, structure.layers, "load path")?;
        ensure_point(path.to, structure.size, structure.layers, "load path")?;
    }
    ensure(
        !structure.rule_packs.is_empty(),
        "no applied rule packs exported",
    )?;
    for rule_pack in &structure.rule_packs {
        for value in [
            rule_pack.density_weight,
            rule_pack.route_weight,
            rule_pack.decay_weight,
            rule_pack.detail_weight,
        ] {
            ensure(value > 0.0, "rule pack weight must be positive")?;
        }
        for value in [
            rule_pack.entity_density_weight,
            rule_pack.entity_layout_weight,
            rule_pack.patrol_weight,
            rule_pack.crowd_weight,
            rule_pack.builder_weight,
        ] {
            ensure(value >= 0.0, "entity rule pack weight cannot be negative")?;
        }
    }
    ensure(
        structure.rule_influences.len() == structure.metadata.rule_influence_count,
        "rule influence count mismatch",
    )?;
    ensure(
        !structure.rule_influences.is_empty(),
        "no rule influence traces exported",
    )?;
    for influence in &structure.rule_influences {
        ensure(
            influence.rule_pack_id < structure.rule_packs.len(),
            "rule influence references invalid rule pack",
        )?;
        ensure(
            !influence.target_type.is_empty() && !influence.target_id.is_empty(),
            "rule influence target is empty",
        )?;
    }

    ensure(
        structure.path_analysis.guaranteed_service_to_skyline,
        "missing guaranteed service-to-skyline path",
    )?;
    ensure(
        structure.path_analysis.alternate_path_count >= 3,
        "route redundancy below minimum",
    )?;
    ensure(
        structure.path_analysis.vertical_transfer_count >= 3,
        "vertical transfer count below minimum",
    )?;
    ensure(
        structure.path_analysis.reachable_landmark_count >= 8,
        "reachable landmark count below minimum",
    )?;
    ensure(
        structure.path_analysis.main_path_room_reachability >= 1.0,
        "main path contains unreachable rooms",
    )?;
    ensure(
        structure.path_analysis.quality_score >= 0.75,
        "topology quality score below minimum",
    )?;
    Ok(())
}

pub fn validate_scenario(scenario: &ScenarioRecord) -> StructureResult<()> {
    ensure(
        scenario.schema_version == SCENARIO_SCHEMA_VERSION,
        format!(
            "unsupported scenario schema {}, expected {}",
            scenario.schema_version, SCENARIO_SCHEMA_VERSION
        ),
    )?;
    ensure(!scenario.seed.is_empty(), "scenario seed is empty")?;
    ensure(!scenario.typology.is_empty(), "scenario typology is empty")?;
    ensure(!scenario.title.is_empty(), "scenario title is empty")?;
    ensure(
        !scenario.objective_route_ids.is_empty(),
        "scenario has no objective routes",
    )?;
    ensure(!scenario.landmarks.is_empty(), "scenario has no landmarks")?;
    ensure(
        !scenario.objective_chains.is_empty(),
        "scenario has no objective chains",
    )?;
    ensure(
        !scenario.route_constraints.is_empty(),
        "scenario has no route constraints",
    )?;
    ensure(
        !scenario.hazard_timings.is_empty(),
        "scenario has no timed hazards",
    )?;
    ensure(
        !scenario.resource_objectives.is_empty(),
        "scenario has no resource objectives",
    )?;
    ensure(
        (0.0..=1.0).contains(&scenario.difficulty_score),
        "scenario difficulty score out of range",
    )?;
    ensure(
        scenario.estimated_duration_minutes > 0,
        "scenario duration estimate must be positive",
    )?;
    ensure(
        !scenario.balance_notes.is_empty(),
        "scenario has no balance notes",
    )?;
    for value in [
        scenario.risk_breakdown.route_risk,
        scenario.risk_breakdown.hazard_risk,
        scenario.risk_breakdown.faction_risk,
        scenario.risk_breakdown.resource_risk,
        scenario.risk_breakdown.dynamic_risk,
        scenario.risk_breakdown.objective_complexity,
    ] {
        ensure((0.0..=1.0).contains(&value), "scenario risk out of range")?;
    }
    ensure(
        !scenario.dynamic_events.is_empty(),
        "scenario has no dynamic events",
    )?;
    ensure(
        !scenario.entity_objectives.is_empty(),
        "scenario has no entity objectives",
    )?;
    ensure(
        !scenario.scenario_consequences.is_empty(),
        "scenario has no scenario consequences",
    )?;
    if scenario.typology != MegastructureTypology::DenseEnclave.as_str() {
        ensure(
            !scenario.typology_objectives.is_empty(),
            "scenario has no typology objectives",
        )?;
    }
    for event in &scenario.dynamic_events {
        ensure(
            (0.0..=1.0).contains(&event.intensity),
            "dynamic event intensity out of range",
        )?;
        ensure(
            !event.affected_route_ids.is_empty() || !event.affected_room_ids.is_empty(),
            "dynamic event affects no routes or rooms",
        )?;
    }
    for objective in &scenario.entity_objectives {
        ensure(
            !objective.label.is_empty(),
            "entity objective label is empty",
        )?;
        ensure(
            !objective.route_ids.is_empty(),
            "entity objective has no routes",
        )?;
    }
    for objective in &scenario.typology_objectives {
        ensure(
            objective.typology == scenario.typology,
            "typology objective typology mismatch",
        )?;
        ensure(
            !objective.route_ids.is_empty() || !objective.hazard_ids.is_empty(),
            "typology objective has no routes or hazards",
        )?;
    }
    for consequence in &scenario.scenario_consequences {
        ensure(
            !consequence.kind.is_empty(),
            "scenario consequence kind is empty",
        )?;
        ensure(
            !consequence.label.is_empty(),
            "scenario consequence label is empty",
        )?;
    }
    ensure(
        !scenario.alternate_endings.is_empty(),
        "scenario has no alternate endings",
    )?;
    Ok(())
}

fn validate_typology_contracts(structure: &SavedStructure) -> StructureResult<()> {
    let typology = MegastructureTypology::from_str(&structure.metadata.typology)
        .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { error.into() })?;
    let route_count = |kind: &str| {
        structure
            .transit_graph
            .edges
            .iter()
            .filter(|edge| edge.kind == kind)
            .count()
    };
    match typology {
        MegastructureTypology::DenseEnclave => Ok(()),
        MegastructureTypology::ArcologySpire => {
            ensure(
                route_count("station_loop") >= 2,
                "arcology missing station loops",
            )?;
            ensure(
                route_count("vertical_transit_core") >= 1,
                "arcology missing vertical core",
            )
        }
        MegastructureTypology::LinearCity => {
            ensure(
                route_count("linear_express") >= 1,
                "linear city missing express spine",
            )?;
            ensure(
                route_count("station_loop") >= 2,
                "linear city missing station loops",
            )
        }
        MegastructureTypology::BridgeVoid => {
            ensure(
                route_count("void_bridge") >= 2,
                "bridge void missing bridges",
            )?;
            ensure(
                structure.typology_frame.service_anchors.len() >= 4,
                "bridge void missing tower anchors",
            )
        }
        MegastructureTypology::MarinePlatform => {
            ensure(
                route_count("marine_causeway") >= 2,
                "marine platform missing causeways",
            )?;
            ensure(
                route_count("pylon_service") >= 2,
                "marine platform missing pylon service routes",
            )
        }
        MegastructureTypology::OrbitalRing => {
            ensure(
                route_count("rim_loop") >= 6,
                "orbital ring missing rim continuity",
            )?;
            ensure(
                route_count("spoke_transfer") >= 2,
                "orbital ring missing spoke redundancy",
            )
        }
        MegastructureTypology::UndergroundHive => {
            ensure(
                route_count("hive_trunk") >= 1,
                "underground hive missing trunk",
            )?;
            ensure(
                route_count("cavern_loop") >= 2,
                "underground hive missing cavern loops",
            )
        }
        MegastructureTypology::MountainBurrow => {
            ensure(
                route_count("cliff_gallery") >= 1,
                "mountain burrow missing cliff gallery",
            )?;
            ensure(
                route_count("burrow_spine") >= 1,
                "mountain burrow missing burrow spine",
            )
        }
        MegastructureTypology::DesertArcology => {
            ensure(
                route_count("climate_spine") >= 1,
                "desert arcology missing climate spine",
            )?;
            ensure(
                route_count("solar_service_ring") >= 1,
                "desert arcology missing solar service ring",
            )
        }
        MegastructureTypology::AirportCity => {
            ensure(
                route_count("runway_spine") >= 1,
                "airport city missing runway spine",
            )?;
            ensure(
                route_count("terminal_loop") >= 2,
                "airport city missing terminal loops",
            )
        }
        MegastructureTypology::DamCity => {
            ensure(
                route_count("dam_wall_spine") >= 1,
                "dam city missing dam spine",
            )?;
            ensure(
                route_count("turbine_gallery") >= 1,
                "dam city missing turbine gallery",
            )
        }
        MegastructureTypology::ShipyardStack => {
            ensure(
                route_count("drydock_spine") >= 1,
                "shipyard stack missing drydock spine",
            )?;
            ensure(
                route_count("gantry_loop") >= 2,
                "shipyard stack missing gantry loops",
            )
        }
    }
}

fn ensure(condition: bool, message: impl Into<String>) -> StructureResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into().into())
    }
}

fn ensure_point(point: [usize; 3], size: usize, layers: usize, label: &str) -> StructureResult<()> {
    ensure(
        point[0] < size && point[1] < layers && point[2] < size,
        format!(
            "{label} point out of bounds: ({}, {}, {})",
            point[0], point[1], point[2]
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GenerationConfig;
    use crate::generation::generate_saved_structure;
    use crate::scenario::generate_scenario;

    #[test]
    fn validates_generated_structure_and_scenario() {
        let structure =
            generate_saved_structure("ABCD1234".to_owned(), GenerationConfig::default()).unwrap();
        validate_structure(&structure).unwrap();
        validate_scenario(&generate_scenario(&structure)).unwrap();
    }
}
