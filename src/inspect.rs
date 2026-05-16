use crate::cli::InspectSection;
use crate::structure::SavedStructure;

pub fn render_inspection_json(
    structure: &SavedStructure,
    sections: &[InspectSection],
) -> serde_json::Result<String> {
    let mut object = serde_json::Map::new();
    for section in sections {
        match section {
            InspectSection::Summary => {
                object.insert(
                    "summary".to_owned(),
                    serde_json::json!({
                        "schema_version": structure.metadata.schema_version,
                        "seed": structure.seed,
                        "profile": structure.metadata.profile,
                        "occupied_cell_ratio": structure.metadata.occupied_cell_ratio,
                    }),
                );
            }
            InspectSection::Routes => {
                object.insert(
                    "routes".to_owned(),
                    serde_json::to_value(&structure.transit_graph.edges)?,
                );
            }
            InspectSection::Landmarks => {
                object.insert(
                    "landmarks".to_owned(),
                    serde_json::to_value(&structure.narrative_landmarks)?,
                );
            }
            InspectSection::Path | InspectSection::Quality => {
                object.insert(
                    "path_analysis".to_owned(),
                    serde_json::to_value(&structure.path_analysis)?,
                );
            }
            InspectSection::Simulation => {
                object.insert(
                    "simulation".to_owned(),
                    serde_json::to_value(&structure.route_simulation)?,
                );
            }
            InspectSection::Factions => {
                object.insert(
                    "factions".to_owned(),
                    serde_json::to_value(&structure.factions)?,
                );
            }
            InspectSection::Hazards => {
                object.insert(
                    "hazards".to_owned(),
                    serde_json::to_value(&structure.hazard_zones)?,
                );
            }
        }
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(object))
}

pub fn render_inspection(structure: &SavedStructure, sections: &[InspectSection]) -> String {
    let mut lines = Vec::new();
    for section in sections {
        match section {
            InspectSection::Summary => render_summary(structure, &mut lines),
            InspectSection::Routes => render_routes(structure, &mut lines),
            InspectSection::Landmarks => render_landmarks(structure, &mut lines),
            InspectSection::Path => render_path(structure, &mut lines),
            InspectSection::Simulation => render_simulation(structure, &mut lines),
            InspectSection::Factions => render_factions(structure, &mut lines),
            InspectSection::Hazards => render_hazards(structure, &mut lines),
            InspectSection::Quality => render_quality(structure, &mut lines),
        }
    }
    lines.join("\n")
}

fn render_simulation(structure: &SavedStructure, lines: &mut Vec<String>) {
    lines.push("Simulation:".to_owned());
    for simulation in structure.route_simulation.iter().take(20) {
        lines.push(format!(
            "- route #{} civilian={:.2} security={:.2} blackout={:.2} market={:.2} evac={:.2}",
            simulation.route_id,
            simulation.civilian_density,
            simulation.security_pressure,
            simulation.blackout_risk,
            simulation.market_congestion,
            simulation.evacuation_viability
        ));
    }
}

fn render_factions(structure: &SavedStructure, lines: &mut Vec<String>) {
    lines.push("Factions:".to_owned());
    for faction in &structure.factions {
        lines.push(format!(
            "- #{} {} influence={:.2} districts={}",
            faction.id,
            faction.name,
            faction.influence,
            faction.controlled_districts.join(",")
        ));
    }
}

fn render_hazards(structure: &SavedStructure, lines: &mut Vec<String>) {
    lines.push("Hazards:".to_owned());
    for hazard in structure.hazard_zones.iter().take(20) {
        lines.push(format!(
            "- #{} {} severity={:.2} routes={:?}",
            hazard.id, hazard.kind, hazard.severity, hazard.route_ids
        ));
    }
}

fn render_quality(structure: &SavedStructure, lines: &mut Vec<String>) {
    lines.push("Quality:".to_owned());
    lines.push(format!(
        "- topology={:.2} redundancy={:.2} reachable_landmarks={} failures={} resources={}",
        structure.path_analysis.quality_score,
        structure.path_analysis.route_redundancy_score,
        structure.path_analysis.reachable_landmark_count,
        structure.metadata.failure_zone_count,
        structure.metadata.resource_network_count
    ));
}

fn render_summary(structure: &SavedStructure, lines: &mut Vec<String>) {
    lines.push(format!(
        "Gibson {} seed={} profile={}",
        structure.metadata.schema_version, structure.seed, structure.metadata.profile
    ));
    lines.push(format!(
        "Grid: {}x{}x{} occupied={:.1}%",
        structure.size,
        structure.size,
        structure.layers,
        structure.metadata.occupied_cell_ratio * 100.0
    ));
    lines.push(format!(
        "Rooms={} routes={} clusters={} landmarks={} hazards={} factions={}",
        structure.metadata.room_count,
        structure.metadata.transit_edge_count,
        structure.metadata.room_cluster_count,
        structure.metadata.narrative_landmark_count,
        structure.metadata.hazard_zone_count,
        structure.metadata.faction_count
    ));
    lines.push(format!(
        "Connectivity: components={} largest={} service_to_skyline={}",
        structure.path_analysis.connected_component_count,
        structure.path_analysis.largest_component_edges,
        structure.path_analysis.guaranteed_service_to_skyline
    ));
}

fn render_routes(structure: &SavedStructure, lines: &mut Vec<String>) {
    lines.push("Routes:".to_owned());
    for edge in structure.transit_graph.edges.iter().take(20) {
        lines.push(format!(
            "- #{} {} role={} stratum={} length={}",
            edge.id, edge.kind, edge.role, edge.stratum, edge.length
        ));
    }
}

fn render_landmarks(structure: &SavedStructure, lines: &mut Vec<String>) {
    lines.push("Landmarks:".to_owned());
    for landmark in structure.narrative_landmarks.iter().take(30) {
        lines.push(format!(
            "- #{} {} [{}] at ({}, {}, {})",
            landmark.id,
            landmark.name,
            landmark.kind,
            landmark.position[0],
            landmark.position[1],
            landmark.position[2]
        ));
    }
}

fn render_path(structure: &SavedStructure, lines: &mut Vec<String>) {
    lines.push("Path Analysis:".to_owned());
    lines.push(format!(
        "- components={} alternate_paths={} vertical_transfers={} chokepoints={} dead_ends={}",
        structure.path_analysis.connected_component_count,
        structure.path_analysis.alternate_path_count,
        structure.path_analysis.vertical_transfer_count,
        structure.path_analysis.chokepoint_count,
        structure.path_analysis.dead_end_count
    ));
    lines.push(format!(
        "- quality={:.2} redundancy={:.2} reachable_landmarks={} faction_connectivity={:.2} main_room_reachability={:.2}",
        structure.path_analysis.quality_score,
        structure.path_analysis.route_redundancy_score,
        structure.path_analysis.reachable_landmark_count,
        structure.path_analysis.faction_territory_connectivity,
        structure.path_analysis.main_path_room_reachability
    ));
    if let Some(path) = &structure.path_analysis.main_path {
        lines.push(format!(
            "- main={} routes={:?} rooms={}",
            path.label,
            path.route_ids,
            path.room_ids.len()
        ));
        lines.push(format!(
            "- start=({}, {}, {}) end=({}, {}, {})",
            path.start[0], path.start[1], path.start[2], path.end[0], path.end[1], path.end[2]
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GenerationConfig;
    use crate::generation::generate_saved_structure;
    use crate::structure::STRUCTURE_SCHEMA_VERSION;

    #[test]
    fn renders_requested_inspection_sections() {
        let structure =
            generate_saved_structure("ABCD1234".to_owned(), GenerationConfig::default()).unwrap();
        let output = render_inspection(
            &structure,
            &[
                InspectSection::Summary,
                InspectSection::Routes,
                InspectSection::Landmarks,
                InspectSection::Path,
                InspectSection::Simulation,
                InspectSection::Factions,
                InspectSection::Hazards,
                InspectSection::Quality,
            ],
        );
        assert!(output.contains(&format!("Gibson {STRUCTURE_SCHEMA_VERSION}")));
        assert!(output.contains("Routes:"));
        assert!(output.contains("Landmarks:"));
        assert!(output.contains("Path Analysis:"));
        assert!(output.contains("Simulation:"));
        assert!(output.contains("Factions:"));
        assert!(output.contains("Hazards:"));
        assert!(output.contains("Quality:"));
        let json = render_inspection_json(&structure, &[InspectSection::Quality]).unwrap();
        assert!(json.contains("path_analysis"));
    }
}
