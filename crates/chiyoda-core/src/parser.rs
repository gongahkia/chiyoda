use crate::model::{
    AgentGroup, Connector, Countermeasure, Exit, Gate, InformationSource, Message, Point3,
    Scenario, Surface,
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
    exits: Vec<Exit>,
    connectors: Vec<Connector>,
    gates: Vec<Gate>,
    agents: Vec<AgentGroup>,
    messages: Vec<Message>,
    countermeasures: Vec<Countermeasure>,
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
            exits: self.exits,
            connectors: self.connectors,
            gates: self.gates,
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

fn parse_declaration(
    line: usize,
    tokens: &[String],
    builder: &mut ScenarioBuilder,
) -> Result<(), ParseError> {
    let keyword = required(line, tokens, 0, "declaration keyword")?;
    match keyword {
        "scenario" => {
            reject_duplicate(line, builder.name.is_some(), "scenario")?;
            require_count(line, tokens, 2)?;
            builder.name = Some(tokens[1].clone());
        }
        "seed" => {
            reject_duplicate(line, builder.seed.is_some(), "seed")?;
            require_count(line, tokens, 2)?;
            builder.seed = Some(parse_plain(line, &tokens[1], "seed")?);
        }
        "duration" => {
            reject_duplicate(line, builder.duration_s.is_some(), "duration")?;
            require_count(line, tokens, 2)?;
            builder.duration_s = Some(parse_duration(line, &tokens[1])?);
        }
        "timestep" => {
            reject_duplicate(line, builder.timestep_s.is_some(), "timestep")?;
            require_count(line, tokens, 2)?;
            builder.timestep_s = Some(parse_duration(line, &tokens[1])?);
        }
        "surface" => {
            require_count(line, tokens, 9)?;
            expect(line, tokens, 2, "at")?;
            expect(line, tokens, 6, "size")?;
            builder.surfaces.push(Surface {
                id: tokens[1].clone(),
                origin: point(line, tokens, 3)?,
                width_m: parse_length(line, required(line, tokens, 7, "surface width")?)?,
                depth_m: parse_length(line, required(line, tokens, 8, "surface depth")?)?,
            });
        }
        "exit" => {
            require_count(line, tokens, 10)?;
            expect(line, tokens, 2, "on")?;
            expect(line, tokens, 4, "at")?;
            expect(line, tokens, 8, "width")?;
            builder.exits.push(Exit {
                id: tokens[1].clone(),
                surface: tokens[3].clone(),
                at: point(line, tokens, 5)?,
                width_m: parse_length(line, required(line, tokens, 9, "exit width")?)?,
            });
        }
        "stair" => {
            require_count(line, tokens, 17)?;
            expect(line, tokens, 2, "from")?;
            expect(line, tokens, 4, "at")?;
            expect(line, tokens, 8, "to")?;
            expect(line, tokens, 10, "at")?;
            expect(line, tokens, 14, "width")?;
            builder.connectors.push(Connector::Stair {
                id: tokens[1].clone(),
                from_surface: tokens[3].clone(),
                from: point(line, tokens, 5)?,
                to_surface: tokens[9].clone(),
                to: point(line, tokens, 11)?,
                width_m: parse_length(line, required(line, tokens, 15, "stair width")?)?,
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
                capacity: parse_plain(line, required(line, tokens, 18, "lift capacity")?)?,
                cycle_s: parse_duration(line, required(line, tokens, 20, "lift cycle")?)?,
            });
        }
        "gate" => {
            require_count(line, tokens, 12)?;
            expect(line, tokens, 2, "on")?;
            expect(line, tokens, 4, "at")?;
            expect(line, tokens, 8, "width")?;
            expect(line, tokens, 10, "capacity")?;
            builder.gates.push(Gate {
                id: tokens[1].clone(),
                surface: tokens[3].clone(),
                at: point(line, tokens, 5)?,
                width_m: parse_length(line, required(line, tokens, 9, "gate width")?)?,
                service_rate_per_s: parse_rate(line, required(line, tokens, 11, "gate capacity")?)?,
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
            builder.agents.push(AgentGroup {
                id: tokens[1].clone(),
                count: parse_plain(line, required(line, tokens, 3, "agent count")?)?,
                surface: tokens[5].clone(),
                at: point(line, tokens, 7)?,
                destination: tokens[11].clone(),
                speed_mps: parse_speed(line, required(line, tokens, 13, "agent speed")?)?,
                radius_m: parse_length(line, required(line, tokens, 15, "agent radius")?)?,
                height_m: parse_length(line, required(line, tokens, 17, "agent height")?)?,
            });
        }
        "message" => {
            require_count(line, tokens, 20)?;
            expect(line, tokens, 2, "source")?;
            expect(line, tokens, 4, "on")?;
            expect(line, tokens, 6, "at")?;
            expect(line, tokens, 10, "claim")?;
            expect(line, tokens, 12, "truth")?;
            expect(line, tokens, 14, "time")?;
            expect(line, tokens, 16, "reach")?;
            expect(line, tokens, 18, "trust")?;
            builder.messages.push(Message {
                id: tokens[1].clone(),
                source: parse_source(line, required(line, tokens, 3, "message source")?)?,
                surface: tokens[5].clone(),
                origin: point(line, tokens, 7)?,
                claim: tokens[11].clone(),
                truthful: parse_plain(line, required(line, tokens, 13, "message truth value")?)?,
                at_s: parse_duration(line, required(line, tokens, 15, "message time")?)?,
                reach_m: parse_length(line, required(line, tokens, 17, "message reach")?)?,
                trust: parse_plain(line, required(line, tokens, 19, "message trust")?)?,
            });
        }
        "countermeasure" => {
            require_count(line, tokens, 18)?;
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
                source: parse_source(line, required(line, tokens, 5, "countermeasure source")?)?,
                surface: tokens[7].clone(),
                origin: point(line, tokens, 9)?,
                at_s: parse_duration(line, required(line, tokens, 13, "countermeasure time")?)?,
                reach_m: parse_length(line, required(line, tokens, 15, "countermeasure reach")?)?,
                trust: parse_plain(line, required(line, tokens, 17, "countermeasure trust")?)?,
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
