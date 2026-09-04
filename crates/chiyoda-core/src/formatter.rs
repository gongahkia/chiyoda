//! Canonical source formatting for reviewable and reproducible DSL programs.

use crate::model::{Connector, Proposition, Scenario};
use std::fmt::Write;

/// Render a typed scenario as canonical version-0.16 source.
#[must_use]
#[allow(clippy::too_many_lines)] // mirrors the complete declaration grammar in one reviewable serializer
pub fn format_scenario(scenario: &Scenario) -> String {
    let mut source = String::new();
    writeln!(source, "scenario \"{}\"", scenario.name).expect("writing to a string cannot fail");
    writeln!(source, "seed {}", scenario.seed).expect("writing to a string cannot fail");
    writeln!(source, "duration {}", duration(scenario.duration_s))
        .expect("writing to a string cannot fail");
    writeln!(source, "timestep {}", duration(scenario.timestep_s))
        .expect("writing to a string cannot fail");

    for surface in &scenario.surfaces {
        writeln!(
            source,
            "surface {} at {} size ({}, {})",
            surface.id,
            point(surface.origin),
            length(surface.width_m),
            length(surface.depth_m),
        )
        .expect("writing to a string cannot fail");
    }
    for obstacle in &scenario.obstacles {
        writeln!(
            source,
            "obstacle {} on {} at {} size ({}, {})",
            obstacle.id,
            obstacle.surface,
            point(obstacle.at),
            length(obstacle.width_m),
            length(obstacle.depth_m),
        )
        .expect("writing to a string cannot fail");
    }
    for waypoint in &scenario.waypoints {
        writeln!(
            source,
            "waypoint {} on {} at {} dwell {}",
            waypoint.id,
            waypoint.surface,
            point(waypoint.at),
            duration(waypoint.dwell_s),
        )
        .expect("writing to a string cannot fail");
    }
    for exit in &scenario.exits {
        writeln!(
            source,
            "exit {} on {} at {} width {}{}",
            exit.id,
            exit.surface,
            point(exit.at),
            length(exit.width_m),
            connector_capacity(exit.capacity_per_s),
        )
        .expect("writing to a string cannot fail");
    }
    for connector in &scenario.connectors {
        match connector {
            Connector::Stair {
                id,
                from_surface,
                from,
                to_surface,
                to,
                width_m,
                capacity_per_s,
                clearance_height_m,
            } => writeln!(
                source,
                "stair {id} from {from_surface} at {} to {to_surface} at {} width {}{}{}",
                point(*from),
                point(*to),
                length(*width_m),
                connector_capacity(*capacity_per_s),
                connector_clearance(*clearance_height_m),
            ),
            Connector::Ramp {
                id,
                from_surface,
                from,
                to_surface,
                to,
                width_m,
                capacity_per_s,
                clearance_height_m,
            } => writeln!(
                source,
                "ramp {id} from {from_surface} at {} to {to_surface} at {} width {}{}{}",
                point(*from),
                point(*to),
                length(*width_m),
                connector_capacity(*capacity_per_s),
                connector_clearance(*clearance_height_m),
            ),
            Connector::Escalator {
                id,
                from_surface,
                from,
                to_surface,
                to,
                width_m,
                belt_speed_mps,
                capacity_per_s,
                clearance_height_m,
            } => writeln!(
                source,
                "escalator {id} from {from_surface} at {} to {to_surface} at {} width {} belt {}{}{}",
                point(*from),
                point(*to),
                length(*width_m),
                speed(*belt_speed_mps),
                connector_capacity(*capacity_per_s),
                connector_clearance(*clearance_height_m),
            ),
            Connector::Lift {
                id,
                from_surface,
                from,
                to_surface,
                to,
                cabin_width_m,
                cabin_depth_m,
                capacity,
                cycle_s,
                clearance_height_m,
            } => writeln!(
                source,
                "lift {id} from {from_surface} at {} to {to_surface} at {} cabin {} {} capacity {capacity} cycle {}{}",
                point(*from),
                point(*to),
                length(*cabin_width_m),
                length(*cabin_depth_m),
                duration(*cycle_s),
                connector_clearance(*clearance_height_m),
            ),
        }
        .expect("writing to a string cannot fail");
    }
    for change in &scenario.connector_states {
        writeln!(
            source,
            "connector-state {} connector {} {} time {}",
            change.id,
            change.connector,
            if change.open { "open" } else { "closed" },
            duration(change.at_s),
        )
        .expect("writing to a string cannot fail");
    }
    for change in &scenario.exit_states {
        writeln!(
            source,
            "exit-state {} exit {} {} time {}",
            change.id,
            change.exit,
            if change.open { "open" } else { "closed" },
            duration(change.at_s),
        )
        .expect("writing to a string cannot fail");
    }
    for gate in &scenario.gates {
        writeln!(
            source,
            "gate {} on {} at {} width {} capacity {}/s to {}",
            gate.id,
            gate.surface,
            point(gate.at),
            length(gate.width_m),
            number(gate.service_rate_per_s),
            gate.destination,
        )
        .expect("writing to a string cannot fail");
    }
    for group in &scenario.agents {
        writeln!(
            source,
            "agents {} count {} on {} at {} to {} speed {} radius {} height {}{}{}{} release {}",
            group.id,
            group.count,
            group.surface,
            point(group.at),
            group.destination,
            speed(group.speed_mps),
            length(group.radius_m),
            length(group.height_m),
            journey_waypoints(&group.via),
            alternative_destinations(&group.alternative_destinations),
            excluded_connector_kinds(&group.excluded_connector_kinds),
            duration(group.release_at_s),
        )
        .expect("writing to a string cannot fail");
    }
    for message in &scenario.messages {
        writeln!(
            source,
            "message {} source {} on {} at {} claim {} truth {} time {} reach {} trust {}",
            message.id,
            message.source.as_str(),
            message.surface,
            point(message.origin),
            proposition(&message.claim),
            message.truthful,
            duration(message.at_s),
            length(message.reach_m),
            number(message.trust),
        )
        .expect("writing to a string cannot fail");
    }
    for countermeasure in &scenario.countermeasures {
        writeln!(
            source,
            "countermeasure {} corrects {} source {} on {} at {} time {} reach {} trust {}",
            countermeasure.id,
            countermeasure.corrects,
            countermeasure.source.as_str(),
            countermeasure.surface,
            point(countermeasure.origin),
            duration(countermeasure.at_s),
            length(countermeasure.reach_m),
            number(countermeasure.trust),
        )
        .expect("writing to a string cannot fail");
    }
    source
}

fn proposition(proposition: &Proposition) -> String {
    match proposition {
        Proposition::ConnectorAvailability { connector, open } => {
            format!(
                "connector {connector} {}",
                if *open { "open" } else { "closed" }
            )
        }
        Proposition::ExitAvailability { exit, open } => {
            format!("exit {exit} {}", if *open { "open" } else { "closed" })
        }
    }
}

fn point(point: crate::model::Point3) -> String {
    format!(
        "({}, {}, {})",
        length(point.x_m),
        length(point.y_m),
        length(point.z_m)
    )
}

fn length(value: f64) -> String {
    format!("{}m", number(value))
}

fn duration(value: f64) -> String {
    format!("{}s", number(value))
}

fn speed(value: f64) -> String {
    format!("{}m/s", number(value))
}

fn connector_capacity(value: Option<f64>) -> String {
    value.map_or_else(String::new, |value| {
        format!(" capacity {}/s", number(value))
    })
}

fn connector_clearance(value: Option<f64>) -> String {
    value.map_or_else(String::new, |value| format!(" clearance {}", length(value)))
}

fn journey_waypoints(waypoints: &[String]) -> String {
    let mut rendered = String::new();
    for waypoint in waypoints {
        write!(rendered, " via {waypoint}").expect("writing to a string cannot fail");
    }
    rendered
}

fn alternative_destinations(destinations: &[String]) -> String {
    let mut rendered = String::new();
    for destination in destinations {
        write!(rendered, " alternative {destination}").expect("writing to a string cannot fail");
    }
    rendered
}

fn excluded_connector_kinds(kinds: &[crate::model::ConnectorKind]) -> String {
    let mut rendered = String::new();
    for kind in kinds {
        write!(rendered, " exclude {}", kind.as_str()).expect("writing to a string cannot fail");
    }
    rendered
}

fn number(value: f64) -> String {
    let rendered = format!("{value:.9}");
    rendered
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}
