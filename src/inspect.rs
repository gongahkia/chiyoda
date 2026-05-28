use crate::cli::InspectSection;
use crate::scenario::generate_scenario;
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
                        "typology": structure.metadata.typology,
                        "occupied_cell_ratio": structure.metadata.occupied_cell_ratio,
                        "construction_eras": structure.construction_history.len(),
                        "section_quality_score": structure.section_quality.score,
                        "stress_field_count": structure.structural_system.stress_fields.len(),
                        "load_path_count": structure.structural_system.load_paths.len(),
                    }),
                );
            }
            InspectSection::Routes => {
                object.insert(
                    "routes".to_owned(),
                    serde_json::json!({
                        "edges": &structure.transit_graph.edges,
                        "route_stress": route_stress_summary(structure),
                    }),
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
                    "quality".to_owned(),
                    serde_json::json!({
                        "path_analysis": &structure.path_analysis,
                        "typology_quality": &structure.typology_quality,
                        "section_quality": &structure.section_quality,
                        "stress_fields": &structure.structural_system.stress_fields,
                        "load_paths": &structure.structural_system.load_paths,
                    }),
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
                    serde_json::json!({
                        "hazards": &structure.hazard_zones,
                        "stress_derived_count": structure.hazard_zones.iter().filter(|hazard| hazard.kind.starts_with("stress_")).count(),
                        "stress_fields": &structure.structural_system.stress_fields,
                    }),
                );
            }
            InspectSection::Entities => {
                let scenario = generate_scenario(structure);
                object.insert(
                    "entities".to_owned(),
                    serde_json::json!({
                        "entities": &structure.entities,
                        "paths": &structure.entity_paths,
                        "pressure_fields": &structure.entity_pressure_fields,
                        "layout_mutations": &structure.layout_mutations,
                        "scenario_consequences": scenario.scenario_consequences,
                    }),
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
            InspectSection::Entities => render_entities(structure, &mut lines),
        }
    }
    lines.join("\n")
}

fn render_entities(structure: &SavedStructure, lines: &mut Vec<String>) {
    lines.push("Entities:".to_owned());
    lines.push(format!(
        "- entities={} paths={} pressure_fields={} layout_mutations={}",
        structure.entities.len(),
        structure.entity_paths.len(),
        structure.entity_pressure_fields.len(),
        structure.layout_mutations.len()
    ));
    for entity in structure.entities.iter().take(20) {
        lines.push(format!(
            "- #{} {} profile={} routes={:?} phases={:?} influence={:.2}",
            entity.id,
            entity.kind,
            entity.movement_profile,
            entity.route_ids,
            entity.active_phase_ids,
            entity.layout_influence
        ));
    }
    for field in structure.entity_pressure_fields.iter().take(8) {
        lines.push(format!(
            "- field #{} {} intensity={:.2} routes={}",
            field.id,
            field.kind,
            field.intensity,
            field.affected_route_ids.len()
        ));
    }
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
        let source = if hazard.kind.starts_with("stress_") {
            "stress-derived"
        } else {
            "generated"
        };
        lines.push(format!(
            "- #{} {} severity={:.2} source={} routes={:?} rooms={}",
            hazard.id,
            hazard.kind,
            hazard.severity,
            source,
            hazard.route_ids,
            hazard.room_ids.len()
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
    lines.push(format!(
        "- typology={:.2} section={:.2} missing_contracts={}",
        structure.typology_quality.score,
        structure.section_quality.score,
        structure.section_quality.missing_contracts.len()
    ));
    lines.push(format!(
        "- section vertical={:.2} void={:.2} service={:.2} evac={:.2} roof={:.2}",
        structure.section_quality.vertical_continuity,
        structure.section_quality.void_exposure,
        structure.section_quality.service_separation,
        structure.section_quality.evacuation_shaft_coverage,
        structure.section_quality.roof_deck_access
    ));
    for field in structure.structural_system.stress_fields.iter().take(8) {
        lines.push(format!(
            "- stress #{} {} stress={:.2} routes={:?} supports={}",
            field.id,
            field.kind,
            field.stress,
            field.route_ids,
            field.support_points.len()
        ));
    }
    for path in structure.structural_system.load_paths.iter().take(6) {
        lines.push(format!(
            "- load_path #{} {} stress={:.2} routes={:?}",
            path.id, path.kind, path.stress, path.route_ids
        ));
    }
}

fn render_summary(structure: &SavedStructure, lines: &mut Vec<String>) {
    lines.push(format!(
        "Gibson {} seed={} profile={} typology={}",
        structure.metadata.schema_version,
        structure.seed,
        structure.metadata.profile,
        structure.metadata.typology
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
        "Dynamics: entities={} paths={} fields={} mutations={}",
        structure.metadata.entity_count,
        structure.metadata.entity_path_count,
        structure.metadata.entity_pressure_field_count,
        structure.metadata.layout_mutation_count
    ));
    lines.push(format!(
        "v22: construction_eras={} section={:.2} stress_fields={} load_paths={}",
        structure.construction_history.len(),
        structure.section_quality.score,
        structure.structural_system.stress_fields.len(),
        structure.structural_system.load_paths.len()
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
            "- #{} {} role={} stratum={} length={} stress={:.2} evac={:.2}",
            edge.id,
            edge.kind,
            edge.role,
            edge.stratum,
            edge.length,
            route_stress_for(structure, edge.id),
            structure
                .route_simulation
                .get(edge.id)
                .map(|simulation| simulation.evacuation_viability)
                .unwrap_or(0.0)
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

fn route_stress_for(structure: &SavedStructure, route_id: usize) -> f32 {
    structure
        .structural_system
        .stress_fields
        .iter()
        .filter(|field| field.route_ids.contains(&route_id))
        .map(|field| field.stress)
        .fold(0.0, f32::max)
}

fn route_stress_summary(structure: &SavedStructure) -> Vec<serde_json::Value> {
    structure
        .transit_graph
        .edges
        .iter()
        .map(|edge| {
            let simulation = structure.route_simulation.get(edge.id);
            serde_json::json!({
                "route_id": edge.id,
                "stress": route_stress_for(structure, edge.id),
                "evacuation_viability": simulation.map(|record| record.evacuation_viability),
                "blackout_risk": simulation.map(|record| record.blackout_risk),
            })
        })
        .collect()
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
                InspectSection::Entities,
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
        assert!(output.contains("Entities:"));
        let json = render_inspection_json(&structure, &[InspectSection::Quality]).unwrap();
        assert!(json.contains("path_analysis"));
        assert!(json.contains("section_quality"));
        assert!(json.contains("stress_fields"));
    }
}
