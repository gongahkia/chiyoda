use crate::model::{
    AgentGroup, Connector, ConnectorCapacityChange, ConnectorKind, ConnectorStateChange,
    Countermeasure, Exit, ExitCapacityChange, ExitStateChange, Gate, GateCapacityChange,
    GateStateChange, InformationSource, Message, Obstacle, Point3, Scenario, Surface, Waypoint,
};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Default)]
struct ScenarioBuilder {
    name: Option<String>,
    seed: Option<u64>,
    duration_s: Option<f64>,
    timestep_s: Option<f64>,
    surfaces: Vec<Surface>,
    obstacles: Vec<Obstacle>,
    waypoints: Vec<Waypoint>,
    exits: Vec<Exit>,
    connectors: Vec<Connector>,
    connector_states: Vec<ConnectorStateChange>,
    exit_states: Vec<ExitStateChange>,
    connector_capacity_states: Vec<ConnectorCapacityChange>,
    exit_capacity_states: Vec<ExitCapacityChange>,
    gates: Vec<Gate>,
    gate_states: Vec<GateStateChange>,
    gate_capacity_states: Vec<GateCapacityChange>,
    agents: Vec<AgentGroup>,
    messages: Vec<Message>,
    countermeasures: Vec<Countermeasure>,
}

#[derive(Debug, Default)]
struct AgentOptions {
    release_at_s: f64,
    release_interval_s: Option<f64>,
    release_batch_size: Option<u32>,
    via: Vec<String>,
    excluded_connector_kinds: Vec<ConnectorKind>,
    alternative_destinations: Vec<String>,
}

impl ScenarioBuilder {
    fn finish(self, line: usize) -> Result<Scenario, ParseError> {
        Ok(Scenario {
            name: self
                .name
                .ok_or_else(|| error(line, "missing `scenario` declaration"))?,
            seed: self
                .seed
                .ok_or_else(|| error(line, "missing `seed` declaration"))?,
            duration_s: self
                .duration_s
                .ok_or_else(|| error(line, "missing `duration` declaration"))?,
            timestep_s: self
                .timestep_s
                .ok_or_else(|| error(line, "missing `timestep` declaration"))?,
            surfaces: self.surfaces,
            obstacles: self.obstacles,
            waypoints: self.waypoints,
            exits: self.exits,
            connectors: self.connectors,
            connector_states: self.connector_states,
            exit_states: self.exit_states,
            connector_capacity_states: self.connector_capacity_states,
            exit_capacity_states: self.exit_capacity_states,
            gates: self.gates,
            gate_states: self.gate_states,
            gate_capacity_states: self.gate_capacity_states,
            agents: self.agents,
            messages: self.messages,
            countermeasures: self.countermeasures,
        })
    }
}

/// Parse the line-oriented Chiyoda experiment language.
///
/// Source declarations are intentionally flat. This makes experiment diffs,
/// source provenance, and diagnostics straightforward while preserving a
/// complete typed representation for the reference interpreter.
pub fn parse(source: &str) -> Result<Scenario, ParseError> {
    let mut builder = ScenarioBuilder::default();
    let mut last_line = 1;

    for (index, raw_line) in source.lines().enumerate() {
        let line = index + 1;
        last_line = line;
        let raw_line = raw_line.split('#').next().unwrap_or_default().trim();
        if raw_line.is_empty() {
            continue;
        }
        let tokens = tokenize(raw_line).map_err(|message| error(line, message))?;
        if tokens.is_empty() {
            continue;
        }
        parse_declaration(line, &tokens, &mut builder)?;
    }

    builder.finish(last_line)
}

#[allow(clippy::too_many_lines)] // one exhaustive declaration-to-AST mapping keeps grammar reviewable
fn parse_declaration(
    line: usize,
    tokens: &[String],
    builder: &mut ScenarioBuilder,
) -> Result<(), ParseError> {
    let keyword = required(line, tokens, 0, "declaration keyword")?;
    match keyword {
        "scenario" => {
            reject_duplicate(line, builder.name.is_some(), "scenario")?;
            require_exact_count(line, tokens, 2)?;
            builder.name = Some(tokens[1].clone());
        }
        "seed" => {
            reject_duplicate(line, builder.seed.is_some(), "seed")?;
            require_exact_count(line, tokens, 2)?;
            builder.seed = Some(parse_plain(line, &tokens[1], "seed")?);
        }
        "duration" => {
            reject_duplicate(line, builder.duration_s.is_some(), "duration")?;
            require_exact_count(line, tokens, 2)?;
            builder.duration_s = Some(parse_duration(line, &tokens[1])?);
        }
        "timestep" => {
            reject_duplicate(line, builder.timestep_s.is_some(), "timestep")?;
            require_exact_count(line, tokens, 2)?;
            builder.timestep_s = Some(parse_duration(line, &tokens[1])?);
        }
        "surface" => {
            require_exact_count(line, tokens, 9)?;
            expect(line, tokens, 2, "at")?;
            expect(line, tokens, 6, "size")?;
            builder.surfaces.push(Surface {
                id: tokens[1].clone(),
                origin: point(line, tokens, 3)?,
                width_m: parse_length(line, required(line, tokens, 7, "surface width")?)?,
                depth_m: parse_length(line, required(line, tokens, 8, "surface depth")?)?,
            });
        }
        "obstacle" => {
            require_exact_count(line, tokens, 11)?;
            expect(line, tokens, 2, "on")?;
            expect(line, tokens, 4, "at")?;
            expect(line, tokens, 8, "size")?;
            builder.obstacles.push(Obstacle {
                id: tokens[1].clone(),
                surface: tokens[3].clone(),
                at: point(line, tokens, 5)?,
                width_m: parse_length(line, required(line, tokens, 9, "obstacle width")?)?,
                depth_m: parse_length(line, required(line, tokens, 10, "obstacle depth")?)?,
            });
        }
        "waypoint" => {
            require_count(line, tokens, 8)?;
            expect(line, tokens, 2, "on")?;
            expect(line, tokens, 4, "at")?;
            let dwell_s = match tokens.get(8).map(String::as_str) {
                None if tokens.len() == 8 => 0.0,
                Some("dwell") if tokens.len() == 10 => {
                    parse_duration(line, required(line, tokens, 9, "waypoint dwell time")?)?
                }
                Some("dwell") => {
                    return Err(error(
                        line,
                        "waypoint dwell must be the final `dwell DURATION` clause",
                    ));
                }
                Some(actual) => {
                    return Err(error(line, format!("expected `dwell`, found `{actual}`")));
                }
                None => unreachable!("the minimum token count was checked"),
            };
            builder.waypoints.push(Waypoint {
                id: tokens[1].clone(),
                surface: tokens[3].clone(),
                at: point(line, tokens, 5)?,
                dwell_s,
            });
        }
        "exit" => {
            require_count(line, tokens, 10)?;
            expect(line, tokens, 2, "on")?;
            expect(line, tokens, 4, "at")?;
            expect(line, tokens, 8, "width")?;
            let capacity_per_s = optional_exit_capacity(line, tokens)?;
            builder.exits.push(Exit {
                id: tokens[1].clone(),
                surface: tokens[3].clone(),
                at: point(line, tokens, 5)?,
                width_m: parse_length(line, required(line, tokens, 9, "exit width")?)?,
                capacity_per_s,
            });
        }
        "stair" => {
            require_count(line, tokens, 16)?;
            expect(line, tokens, 2, "from")?;
            expect(line, tokens, 4, "at")?;
            expect(line, tokens, 8, "to")?;
            expect(line, tokens, 10, "at")?;
            expect(line, tokens, 14, "width")?;
            let (capacity_per_s, clearance_height_m) = connector_options(line, tokens, 16)?;
            builder.connectors.push(Connector::Stair {
                id: tokens[1].clone(),
                from_surface: tokens[3].clone(),
                from: point(line, tokens, 5)?,
                to_surface: tokens[9].clone(),
                to: point(line, tokens, 11)?,
                width_m: parse_length(line, required(line, tokens, 15, "stair width")?)?,
                capacity_per_s,
                clearance_height_m,
            });
        }
        "ramp" => {
            require_count(line, tokens, 16)?;
            expect(line, tokens, 2, "from")?;
            expect(line, tokens, 4, "at")?;
            expect(line, tokens, 8, "to")?;
            expect(line, tokens, 10, "at")?;
            expect(line, tokens, 14, "width")?;
            let (capacity_per_s, clearance_height_m) = connector_options(line, tokens, 16)?;
            builder.connectors.push(Connector::Ramp {
                id: tokens[1].clone(),
                from_surface: tokens[3].clone(),
                from: point(line, tokens, 5)?,
                to_surface: tokens[9].clone(),
                to: point(line, tokens, 11)?,
                width_m: parse_length(line, required(line, tokens, 15, "ramp width")?)?,
                capacity_per_s,
                clearance_height_m,
            });
        }
        "escalator" => {
            require_count(line, tokens, 18)?;
            expect(line, tokens, 2, "from")?;
            expect(line, tokens, 4, "at")?;
            expect(line, tokens, 8, "to")?;
            expect(line, tokens, 10, "at")?;
            expect(line, tokens, 14, "width")?;
            expect(line, tokens, 16, "belt")?;
            let (capacity_per_s, clearance_height_m) = connector_options(line, tokens, 18)?;
            builder.connectors.push(Connector::Escalator {
                id: tokens[1].clone(),
                from_surface: tokens[3].clone(),
                from: point(line, tokens, 5)?,
                to_surface: tokens[9].clone(),
                to: point(line, tokens, 11)?,
                width_m: parse_length(line, required(line, tokens, 15, "escalator width")?)?,
                belt_speed_mps: parse_speed(
                    line,
                    required(line, tokens, 17, "escalator belt speed")?,
                )?,
                capacity_per_s,
                clearance_height_m,
            });
        }
        "lift" => {
            require_count(line, tokens, 21)?;
            expect(line, tokens, 2, "from")?;
            expect(line, tokens, 4, "at")?;
            expect(line, tokens, 8, "to")?;
            expect(line, tokens, 10, "at")?;
            expect(line, tokens, 14, "cabin")?;
            expect(line, tokens, 17, "capacity")?;
            expect(line, tokens, 19, "cycle")?;
            builder.connectors.push(Connector::Lift {
                id: tokens[1].clone(),
                from_surface: tokens[3].clone(),
                from: point(line, tokens, 5)?,
                to_surface: tokens[9].clone(),
                to: point(line, tokens, 11)?,
                cabin_width_m: parse_length(line, required(line, tokens, 15, "cabin width")?)?,
                cabin_depth_m: parse_length(line, required(line, tokens, 16, "cabin depth")?)?,
                capacity: parse_plain(
                    line,
                    required(line, tokens, 18, "lift capacity")?,
                    "lift capacity",
                )?,
                cycle_s: parse_duration(line, required(line, tokens, 20, "lift cycle")?)?,
                clearance_height_m: optional_clearance(line, tokens, 21, "lift")?,
            });
        }
        "connector-state" => {
            require_exact_count(line, tokens, 7)?;
            expect(line, tokens, 2, "connector")?;
            expect(line, tokens, 5, "time")?;
            builder.connector_states.push(ConnectorStateChange {
                id: tokens[1].clone(),
                connector: tokens[3].clone(),
                open: parse_availability_open(line, required(line, tokens, 4, "connector state")?)?,
                at_s: parse_duration(line, required(line, tokens, 6, "connector state time")?)?,
            });
        }
        "exit-state" => {
            require_exact_count(line, tokens, 7)?;
            expect(line, tokens, 2, "exit")?;
            expect(line, tokens, 5, "time")?;
            builder.exit_states.push(ExitStateChange {
                id: tokens[1].clone(),
                exit: tokens[3].clone(),
                open: parse_availability_open(line, required(line, tokens, 4, "exit state")?)?,
                at_s: parse_duration(line, required(line, tokens, 6, "exit state time")?)?,
            });
        }
        "connector-capacity-state" => {
            require_exact_count(line, tokens, 8)?;
            expect(line, tokens, 2, "connector")?;
            expect(line, tokens, 4, "capacity")?;
            expect(line, tokens, 6, "time")?;
            builder
                .connector_capacity_states
                .push(ConnectorCapacityChange {
                    id: tokens[1].clone(),
                    connector: tokens[3].clone(),
                    capacity_per_s: parse_rate(
                        line,
                        required(line, tokens, 5, "connector capacity state")?,
                    )?,
                    at_s: parse_duration(
                        line,
                        required(line, tokens, 7, "connector capacity state time")?,
                    )?,
                });
        }
        "exit-capacity-state" => {
            require_exact_count(line, tokens, 8)?;
            expect(line, tokens, 2, "exit")?;
            expect(line, tokens, 4, "capacity")?;
            expect(line, tokens, 6, "time")?;
            builder.exit_capacity_states.push(ExitCapacityChange {
                id: tokens[1].clone(),
                exit: tokens[3].clone(),
                capacity_per_s: parse_rate(
                    line,
                    required(line, tokens, 5, "exit capacity state")?,
                )?,
                at_s: parse_duration(line, required(line, tokens, 7, "exit capacity state time")?)?,
            });
        }
        "gate" => {
            require_exact_count(line, tokens, 14)?;
            expect(line, tokens, 2, "on")?;
            expect(line, tokens, 4, "at")?;
            expect(line, tokens, 8, "width")?;
            expect(line, tokens, 10, "capacity")?;
            expect(line, tokens, 12, "to")?;
            builder.gates.push(Gate {
                id: tokens[1].clone(),
                surface: tokens[3].clone(),
                at: point(line, tokens, 5)?,
                width_m: parse_length(line, required(line, tokens, 9, "gate width")?)?,
                service_rate_per_s: parse_rate(line, required(line, tokens, 11, "gate capacity")?)?,
                destination: tokens[13].clone(),
            });
        }
        "gate-state" => {
            require_exact_count(line, tokens, 7)?;
            expect(line, tokens, 2, "gate")?;
            expect(line, tokens, 5, "time")?;
            builder.gate_states.push(GateStateChange {
                id: tokens[1].clone(),
                gate: tokens[3].clone(),
                open: parse_availability_open(line, required(line, tokens, 4, "gate state")?)?,
                at_s: parse_duration(line, required(line, tokens, 6, "gate state time")?)?,
            });
        }
        "gate-capacity-state" => {
            require_exact_count(line, tokens, 8)?;
            expect(line, tokens, 2, "gate")?;
            expect(line, tokens, 4, "capacity")?;
            expect(line, tokens, 6, "time")?;
            builder.gate_capacity_states.push(GateCapacityChange {
                id: tokens[1].clone(),
                gate: tokens[3].clone(),
                capacity_per_s: parse_rate(
                    line,
                    required(line, tokens, 5, "gate capacity state")?,
                )?,
                at_s: parse_duration(line, required(line, tokens, 7, "gate capacity state time")?)?,
            });
        }
        "agents" => {
            require_count(line, tokens, 18)?;
            expect(line, tokens, 2, "count")?;
            expect(line, tokens, 4, "on")?;
            expect(line, tokens, 6, "at")?;
            expect(line, tokens, 10, "to")?;
            expect(line, tokens, 12, "speed")?;
            expect(line, tokens, 14, "radius")?;
            expect(line, tokens, 16, "height")?;
            let agent_options = agent_options(line, tokens, &tokens[11])?;
            builder.agents.push(AgentGroup {
                id: tokens[1].clone(),
                count: parse_plain(
                    line,
                    required(line, tokens, 3, "agent count")?,
                    "agent count",
                )?,
                surface: tokens[5].clone(),
                at: point(line, tokens, 7)?,
                destination: tokens[11].clone(),
                alternative_destinations: agent_options.alternative_destinations,
                speed_mps: parse_speed(line, required(line, tokens, 13, "agent speed")?)?,
                radius_m: parse_length(line, required(line, tokens, 15, "agent radius")?)?,
                height_m: parse_length(line, required(line, tokens, 17, "agent height")?)?,
                release_at_s: agent_options.release_at_s,
                release_interval_s: agent_options.release_interval_s,
                release_batch_size: agent_options.release_batch_size,
                via: agent_options.via,
                excluded_connector_kinds: agent_options.excluded_connector_kinds,
            });
        }
        "message" => {
            require_count(line, tokens, 22)?;
            let sampling_key = optional_sampling_key(line, tokens, 22, "message")?;
            expect(line, tokens, 2, "source")?;
            expect(line, tokens, 4, "on")?;
            expect(line, tokens, 6, "at")?;
            expect(line, tokens, 10, "claim")?;
            expect(line, tokens, 14, "truth")?;
            expect(line, tokens, 16, "time")?;
            expect(line, tokens, 18, "reach")?;
            expect(line, tokens, 20, "trust")?;
            builder.messages.push(Message {
                id: tokens[1].clone(),
                source: parse_source(line, required(line, tokens, 3, "message source")?)?,
                surface: tokens[5].clone(),
                origin: point(line, tokens, 7)?,
                claim: parse_claim(
                    line,
                    required(line, tokens, 11, "claim kind")?,
                    required(line, tokens, 12, "claim subject")?,
                    required(line, tokens, 13, "claim state")?,
                )?,
                truthful: parse_plain(
                    line,
                    required(line, tokens, 15, "message truth value")?,
                    "message truth value",
                )?,
                at_s: parse_duration(line, required(line, tokens, 17, "message time")?)?,
                reach_m: parse_length(line, required(line, tokens, 19, "message reach")?)?,
                trust: parse_plain(
                    line,
                    required(line, tokens, 21, "message trust")?,
                    "message trust",
                )?,
                sampling_key,
            });
        }
        "countermeasure" => {
            require_count(line, tokens, 18)?;
            let sampling_key = optional_sampling_key(line, tokens, 18, "countermeasure")?;
            expect(line, tokens, 2, "corrects")?;
            expect(line, tokens, 4, "source")?;
            expect(line, tokens, 6, "on")?;
            expect(line, tokens, 8, "at")?;
            expect(line, tokens, 12, "time")?;
            expect(line, tokens, 14, "reach")?;
            expect(line, tokens, 16, "trust")?;
            builder.countermeasures.push(Countermeasure {
                id: tokens[1].clone(),
                corrects: tokens[3].clone(),
                source: parse_countermeasure_source(
                    line,
                    required(line, tokens, 5, "countermeasure source")?,
                )?,
                surface: tokens[7].clone(),
                origin: point(line, tokens, 9)?,
                at_s: parse_duration(line, required(line, tokens, 13, "countermeasure time")?)?,
                reach_m: parse_length(line, required(line, tokens, 15, "countermeasure reach")?)?,
                trust: parse_plain(
                    line,
                    required(line, tokens, 17, "countermeasure trust")?,
                    "countermeasure trust",
                )?,
                sampling_key,
            });
        }
        other => return Err(error(line, format!("unknown declaration `{other}`"))),
    }
    Ok(())
}

fn tokenize(line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in line.chars() {
        match character {
            '"' => {
                if quoted {
                    tokens.push(std::mem::take(&mut current));
                } else if !current.is_empty() {
                    return Err("a quote must begin at a token boundary".to_owned());
                }
                quoted = !quoted;
            }
            '(' | ')' | ',' if !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            character if character.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if quoted {
        return Err("unterminated quoted string".to_owned());
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

fn point(line: usize, tokens: &[String], start: usize) -> Result<Point3, ParseError> {
    Ok(Point3 {
        x_m: parse_length(line, required(line, tokens, start, "x coordinate")?)?,
        y_m: parse_length(line, required(line, tokens, start + 1, "y coordinate")?)?,
        z_m: parse_length(line, required(line, tokens, start + 2, "z coordinate")?)?,
    })
}

fn parse_length(line: usize, value: &str) -> Result<f64, ParseError> {
    parse_unit(line, value, "m", "length")
}

fn parse_duration(line: usize, value: &str) -> Result<f64, ParseError> {
    if let Some(milliseconds) = value.strip_suffix("ms") {
        return parse_plain(line, milliseconds, "duration").map(|parsed: f64| parsed / 1_000.0);
    }
    parse_unit(line, value, "s", "duration")
}

fn parse_speed(line: usize, value: &str) -> Result<f64, ParseError> {
    parse_unit(line, value, "m/s", "speed")
}

fn parse_rate(line: usize, value: &str) -> Result<f64, ParseError> {
    parse_unit(line, value, "/s", "rate")
}

fn connector_options(
    line: usize,
    tokens: &[String],
    start: usize,
) -> Result<(Option<f64>, Option<f64>), ParseError> {
    let mut capacity_per_s = None;
    let mut clearance_height_m = None;
    let mut index = start;
    while let Some(clause) = tokens.get(index).map(String::as_str) {
        let value = required(line, tokens, index + 1, "connector option value")?;
        match clause {
            "capacity" if capacity_per_s.is_none() => {
                capacity_per_s = Some(parse_rate(line, value)?);
            }
            "capacity" => {
                return Err(error(line, "connector capacity may be declared only once"));
            }
            "clearance" if clearance_height_m.is_none() => {
                clearance_height_m = Some(parse_length(line, value)?);
            }
            "clearance" => {
                return Err(error(line, "connector clearance may be declared only once"));
            }
            actual => {
                return Err(error(
                    line,
                    format!("expected `capacity` or `clearance`, found `{actual}`"),
                ));
            }
        }
        index += 2;
    }
    Ok((capacity_per_s, clearance_height_m))
}

fn optional_clearance(
    line: usize,
    tokens: &[String],
    start: usize,
    subject: &str,
) -> Result<Option<f64>, ParseError> {
    match tokens.get(start).map(String::as_str) {
        None => Ok(None),
        Some("clearance") if tokens.len() == start + 2 => Ok(Some(parse_length(
            line,
            required(line, tokens, start + 1, "connector clearance")?,
        )?)),
        Some("clearance") => Err(error(
            line,
            format!("{subject} clearance must be the final `clearance LENGTH` clause"),
        )),
        Some(actual) => Err(error(
            line,
            format!("expected `clearance`, found `{actual}`"),
        )),
    }
}

fn optional_exit_capacity(line: usize, tokens: &[String]) -> Result<Option<f64>, ParseError> {
    match tokens.get(10).map(String::as_str) {
        None => Ok(None),
        Some("capacity") if tokens.len() == 12 => Ok(Some(parse_rate(
            line,
            required(line, tokens, 11, "exit capacity")?,
        )?)),
        Some("capacity") => Err(error(
            line,
            "exit capacity must be the final `capacity RATE` clause",
        )),
        Some(actual) => Err(error(
            line,
            format!("expected `capacity`, found `{actual}`"),
        )),
    }
}

fn agent_options(
    line: usize,
    tokens: &[String],
    primary_destination: &str,
) -> Result<AgentOptions, ParseError> {
    let mut release_at_s = None;
    let mut release_interval_s = None;
    let mut release_batch_size = None;
    let mut via = Vec::new();
    let mut excluded_connector_kinds = Vec::new();
    let mut alternative_destinations = Vec::new();
    let mut index = 18;
    while index < tokens.len() {
        match tokens[index].as_str() {
            "release" if release_at_s.is_none() => {
                release_at_s = Some(parse_duration(
                    line,
                    required(line, tokens, index + 1, "agent release time")?,
                )?);
                if tokens.get(index + 2).is_some_and(|token| token == "every") {
                    release_interval_s = Some(parse_duration(
                        line,
                        required(line, tokens, index + 3, "agent release interval")?,
                    )?);
                    index += 2;
                    if tokens.get(index + 2).is_some_and(|token| token == "batch") {
                        release_batch_size = Some(parse_plain(
                            line,
                            required(line, tokens, index + 3, "agent release batch size")?,
                            "agent release batch size",
                        )?);
                        index += 2;
                    }
                }
            }
            "via" => via.push(required(line, tokens, index + 1, "journey waypoint")?.to_owned()),
            "alternative" => {
                let exit_id = required(line, tokens, index + 1, "alternative exit")?;
                if exit_id == primary_destination
                    || alternative_destinations
                        .iter()
                        .any(|alternative| alternative == exit_id)
                {
                    return Err(error(
                        line,
                        format!("duplicate alternative exit `{exit_id}`"),
                    ));
                }
                alternative_destinations.push(exit_id.to_owned());
            }
            "exclude" => {
                let connector_kind = parse_connector_kind(
                    line,
                    required(line, tokens, index + 1, "excluded connector kind")?,
                )?;
                if excluded_connector_kinds.contains(&connector_kind) {
                    return Err(error(
                        line,
                        format!("duplicate `exclude {}` clause", connector_kind.as_str()),
                    ));
                }
                excluded_connector_kinds.push(connector_kind);
            }
            "batch" => {
                return Err(error(
                    line,
                    "`batch` requires `release DURATION every DURATION`",
                ));
            }
            "release" => return Err(error(line, "duplicate `release` clause")),
            actual => return Err(error(line, format!("unknown agent option `{actual}`"))),
        }
        index += 2;
    }
    excluded_connector_kinds.sort_unstable();
    Ok(AgentOptions {
        release_at_s: release_at_s.unwrap_or(0.0),
        release_interval_s,
        release_batch_size,
        via,
        excluded_connector_kinds,
        alternative_destinations,
    })
}

fn parse_unit(line: usize, value: &str, suffix: &str, label: &str) -> Result<f64, ParseError> {
    let number = value
        .strip_suffix(suffix)
        .ok_or_else(|| error(line, format!("{label} `{value}` must use `{suffix}` units")))?;
    parse_plain(line, number, label)
}

fn parse_source(line: usize, value: &str) -> Result<InformationSource, ParseError> {
    match value {
        "peer" => Ok(InformationSource::Peer),
        "official" => Ok(InformationSource::Official),
        "signage" => Ok(InformationSource::Signage),
        "staff" => Ok(InformationSource::Staff),
        _ => Err(error(
            line,
            format!("unknown information source `{value}`; use peer, official, signage, or staff"),
        )),
    }
}

fn parse_countermeasure_source(line: usize, value: &str) -> Result<InformationSource, ParseError> {
    match parse_source(line, value)? {
        InformationSource::Peer => Err(error(
            line,
            "countermeasure source must be official, signage, or staff",
        )),
        source => Ok(source),
    }
}

fn parse_claim(
    line: usize,
    kind: &str,
    subject: &str,
    state: &str,
) -> Result<crate::model::Proposition, ParseError> {
    let open = parse_availability_open(line, state)?;
    match kind {
        "connector" => Ok(crate::model::Proposition::ConnectorAvailability {
            connector: subject.to_owned(),
            open,
        }),
        "exit" => Ok(crate::model::Proposition::ExitAvailability {
            exit: subject.to_owned(),
            open,
        }),
        _ => Err(error(
            line,
            format!("unsupported claim kind `{kind}`; use `connector` or `exit`"),
        )),
    }
}

fn parse_availability_open(line: usize, state: &str) -> Result<bool, ParseError> {
    match state {
        "open" => Ok(true),
        "closed" => Ok(false),
        _ => Err(error(
            line,
            format!("unknown availability state `{state}`; use `open` or `closed`"),
        )),
    }
}

fn parse_connector_kind(line: usize, value: &str) -> Result<ConnectorKind, ParseError> {
    match value {
        "stair" => Ok(ConnectorKind::Stair),
        "ramp" => Ok(ConnectorKind::Ramp),
        "escalator" => Ok(ConnectorKind::Escalator),
        "lift" => Ok(ConnectorKind::Lift),
        _ => Err(error(
            line,
            format!("unknown connector kind `{value}`; use stair, ramp, escalator, or lift"),
        )),
    }
}

fn parse_plain<T>(line: usize, value: &str, label: &str) -> Result<T, ParseError>
where
    T: FromStr,
{
    value
        .parse()
        .map_err(|_| error(line, format!("invalid {label} `{value}`")))
}

fn required<'a>(
    line: usize,
    tokens: &'a [String],
    index: usize,
    label: &str,
) -> Result<&'a str, ParseError> {
    tokens
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| error(line, format!("missing {label}")))
}

fn expect(line: usize, tokens: &[String], index: usize, expected: &str) -> Result<(), ParseError> {
    let actual = required(line, tokens, index, expected)?;
    if actual == expected {
        Ok(())
    } else {
        Err(error(
            line,
            format!("expected `{expected}`, found `{actual}`"),
        ))
    }
}

fn require_count(line: usize, tokens: &[String], minimum: usize) -> Result<(), ParseError> {
    if tokens.len() < minimum {
        Err(error(
            line,
            format!("expected at least {minimum} tokens, found {}", tokens.len()),
        ))
    } else {
        Ok(())
    }
}

fn optional_sampling_key(
    line: usize,
    tokens: &[String],
    base_count: usize,
    declaration: &str,
) -> Result<Option<String>, ParseError> {
    match tokens.len() {
        count if count == base_count => Ok(None),
        count if count == base_count + 2 => {
            expect(line, tokens, base_count, "sample")?;
            Ok(Some(
                required(line, tokens, base_count + 1, "sampling key")?.to_owned(),
            ))
        }
        count => Err(error(
            line,
            format!(
                "expected {base_count} tokens or {base_count} followed by `sample KEY` for {declaration}, found {count}"
            ),
        )),
    }
}

fn require_exact_count(line: usize, tokens: &[String], expected: usize) -> Result<(), ParseError> {
    if tokens.len() == expected {
        Ok(())
    } else {
        Err(error(
            line,
            format!("expected {expected} tokens, found {}", tokens.len()),
        ))
    }
}

fn reject_duplicate(line: usize, duplicate: bool, name: &str) -> Result<(), ParseError> {
    if duplicate {
        Err(error(line, format!("duplicate `{name}` declaration")))
    } else {
        Ok(())
    }
}

fn error(line: usize, message: impl Into<String>) -> ParseError {
    ParseError {
        line,
        message: message.into(),
    }
}
