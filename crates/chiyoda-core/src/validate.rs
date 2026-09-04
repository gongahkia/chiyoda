use crate::model::{Connector, Point3, Scenario, Surface};
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

    for (index, exit) in scenario.exits.iter().enumerate() {
        let path = format!("exits[{index}]");
        check_unique(&mut ids, &exit.id, &path, &mut errors);
        check_positive(&format!("{path}.width_m"), exit.width_m, &mut errors);
        check_point(
            &surfaces,
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
        check_point(
            &surfaces,
            connector.from_surface(),
            connector.from(),
            &format!("{path}.from"),
            &mut errors,
        );
        check_point(
            &surfaces,
            connector.to_surface(),
            connector.to(),
            &format!("{path}.to"),
            &mut errors,
        );
        if connector.from_surface() == connector.to_surface() {
            errors.push(issue(&path, "must connect two distinct surfaces"));
        }
        match connector {
            Connector::Stair { width_m, .. } => {
                check_positive(&format!("{path}.width_m"), *width_m, &mut errors);
            }
            Connector::Lift {
                cabin_width_m,
                cabin_depth_m,
                capacity,
                cycle_s,
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
                if *capacity == 0 {
                    errors.push(issue(
                        &format!("{path}.capacity"),
                        "must be greater than zero",
                    ));
                }
            }
        }
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
        check_point(
            &surfaces,
            &gate.surface,
            gate.at,
            &format!("{path}.at"),
            &mut errors,
        );
    }

    let exit_ids: HashSet<&str> = scenario.exits.iter().map(|exit| exit.id.as_str()).collect();
    for (index, group) in scenario.agents.iter().enumerate() {
        let path = format!("agents[{index}]");
        check_unique(&mut ids, &group.id, &path, &mut errors);
        if group.count == 0 {
            errors.push(issue(&format!("{path}.count"), "must be greater than zero"));
        }
        check_positive(&format!("{path}.speed_mps"), group.speed_mps, &mut errors);
        check_positive(&format!("{path}.radius_m"), group.radius_m, &mut errors);
        check_positive(&format!("{path}.height_m"), group.height_m, &mut errors);
        check_point(
            &surfaces,
            &group.surface,
            group.at,
            &format!("{path}.at"),
            &mut errors,
        );
        if !exit_ids.contains(group.destination.as_str()) {
            errors.push(issue(
                &format!("{path}.destination"),
                format!("unknown exit `{}`", group.destination),
            ));
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
        if !scenario
            .connectors
            .iter()
            .any(|connector| connector.id() == message.claim.connector())
        {
            errors.push(issue(
                &format!("{path}.claim"),
                format!(
                    "references unknown connector `{}`",
                    message.claim.connector()
                ),
            ));
        }
        check_point(
            &surfaces,
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
            && message.truthful
        {
            errors.push(issue(
                &format!("{path}.corrects"),
                "may only correct a message declared `truth false`",
            ));
        }
        check_point(
            &surfaces,
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
    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();
    for connector in &scenario.connectors {
        graph
            .entry(connector.from_surface())
            .or_default()
            .push(connector.to_surface());
    }
    for (index, group) in scenario.agents.iter().enumerate() {
        let Some(&exit_surface) = exit_surfaces.get(group.destination.as_str()) else {
            continue;
        };
        let mut visited = HashSet::from([group.surface.as_str()]);
        let mut queue = VecDeque::from([group.surface.as_str()]);
        while let Some(current) = queue.pop_front() {
            for next in graph.get(current).into_iter().flatten() {
                if visited.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        if !visited.contains(exit_surface) {
            errors.push(issue(
                &format!("agents[{index}].destination"),
                format!(
                    "exit `{}` on surface `{exit_surface}` is unreachable from `{}`",
                    group.destination, group.surface
                ),
            ));
        }
    }
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
