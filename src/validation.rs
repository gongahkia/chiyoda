use std::fs;
use std::path::Path;

use crate::scenario::{self, ScenarioRecord};
use crate::structure::{self, SavedStructure, StructureResult, STRUCTURE_SCHEMA_VERSION};

const SCENARIO_SCHEMA_VERSION: &str = "gibson.scenario.v3";
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
            "valid structure {} seed={} profile={}",
            structure.metadata.schema_version, structure.seed, structure.metadata.profile
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
        !scenario.alternate_endings.is_empty(),
        "scenario has no alternate endings",
    )?;
    Ok(())
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
