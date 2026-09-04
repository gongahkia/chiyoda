use crate::model::{
    Connector, NAVIGATION_CLEARANCE_EPSILON_M, Obstacle, Point3, Scenario, Surface,
};
use std::{
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    fmt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Check the source-level invariants required for a scenario to be meaningful.
///
/// This verifies topology and declared resources, not real-world safety or
/// evacuation outcomes. A successful validation is therefore a precondition
/// for an experiment, not evidence that the experiment is calibrated.
#[allow(clippy::too_many_lines)] // all public scenario invariants are intentionally visible together
pub fn validate(scenario: &Scenario) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    let mut ids = BTreeSet::new();
    check_nonempty("scenario.name", &scenario.name, &mut errors);
    check_positive("duration", scenario.duration_s, &mut errors);
    check_positive("timestep", scenario.timestep_s, &mut errors);
    if scenario.timestep_s > scenario.duration_s {
        errors.push(issue("timestep", "must not exceed duration"));
    }

    for (index, surface) in scenario.surfaces.iter().enumerate() {
        let path = format!("surfaces[{index}]");
        check_unique(&mut ids, &surface.id, &path, &mut errors);
        check_positive(&format!("{path}.width_m"), surface.width_m, &mut errors);
        check_positive(&format!("{path}.depth_m"), surface.depth_m, &mut errors);
    }
    let surfaces: HashMap<&str, &Surface> = scenario
        .surfaces
        .iter()
        .map(|surface| (surface.id.as_str(), surface))
        .collect();
    if surfaces.is_empty() {
        errors.push(issue("surfaces", "at least one surface is required"));
    }

    for (index, obstacle) in scenario.obstacles.iter().enumerate() {
        let path = format!("obstacles[{index}]");
        check_unique(&mut ids, &obstacle.id, &path, &mut errors);
        check_positive(&format!("{path}.width_m"), obstacle.width_m, &mut errors);
        check_positive(&format!("{path}.depth_m"), obstacle.depth_m, &mut errors);
        check_point(
            &surfaces,
            &obstacle.surface,
            obstacle.at,
            &format!("{path}.at"),
            &mut errors,
        );
        check_point(
            &surfaces,
            &obstacle.surface,
            Point3 {
                x_m: obstacle.at.x_m + obstacle.width_m,
                y_m: obstacle.at.y_m + obstacle.depth_m,
                z_m: obstacle.at.z_m,
            },
            &format!("{path}.extent"),
            &mut errors,
        );
    }

    for (index, waypoint) in scenario.waypoints.iter().enumerate() {
        let path = format!("waypoints[{index}]");
        check_unique(&mut ids, &waypoint.id, &path, &mut errors);
        check_walkable_point(
            &surfaces,
            &scenario.obstacles,
            &waypoint.surface,
            waypoint.at,
            &format!("{path}.at"),
            &mut errors,
        );
        check_nonnegative(&format!("{path}.dwell_s"), waypoint.dwell_s, &mut errors);
    }

    for (index, exit) in scenario.exits.iter().enumerate() {
        let path = format!("exits[{index}]");
        check_unique(&mut ids, &exit.id, &path, &mut errors);
        check_positive(&format!("{path}.width_m"), exit.width_m, &mut errors);
        check_optional_positive(
            &format!("{path}.capacity_per_s"),
            exit.capacity_per_s,
            &mut errors,
        );
        check_walkable_point(
            &surfaces,
            &scenario.obstacles,
            &exit.surface,
            exit.at,
            &format!("{path}.at"),
            &mut errors,
        );
    }
    if scenario.exits.is_empty() {
        errors.push(issue("exits", "at least one exit is required"));
    }

    for (index, connector) in scenario.connectors.iter().enumerate() {
        let path = format!("connectors[{index}]");
        check_unique(&mut ids, connector.id(), &path, &mut errors);
        check_walkable_point(
            &surfaces,
            &scenario.obstacles,
            connector.from_surface(),
            connector.from(),
            &format!("{path}.from"),
            &mut errors,
        );
        check_walkable_point(
            &surfaces,
            &scenario.obstacles,
            connector.to_surface(),
            connector.to(),
            &format!("{path}.to"),
            &mut errors,
        );
        if connector.from_surface() == connector.to_surface() {
            errors.push(issue(&path, "must connect two distinct surfaces"));
        }
        match connector {
            Connector::Stair {
                width_m,
                capacity_per_s,
                clearance_height_m,
                ..
            }
            | Connector::Ramp {
                width_m,
                capacity_per_s,
                clearance_height_m,
                ..
            } => {
                check_positive(&format!("{path}.width_m"), *width_m, &mut errors);
                check_optional_positive(
                    &format!("{path}.capacity_per_s"),
                    *capacity_per_s,
                    &mut errors,
                );
                check_optional_positive(
                    &format!("{path}.clearance_height_m"),
                    *clearance_height_m,
                    &mut errors,
                );
            }
            Connector::Escalator {
                width_m,
                belt_speed_mps,
                capacity_per_s,
                clearance_height_m,
                ..
            } => {
                check_positive(&format!("{path}.width_m"), *width_m, &mut errors);
                check_positive(
                    &format!("{path}.belt_speed_mps"),
                    *belt_speed_mps,
                    &mut errors,
                );
                check_optional_positive(
                    &format!("{path}.capacity_per_s"),
                    *capacity_per_s,
                    &mut errors,
                );
                check_optional_positive(
                    &format!("{path}.clearance_height_m"),
                    *clearance_height_m,
                    &mut errors,
                );
            }
            Connector::Lift {
                cabin_width_m,
                cabin_depth_m,
                capacity,
                cycle_s,
                clearance_height_m,
                ..
            } => {
                check_positive(
                    &format!("{path}.cabin_width_m"),
                    *cabin_width_m,
                    &mut errors,
                );
                check_positive(
                    &format!("{path}.cabin_depth_m"),
                    *cabin_depth_m,
                    &mut errors,
                );
                check_positive(&format!("{path}.cycle_s"), *cycle_s, &mut errors);
                check_optional_positive(
                    &format!("{path}.clearance_height_m"),
                    *clearance_height_m,
                    &mut errors,
                );
                if *capacity == 0 {
                    errors.push(issue(
                        &format!("{path}.capacity"),
                        "must be greater than zero",
                    ));
                }
            }
        }
    }

    let connector_ids: HashSet<&str> = scenario.connectors.iter().map(Connector::id).collect();
    for (index, change) in scenario.connector_states.iter().enumerate() {
        let path = format!("connector_states[{index}]");
        check_unique(&mut ids, &change.id, &path, &mut errors);
        if !connector_ids.contains(change.connector.as_str()) {
            errors.push(issue(
                &format!("{path}.connector"),
                format!("references unknown connector `{}`", change.connector),
            ));
        }
        check_time(
            &format!("{path}.at_s"),
            change.at_s,
            scenario.duration_s,
            &mut errors,
        );
    }

    for (index, gate) in scenario.gates.iter().enumerate() {
        let path = format!("gates[{index}]");
        check_unique(&mut ids, &gate.id, &path, &mut errors);
        check_positive(&format!("{path}.width_m"), gate.width_m, &mut errors);
        check_positive(
            &format!("{path}.service_rate_per_s"),
            gate.service_rate_per_s,
            &mut errors,
        );
        check_walkable_point(
            &surfaces,
            &scenario.obstacles,
            &gate.surface,
            gate.at,
            &format!("{path}.at"),
            &mut errors,
        );
        match scenario
            .exits
            .iter()
            .find(|exit| exit.id == gate.destination)
        {
            Some(exit) if exit.surface == gate.surface => {}
            Some(_) => errors.push(issue(
                &format!("{path}.destination"),
                "gate and controlled exit must be on the same surface",
            )),
            None => errors.push(issue(
                &format!("{path}.destination"),
                format!("references unknown exit `{}`", gate.destination),
            )),
        }
    }

    let exit_ids: HashSet<&str> = scenario.exits.iter().map(|exit| exit.id.as_str()).collect();
    let waypoint_ids: HashSet<&str> = scenario
        .waypoints
        .iter()
        .map(|waypoint| waypoint.id.as_str())
        .collect();
    for (index, group) in scenario.agents.iter().enumerate() {
        let path = format!("agents[{index}]");
        check_unique(&mut ids, &group.id, &path, &mut errors);
        if group.count == 0 {
            errors.push(issue(&format!("{path}.count"), "must be greater than zero"));
        }
        check_positive(&format!("{path}.speed_mps"), group.speed_mps, &mut errors);
        check_positive(&format!("{path}.radius_m"), group.radius_m, &mut errors);
        check_positive(&format!("{path}.height_m"), group.height_m, &mut errors);
        let mut excluded_connector_kinds = BTreeSet::new();
        for (kind_index, kind) in group.excluded_connector_kinds.iter().enumerate() {
            if !excluded_connector_kinds.insert(kind) {
                errors.push(issue(
                    &format!("{path}.excluded_connector_kinds[{kind_index}]"),
                    format!("duplicate excluded connector kind `{}`", kind.as_str()),
                ));
            }
        }
        check_time(
            &format!("{path}.release_at_s"),
            group.release_at_s,
            scenario.duration_s,
            &mut errors,
        );
        if group.radius_m.is_finite() && group.radius_m > 0.0 {
            check_agent_spawn(
                &surfaces,
                &scenario.obstacles,
                &group.surface,
                group.at,
                group.radius_m,
                &format!("{path}.at"),
                &mut errors,
            );
            for (ordinal, position) in group.spawn_positions().enumerate().skip(1) {
                let error_count = errors.len();
                check_agent_spawn(
                    &surfaces,
                    &scenario.obstacles,
                    &group.surface,
                    position,
                    group.radius_m,
                    &format!("{path}.spawn[{ordinal}]"),
                    &mut errors,
                );
                if errors.len() > error_count {
                    break;
                }
            }
        } else {
            check_walkable_point(
                &surfaces,
                &scenario.obstacles,
                &group.surface,
                group.at,
                &format!("{path}.at"),
                &mut errors,
            );
        }
        if !exit_ids.contains(group.destination.as_str()) {
            errors.push(issue(
                &format!("{path}.destination"),
                format!("unknown exit `{}`", group.destination),
            ));
        }
        let mut destinations = HashSet::from([group.destination.as_str()]);
        for (alternative_index, destination) in group.alternative_destinations.iter().enumerate() {
            if !exit_ids.contains(destination.as_str()) {
                errors.push(issue(
                    &format!("{path}.alternative_destinations[{alternative_index}]"),
                    format!("unknown exit `{destination}`"),
                ));
            }
            if !destinations.insert(destination) {
                errors.push(issue(
                    &format!("{path}.alternative_destinations[{alternative_index}]"),
                    format!("duplicate alternative exit `{destination}`"),
                ));
            }
        }
        for (waypoint_index, waypoint) in group.via.iter().enumerate() {
            if !waypoint_ids.contains(waypoint.as_str()) {
                errors.push(issue(
                    &format!("{path}.via[{waypoint_index}]"),
                    format!("references unknown waypoint `{waypoint}`"),
                ));
            }
        }
    }
    if scenario.agents.is_empty() {
        errors.push(issue("agents", "at least one agent group is required"));
    }

    let message_ids: HashSet<&str> = scenario
        .messages
        .iter()
        .map(|message| message.id.as_str())
        .collect();
    for (index, message) in scenario.messages.iter().enumerate() {
        let path = format!("messages[{index}]");
        check_unique(&mut ids, &message.id, &path, &mut errors);
        if connector_ids.contains(message.claim.connector()) {
            let claim_matches_reality = message.claim.is_open()
                == scenario.connector_open_at(message.claim.connector(), message.at_s);
            if message.truthful != claim_matches_reality {
                errors.push(issue(
                    &format!("{path}.truthful"),
                    "does not match the connector's authored physical state at message time",
                ));
            }
        } else {
            errors.push(issue(
                &format!("{path}.claim"),
                format!(
                    "references unknown connector `{}`",
                    message.claim.connector()
                ),
            ));
        }
        check_walkable_point(
            &surfaces,
            &scenario.obstacles,
            &message.surface,
            message.origin,
            &format!("{path}.origin"),
            &mut errors,
        );
        check_time(&path, message.at_s, scenario.duration_s, &mut errors);
        check_positive(&format!("{path}.reach_m"), message.reach_m, &mut errors);
        check_probability(&format!("{path}.trust"), message.trust, &mut errors);
    }
    for (index, countermeasure) in scenario.countermeasures.iter().enumerate() {
        let path = format!("countermeasures[{index}]");
        check_unique(&mut ids, &countermeasure.id, &path, &mut errors);
        if !message_ids.contains(countermeasure.corrects.as_str()) {
            errors.push(issue(
                &format!("{path}.corrects"),
                format!("unknown message `{}`", countermeasure.corrects),
            ));
        }
        if let Some(message) = scenario
            .messages
            .iter()
            .find(|message| message.id == countermeasure.corrects)
        {
            if message.truthful {
                errors.push(issue(
                    &format!("{path}.corrects"),
                    "may only correct a message declared `truth false`",
                ));
            }
            if countermeasure.at_s < message.at_s {
                errors.push(issue(
                    &format!("{path}.at_s"),
                    "must not precede the message it corrects",
                ));
            }
        }
        check_walkable_point(
            &surfaces,
            &scenario.obstacles,
            &countermeasure.surface,
            countermeasure.origin,
            &format!("{path}.origin"),
            &mut errors,
        );
        check_time(&path, countermeasure.at_s, scenario.duration_s, &mut errors);
        check_positive(
            &format!("{path}.reach_m"),
            countermeasure.reach_m,
            &mut errors,
        );
        check_probability(&format!("{path}.trust"), countermeasure.trust, &mut errors);
    }

    if errors.is_empty() {
        check_reachability(scenario, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_reachability(scenario: &Scenario, errors: &mut Vec<ValidationError>) {
    let exit_surfaces: HashMap<&str, &str> = scenario
        .exits
        .iter()
        .map(|exit| (exit.id.as_str(), exit.surface.as_str()))
        .collect();
    let waypoint_surfaces: HashMap<&str, &str> = scenario
        .waypoints
        .iter()
        .map(|waypoint| (waypoint.id.as_str(), waypoint.surface.as_str()))
        .collect();
    for (index, group) in scenario.agents.iter().enumerate() {
        let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();
        for connector in scenario.connectors.iter().filter(|connector| {
            connector.supports_height(group.height_m) && group.allows_connector(connector)
        }) {
            graph
                .entry(connector.from_surface())
                .or_default()
                .push(connector.to_surface());
        }
        let mut current_surface = group.surface.as_str();
        for (waypoint_index, waypoint_id) in group.via.iter().enumerate() {
            let Some(&waypoint_surface) = waypoint_surfaces.get(waypoint_id.as_str()) else {
                continue;
            };
            if !surface_is_reachable(&graph, current_surface, waypoint_surface) {
                errors.push(issue(
                    &format!("agents[{index}].via[{waypoint_index}]"),
                    format!(
                        "waypoint `{waypoint_id}` on surface `{waypoint_surface}` is unreachable from `{current_surface}`"
                    ),
                ));
                break;
            }
            current_surface = waypoint_surface;
        }
        for (destination_index, destination) in group.exit_candidates().enumerate() {
            let Some(&exit_surface) = exit_surfaces.get(destination) else {
                continue;
            };
            if !surface_is_reachable(&graph, current_surface, exit_surface) {
                let path = if destination_index == 0 {
                    format!("agents[{index}].destination")
                } else {
                    format!("agents[{index}].alternative_destinations[{}]", destination_index - 1)
                };
                errors.push(issue(
                    &path,
                    format!(
                        "exit `{destination}` on surface `{exit_surface}` is unreachable from `{current_surface}`"
                    ),
                ));
            }
        }
    }
}

fn surface_is_reachable(
    graph: &HashMap<&str, Vec<&str>>,
    start_surface: &str,
    target_surface: &str,
) -> bool {
    let mut visited = HashSet::from([start_surface]);
    let mut queue = VecDeque::from([start_surface]);
    while let Some(current) = queue.pop_front() {
        for next in graph.get(current).into_iter().flatten() {
            if visited.insert(next) {
                queue.push_back(next);
            }
        }
    }
    visited.contains(target_surface)
}

fn check_point(
    surfaces: &HashMap<&str, &Surface>,
    surface_id: &str,
    point: Point3,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    match surfaces.get(surface_id) {
        Some(surface) if surface.contains(point) => {}
        Some(_) => errors.push(issue(path, format!("is outside surface `{surface_id}`"))),
        None => errors.push(issue(
            path,
            format!("references unknown surface `{surface_id}`"),
        )),
    }
}

fn check_walkable_point(
    surfaces: &HashMap<&str, &Surface>,
    obstacles: &[Obstacle],
    surface_id: &str,
    point: Point3,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    let Some(surface) = surfaces.get(surface_id) else {
        errors.push(issue(
            path,
            format!("references unknown surface `{surface_id}`"),
        ));
        return;
    };
    if !surface.contains(point) {
        errors.push(issue(path, format!("is outside surface `{surface_id}`")));
        return;
    }
    if obstacles
        .iter()
        .any(|obstacle| obstacle.surface == surface_id && obstacle.contains(point))
    {
        errors.push(issue(
            path,
            format!("is inside an obstacle on `{surface_id}`"),
        ));
    }
}

fn check_agent_spawn(
    surfaces: &HashMap<&str, &Surface>,
    obstacles: &[Obstacle],
    surface_id: &str,
    point: Point3,
    radius_m: f64,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    let error_count = errors.len();
    check_walkable_point(surfaces, obstacles, surface_id, point, path, errors);
    if errors.len() > error_count {
        return;
    }
    if let Some(obstacle) = obstacles.iter().find(|obstacle| {
        obstacle.surface == surface_id
            && obstacle.contains_with_clearance(point, radius_m + NAVIGATION_CLEARANCE_EPSILON_M)
    }) {
        errors.push(issue(
            path,
            format!(
                "does not clear obstacle `{}` on `{surface_id}` by the agent radius",
                obstacle.id
            ),
        ));
    }
}

fn check_unique(
    ids: &mut BTreeSet<String>,
    id: &str,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    if id.trim().is_empty() {
        errors.push(issue(path, "identifier must not be empty"));
    } else if !ids.insert(id.to_owned()) {
        errors.push(issue(path, format!("duplicate identifier `{id}`")));
    }
}

fn check_positive(path: &str, value: f64, errors: &mut Vec<ValidationError>) {
    if !value.is_finite() || value <= 0.0 {
        errors.push(issue(path, "must be finite and greater than zero"));
    }
}

fn check_nonnegative(path: &str, value: f64, errors: &mut Vec<ValidationError>) {
    if !value.is_finite() || value < 0.0 {
        errors.push(issue(path, "must be finite and non-negative"));
    }
}

fn check_optional_positive(path: &str, value: Option<f64>, errors: &mut Vec<ValidationError>) {
    if let Some(value) = value {
        check_positive(path, value, errors);
    }
}

fn check_probability(path: &str, value: f64, errors: &mut Vec<ValidationError>) {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        errors.push(issue(path, "must be between zero and one"));
    }
}

fn check_time(path: &str, value: f64, duration_s: f64, errors: &mut Vec<ValidationError>) {
    if !value.is_finite() || value < 0.0 || value > duration_s {
        errors.push(issue(path, "must occur within the scenario duration"));
    }
}

fn check_nonempty(path: &str, value: &str, errors: &mut Vec<ValidationError>) {
    if value.trim().is_empty() {
        errors.push(issue(path, "must not be empty"));
    }
}

fn issue(path: &str, message: impl Into<String>) -> ValidationError {
    ValidationError {
        path: path.to_owned(),
        message: message.into(),
    }
}
