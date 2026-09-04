//! Canonical source formatting for reviewable and reproducible DSL programs.

use crate::model::{Connector, Proposition, Scenario};
use std::fmt::Write;

/// Render a typed scenario as canonical version-0.1 source.
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
    for exit in &scenario.exits {
        writeln!(
            source,
            "exit {} on {} at {} width {}",
            exit.id,
            exit.surface,
            point(exit.at),
            length(exit.width_m),
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
            } => writeln!(
                source,
                "stair {id} from {from_surface} at {} to {to_surface} at {} width {}",
                point(*from),
                point(*to),
                length(*width_m),
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
            } => writeln!(
                source,
                "lift {id} from {from_surface} at {} to {to_surface} at {} cabin {} {} capacity {capacity} cycle {}",
                point(*from),
                point(*to),
                length(*cabin_width_m),
                length(*cabin_depth_m),
                duration(*cycle_s),
            ),
        }
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
            "agents {} count {} on {} at {} to {} speed {} radius {} height {}",
            group.id,
            group.count,
            group.surface,
            point(group.at),
            group.destination,
            speed(group.speed_mps),
            length(group.radius_m),
            length(group.height_m),
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

fn number(value: f64) -> String {
    let rendered = format!("{value:.9}");
    rendered
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}
