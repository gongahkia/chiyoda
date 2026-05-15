use crate::cli::InspectSection;
use crate::structure::SavedStructure;

pub fn render_inspection(structure: &SavedStructure, sections: &[InspectSection]) -> String {
    let mut lines = Vec::new();
    for section in sections {
        match section {
            InspectSection::Summary => render_summary(structure, &mut lines),
            InspectSection::Routes => render_routes(structure, &mut lines),
            InspectSection::Landmarks => render_landmarks(structure, &mut lines),
            InspectSection::Path => render_path(structure, &mut lines),
        }
    }
    lines.join("\n")
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
            ],
        );
        assert!(output.contains("Gibson gibson.structure.v13"));
        assert!(output.contains("Routes:"));
        assert!(output.contains("Landmarks:"));
        assert!(output.contains("Path Analysis:"));
    }
}
