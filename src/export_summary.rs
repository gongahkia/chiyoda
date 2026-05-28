use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::structure::SavedStructure;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CompactExportSummary {
    pub schema_version: String,
    pub seed: String,
    pub profile: String,
    pub typology: String,
    pub size: usize,
    pub layers: usize,
    pub counts: BTreeMap<String, usize>,
    pub quality_milli: usize,
    pub occupied_milli: usize,
}

pub fn compact_export_summary(structure: &SavedStructure) -> CompactExportSummary {
    let counts = BTreeMap::from([
        ("factions".to_owned(), structure.factions.len()),
        (
            "construction_eras".to_owned(),
            structure.construction_history.len(),
        ),
        ("entities".to_owned(), structure.entities.len()),
        ("entity_paths".to_owned(), structure.entity_paths.len()),
        (
            "entity_pressure_fields".to_owned(),
            structure.entity_pressure_fields.len(),
        ),
        ("failure_zones".to_owned(), structure.failure_zones.len()),
        ("hazards".to_owned(), structure.hazard_zones.len()),
        (
            "layout_mutations".to_owned(),
            structure.layout_mutations.len(),
        ),
        ("landmarks".to_owned(), structure.narrative_landmarks.len()),
        ("macro_massing".to_owned(), structure.macro_massing.len()),
        (
            "meso_placements".to_owned(),
            structure.meso_placements.len(),
        ),
        ("micro_details".to_owned(), structure.micro_details.len()),
        (
            "resource_networks".to_owned(),
            structure.resource_networks.len(),
        ),
        ("room_clusters".to_owned(), structure.room_clusters.len()),
        ("rooms".to_owned(), structure.rooms.len()),
        (
            "route_nodes".to_owned(),
            structure.transit_graph.nodes.len(),
        ),
        ("routes".to_owned(), structure.transit_graph.edges.len()),
        ("rule_packs".to_owned(), structure.rule_packs.len()),
        (
            "stress_fields".to_owned(),
            structure.structural_system.stress_fields.len(),
        ),
        (
            "load_paths".to_owned(),
            structure.structural_system.load_paths.len(),
        ),
    ]);

    CompactExportSummary {
        schema_version: structure.metadata.schema_version.clone(),
        seed: structure.seed.clone(),
        profile: structure.metadata.profile.clone(),
        typology: structure.metadata.typology.clone(),
        size: structure.size,
        layers: structure.layers,
        counts,
        quality_milli: rounded_milli(structure.path_analysis.quality_score),
        occupied_milli: rounded_milli(structure.metadata.occupied_cell_ratio),
    }
}

fn rounded_milli(value: f32) -> usize {
    (value * 1000.0).round().clamp(0.0, 1000.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GenerationConfig;
    use crate::generation::generate_saved_structure;

    #[test]
    fn balanced_seed_export_summary_matches_golden_snapshot() {
        let structure =
            generate_saved_structure("ABCD1234".to_owned(), GenerationConfig::default()).unwrap();
        let summary = compact_export_summary(&structure);
        let golden: CompactExportSummary = serde_json::from_str(include_str!(
            "../tests/golden/balanced_ABCD1234_summary.json"
        ))
        .unwrap();
        assert_eq!(summary, golden);
    }
}
