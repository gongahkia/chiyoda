use serde::{Deserialize, Serialize};

use crate::structure::{SavedStructure, StructureResult};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ScenarioRecord {
    pub schema_version: String,
    pub seed: String,
    pub profile: String,
    pub title: String,
    pub start: [usize; 3],
    pub goal: [usize; 3],
    pub objective_route_ids: Vec<usize>,
    pub objective_room_ids: Vec<usize>,
    pub hazard_ids: Vec<usize>,
    pub faction_conflicts: Vec<ScenarioConflictRecord>,
    pub landmarks: Vec<ScenarioLandmarkRecord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ScenarioConflictRecord {
    pub border_id: usize,
    pub faction_ids: Vec<usize>,
    pub intensity: f32,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ScenarioLandmarkRecord {
    pub id: usize,
    pub name: String,
    pub kind: String,
    pub position: [usize; 3],
}

pub fn generate_scenario(structure: &SavedStructure) -> ScenarioRecord {
    let main_path = structure.path_analysis.main_path.as_ref();
    let start =
        main_path
            .map(|path| path.start)
            .unwrap_or([structure.size / 2, 1, structure.size / 2]);
    let goal = main_path.map(|path| path.end).unwrap_or(start);
    let landmarks: Vec<_> = structure
        .narrative_landmarks
        .iter()
        .take(12)
        .map(|landmark| ScenarioLandmarkRecord {
            id: landmark.id,
            name: landmark.name.clone(),
            kind: landmark.kind.clone(),
            position: landmark.position,
        })
        .collect();
    ScenarioRecord {
        schema_version: "gibson.scenario.v1".to_owned(),
        seed: structure.seed.clone(),
        profile: structure.metadata.profile.clone(),
        title: scenario_title(structure),
        start,
        goal,
        objective_route_ids: main_path
            .map(|path| path.route_ids.clone())
            .unwrap_or_default(),
        objective_room_ids: main_path
            .map(|path| path.room_ids.clone())
            .unwrap_or_default(),
        hazard_ids: structure
            .hazard_zones
            .iter()
            .filter(|hazard| hazard.severity >= 0.5)
            .map(|hazard| hazard.id)
            .take(12)
            .collect(),
        faction_conflicts: structure
            .contested_borders
            .iter()
            .take(12)
            .map(|conflict| ScenarioConflictRecord {
                border_id: conflict.border_id,
                faction_ids: conflict.faction_ids.clone(),
                intensity: conflict.intensity,
                reason: conflict.reason.clone(),
            })
            .collect(),
        landmarks,
    }
}

pub fn to_json(scenario: &ScenarioRecord) -> serde_json::Result<String> {
    serde_json::to_string_pretty(scenario)
}

pub fn save_scenario(
    path: impl AsRef<std::path::Path>,
    scenario: &ScenarioRecord,
) -> StructureResult<()> {
    std::fs::write(path, to_json(scenario)?)?;
    Ok(())
}

fn scenario_title(structure: &SavedStructure) -> String {
    let landmark = structure
        .narrative_landmarks
        .first()
        .map(|landmark| landmark.name.as_str())
        .unwrap_or("Unnamed Stack");
    format!("{} / {}", structure.seed, landmark)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GenerationConfig;
    use crate::generation::generate_saved_structure;

    #[test]
    fn scenario_extracts_objectives_from_structure() {
        let structure =
            generate_saved_structure("ABCD1234".to_owned(), GenerationConfig::default()).unwrap();
        let scenario = generate_scenario(&structure);
        assert_eq!(scenario.schema_version, "gibson.scenario.v1");
        assert_eq!(scenario.seed, structure.seed);
        assert!(!scenario.objective_route_ids.is_empty());
        assert!(!scenario.landmarks.is_empty());
        assert!(to_json(&scenario).unwrap().contains("faction_conflicts"));
    }
}
