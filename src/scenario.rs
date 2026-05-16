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
    pub objective_chains: Vec<ScenarioObjectiveRecord>,
    pub route_constraints: Vec<ScenarioRouteConstraintRecord>,
    pub faction_choices: Vec<ScenarioFactionChoiceRecord>,
    pub hazard_timings: Vec<ScenarioHazardTimingRecord>,
    pub resource_objectives: Vec<ScenarioResourceObjectiveRecord>,
    pub difficulty_score: f32,
    pub estimated_duration_minutes: usize,
    pub risk_breakdown: ScenarioRiskBreakdownRecord,
    pub balance_notes: Vec<String>,
    pub failure_states: Vec<String>,
    pub alternate_endings: Vec<ScenarioEndingRecord>,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ScenarioObjectiveRecord {
    pub id: usize,
    pub label: String,
    pub route_ids: Vec<usize>,
    pub room_ids: Vec<usize>,
    pub landmark_ids: Vec<usize>,
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ScenarioRouteConstraintRecord {
    pub route_id: usize,
    pub constraint: String,
    pub pressure: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ScenarioFactionChoiceRecord {
    pub faction_id: usize,
    pub label: String,
    pub benefit: String,
    pub cost: String,
    pub affected_route_ids: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ScenarioHazardTimingRecord {
    pub hazard_id: usize,
    pub phase: String,
    pub cycle_hour: usize,
    pub severity: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ScenarioEndingRecord {
    pub label: String,
    pub condition: String,
    pub consequence: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ScenarioResourceObjectiveRecord {
    pub network_id: usize,
    pub kind: String,
    pub route_ids: Vec<usize>,
    pub objective: String,
    pub outage: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ScenarioRiskBreakdownRecord {
    pub route_risk: f32,
    pub hazard_risk: f32,
    pub faction_risk: f32,
    pub resource_risk: f32,
    pub objective_complexity: f32,
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
    let objective_route_ids = main_path
        .map(|path| path.route_ids.clone())
        .unwrap_or_default();
    let objective_room_ids = main_path
        .map(|path| path.room_ids.clone())
        .unwrap_or_default();
    let hazard_ids: Vec<_> = structure
        .hazard_zones
        .iter()
        .filter(|hazard| hazard.severity >= 0.5)
        .map(|hazard| hazard.id)
        .take(12)
        .collect();
    let faction_conflicts: Vec<_> = structure
        .contested_borders
        .iter()
        .take(12)
        .map(|conflict| ScenarioConflictRecord {
            border_id: conflict.border_id,
            faction_ids: conflict.faction_ids.clone(),
            intensity: conflict.intensity,
            reason: conflict.reason.clone(),
        })
        .collect();
    let objective_chains = objective_chains(&objective_route_ids, &objective_room_ids, &landmarks);
    let risk_breakdown = risk_breakdown(
        structure,
        &objective_route_ids,
        &hazard_ids,
        &faction_conflicts,
    );
    let difficulty_score = scenario_difficulty(&risk_breakdown);
    let estimated_duration_minutes =
        estimated_duration_minutes(&objective_route_ids, &objective_room_ids, difficulty_score);
    ScenarioRecord {
        schema_version: "gibson.scenario.v3".to_owned(),
        seed: structure.seed.clone(),
        profile: structure.metadata.profile.clone(),
        title: scenario_title(structure),
        start,
        goal,
        objective_route_ids: objective_route_ids.clone(),
        objective_room_ids: objective_room_ids.clone(),
        hazard_ids: hazard_ids.clone(),
        faction_conflicts: faction_conflicts.clone(),
        landmarks,
        objective_chains,
        route_constraints: route_constraints(structure, &objective_route_ids),
        faction_choices: faction_choices(structure, &faction_conflicts),
        hazard_timings: hazard_timings(structure, &hazard_ids),
        resource_objectives: resource_objectives(structure),
        difficulty_score,
        estimated_duration_minutes,
        risk_breakdown: risk_breakdown.clone(),
        balance_notes: balance_notes(difficulty_score, &risk_breakdown),
        failure_states: failure_states(structure),
        alternate_endings: alternate_endings(structure),
    }
}

pub fn to_json(scenario: &ScenarioRecord) -> serde_json::Result<String> {
    serde_json::to_string_pretty(scenario)
}

pub fn from_json(json: &str) -> serde_json::Result<ScenarioRecord> {
    serde_json::from_str(json)
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

fn objective_chains(
    route_ids: &[usize],
    room_ids: &[usize],
    landmarks: &[ScenarioLandmarkRecord],
) -> Vec<ScenarioObjectiveRecord> {
    let landmark_ids: Vec<_> = landmarks.iter().map(|landmark| landmark.id).collect();
    [
        ("enter through maintenance access", 0usize, 3usize, true),
        ("cross contested circulation", 3, 7, true),
        ("reach the skyline objective", 7, route_ids.len(), true),
    ]
    .into_iter()
    .enumerate()
    .map(
        |(id, (label, start, end, required))| ScenarioObjectiveRecord {
            id,
            label: label.to_owned(),
            route_ids: slice_ids(route_ids, start, end),
            room_ids: slice_ids(room_ids, start * 4, end * 8),
            landmark_ids: slice_ids(&landmark_ids, id * 2, id * 2 + 3),
            required,
        },
    )
    .filter(|objective| !objective.route_ids.is_empty() || !objective.room_ids.is_empty())
    .collect()
}

fn route_constraints(
    structure: &SavedStructure,
    objective_route_ids: &[usize],
) -> Vec<ScenarioRouteConstraintRecord> {
    objective_route_ids
        .iter()
        .filter_map(|route_id| structure.transit_graph.edges.get(*route_id))
        .map(|edge| {
            let constraint = match edge.role.as_str() {
                "restricted_spine" => "corp checkpoint",
                "market_run" => "crowd congestion",
                "evacuation_route" => "exposed crossing",
                "maintenance_backbone" => "access key required",
                "service_loop" => "low clearance crawl",
                _ => "route surveillance",
            };
            ScenarioRouteConstraintRecord {
                route_id: edge.id,
                constraint: constraint.to_owned(),
                pressure: route_pressure(edge),
            }
        })
        .collect()
}

fn faction_choices(
    structure: &SavedStructure,
    conflicts: &[ScenarioConflictRecord],
) -> Vec<ScenarioFactionChoiceRecord> {
    conflicts
        .iter()
        .take(5)
        .filter_map(|conflict| {
            let faction_id = *conflict.faction_ids.first()?;
            let faction = structure.factions.get(faction_id)?;
            Some(ScenarioFactionChoiceRecord {
                faction_id,
                label: format!("negotiate with {}", faction.name),
                benefit: faction_benefit(&faction.agenda).to_owned(),
                cost: "raises pressure with the opposing border faction".to_owned(),
                affected_route_ids: structure
                    .district_borders
                    .iter()
                    .find(|border| border.id == conflict.border_id)
                    .map(|border| border.route_ids.clone())
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn hazard_timings(
    structure: &SavedStructure,
    hazard_ids: &[usize],
) -> Vec<ScenarioHazardTimingRecord> {
    hazard_ids
        .iter()
        .filter_map(|hazard_id| {
            let hazard = structure.hazard_zones.get(*hazard_id)?;
            let phase = structure
                .temporal_state
                .phases
                .iter()
                .find(|phase| phase.affected_hazard_ids.contains(hazard_id))
                .or_else(|| structure.temporal_state.phases.first())?;
            Some(ScenarioHazardTimingRecord {
                hazard_id: *hazard_id,
                phase: phase.name.clone(),
                cycle_hour: phase.cycle_hour,
                severity: hazard.severity,
            })
        })
        .collect()
}

fn resource_objectives(structure: &SavedStructure) -> Vec<ScenarioResourceObjectiveRecord> {
    let mut objectives: Vec<_> = structure
        .resource_networks
        .iter()
        .filter(|network| network.outage || network.overloaded)
        .take(6)
        .map(resource_objective)
        .collect();
    if objectives.is_empty() {
        objectives.extend(
            structure
                .resource_networks
                .iter()
                .take(1)
                .map(resource_objective),
        );
    }
    objectives
}

fn resource_objective(
    network: &crate::structure::ResourceNetworkRecord,
) -> ScenarioResourceObjectiveRecord {
    ScenarioResourceObjectiveRecord {
        network_id: network.id,
        kind: network.kind.clone(),
        route_ids: if network.reroute_route_ids.is_empty() {
            network.route_ids.clone()
        } else {
            network.reroute_route_ids.clone()
        },
        objective: if network.outage {
            format!("restore {} through reroute", network.kind)
        } else if network.overloaded {
            format!("reduce {} overload", network.kind)
        } else {
            format!("keep {} stable", network.kind)
        },
        outage: network.outage,
    }
}

fn risk_breakdown(
    structure: &SavedStructure,
    objective_route_ids: &[usize],
    hazard_ids: &[usize],
    conflicts: &[ScenarioConflictRecord],
) -> ScenarioRiskBreakdownRecord {
    let route_risks: Vec<_> = objective_route_ids
        .iter()
        .filter_map(|route_id| structure.route_simulation.get(*route_id))
        .map(|simulation| 1.0 - simulation.evacuation_viability)
        .collect();
    let route_risk = average(&route_risks);
    let hazard_risk = hazard_ids
        .iter()
        .filter_map(|hazard_id| structure.hazard_zones.get(*hazard_id))
        .map(|hazard| hazard.severity)
        .fold(0.0, f32::max);
    let faction_risk = conflicts
        .iter()
        .map(|conflict| conflict.intensity)
        .fold(0.0, f32::max);
    let resource_risk = structure
        .resource_networks
        .iter()
        .map(|network| {
            let overload_pressure = (network.load / network.capacity - 1.0).max(0.0);
            if network.outage {
                0.70 + overload_pressure * 0.20
            } else {
                overload_pressure * 0.35
            }
        })
        .fold(0.0, f32::max)
        .clamp(0.0, 1.0);
    let objective_complexity = (objective_route_ids.len() as f32 / 12.0
        + structure.path_analysis.dead_end_count as f32 * 0.015)
        .clamp(0.0, 1.0);

    ScenarioRiskBreakdownRecord {
        route_risk,
        hazard_risk,
        faction_risk,
        resource_risk,
        objective_complexity,
    }
}

fn scenario_difficulty(risk: &ScenarioRiskBreakdownRecord) -> f32 {
    (risk.route_risk * 0.28
        + risk.hazard_risk * 0.22
        + risk.faction_risk * 0.18
        + risk.resource_risk * 0.18
        + risk.objective_complexity * 0.14)
        .clamp(0.0, 1.0)
}

fn estimated_duration_minutes(route_ids: &[usize], room_ids: &[usize], difficulty: f32) -> usize {
    (18.0 + route_ids.len() as f32 * 3.0 + room_ids.len() as f32 * 0.35 + difficulty * 28.0)
        .round()
        .max(1.0) as usize
}

fn balance_notes(difficulty: f32, risk: &ScenarioRiskBreakdownRecord) -> Vec<String> {
    let mut notes = vec![format!(
        "difficulty {:.2} from route, hazard, faction, resource, and objective pressure",
        difficulty
    )];
    if risk.resource_risk > 0.6 {
        notes.push("resource outage pressure should become an explicit mission branch".to_owned());
    }
    if risk.hazard_risk > 0.6 {
        notes.push("severe hazards require alternate route visibility".to_owned());
    }
    if risk.route_risk > 0.45 {
        notes.push("route viability is low enough to reward bypass discovery".to_owned());
    }
    if risk.faction_risk > 0.6 {
        notes.push("faction conflict can support negotiation or stealth choices".to_owned());
    }
    if notes.len() == 1 {
        notes.push("scenario stays inside standard traversal bounds".to_owned());
    }
    notes
}

fn failure_states(structure: &SavedStructure) -> Vec<String> {
    vec![
        "objective route collapses before alternate path is found".to_owned(),
        "corp checkpoint locks the restricted spine".to_owned(),
        format!(
            "topology quality drops below {:.2} after hazard escalation",
            structure.path_analysis.quality_score
        ),
    ]
}

fn alternate_endings(structure: &SavedStructure) -> Vec<ScenarioEndingRecord> {
    vec![
        ScenarioEndingRecord {
            label: "quiet extraction".to_owned(),
            condition: "avoid severe hazards and keep faction conflicts cold".to_owned(),
            consequence: "main route remains usable for future runs".to_owned(),
        },
        ScenarioEndingRecord {
            label: "market uprising".to_owned(),
            condition: "side with market or slum factions at contested borders".to_owned(),
            consequence: "commercial corridors open while security thresholds harden".to_owned(),
        },
        ScenarioEndingRecord {
            label: "skyline breach".to_owned(),
            condition: format!(
                "reach goal at ({}, {}, {})",
                structure.size / 2,
                structure.layers.saturating_sub(2),
                structure.size / 2
            ),
            consequence: "the skyline vault becomes a new landmark for sequel exports".to_owned(),
        },
    ]
}

fn average(values: &[f32]) -> f32 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f32>() / values.len() as f32
    }
}

fn slice_ids(ids: &[usize], start: usize, end: usize) -> Vec<usize> {
    ids.iter()
        .skip(start)
        .take(end.saturating_sub(start))
        .copied()
        .collect()
}

fn route_pressure(edge: &crate::structure::TransitEdgeRecord) -> f32 {
    let base = match edge.role.as_str() {
        "restricted_spine" => 0.9,
        "evacuation_route" => 0.75,
        "market_run" => 0.65,
        "maintenance_backbone" => 0.55,
        _ => 0.45,
    };
    (base + edge.length as f32 / 180.0).clamp(0.0, 1.0)
}

fn faction_benefit(agenda: &str) -> &'static str {
    if agenda.contains("security") {
        "temporary checkpoint credentials"
    } else if agenda.contains("trade") {
        "safe passage through market congestion"
    } else if agenda.contains("repair") {
        "restored power on one service loop"
    } else if agenda.contains("salvage") {
        "alternate crawl route through debris"
    } else {
        "local guides reveal a hidden landmark"
    }
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
        assert_eq!(scenario.schema_version, "gibson.scenario.v3");
        assert_eq!(scenario.seed, structure.seed);
        assert!(!scenario.objective_route_ids.is_empty());
        assert!(!scenario.landmarks.is_empty());
        assert!(!scenario.objective_chains.is_empty());
        assert!(!scenario.route_constraints.is_empty());
        assert!(!scenario.hazard_timings.is_empty());
        assert!(!scenario.resource_objectives.is_empty());
        assert!((0.0..=1.0).contains(&scenario.difficulty_score));
        assert!(scenario.estimated_duration_minutes > 0);
        assert!(!scenario.balance_notes.is_empty());
        assert!(!scenario.alternate_endings.is_empty());
        assert!(to_json(&scenario).unwrap().contains("faction_conflicts"));
    }
}
