use crate::{
    bundle::{AgentSnapshot, AgentState, RunBundle, RunEvent, RunMetrics, TraceFrame},
    model::{CanonicalScenario, Point3, Scenario, Surface},
    validate,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fmt,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOptions {
    /// Write one trace frame after this many integration steps.
    pub trace_every_steps: u32,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            trace_every_steps: 10,
        }
    }
}

#[derive(Debug)]
pub enum RunError {
    InvalidScenario(Vec<crate::ValidationError>),
    NoRoute {
        agent_group: String,
        destination: String,
    },
    InvalidOptions(String),
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidScenario(errors) => {
                write!(formatter, "scenario validation failed: {errors:?}")
            }
            Self::NoRoute {
                agent_group,
                destination,
            } => write!(
                formatter,
                "agent group `{agent_group}` has no route to `{destination}`"
            ),
            Self::InvalidOptions(message) => write!(formatter, "invalid run options: {message}"),
        }
    }
}

impl std::error::Error for RunError {}

#[derive(Debug, Clone)]
enum Motion {
    OnSurface,
    Transit {
        connector_index: usize,
        elapsed_s: f64,
        duration_s: f64,
        start: Point3,
        end: Point3,
        next_surface: String,
    },
    Evacuated {
        at_s: f64,
    },
}

#[derive(Debug, Clone)]
struct Agent {
    id: String,
    group: String,
    surface: String,
    position: Point3,
    destination: String,
    speed_mps: f64,
    radius_m: f64,
    route: Vec<usize>,
    route_cursor: usize,
    motion: Motion,
    beliefs: BTreeMap<String, f64>,
    blocked_connectors: HashSet<String>,
    passed_gates: HashSet<String>,
}

/// Execute the reference small-step semantics deterministically.
pub fn run(scenario: Scenario, options: RunOptions) -> Result<RunBundle, RunError> {
    validate(&scenario).map_err(RunError::InvalidScenario)?;
    if options.trace_every_steps == 0 {
        return Err(RunError::InvalidOptions(
            "trace_every_steps must be greater than zero".to_owned(),
        ));
    }
    let canonical = CanonicalScenario::from(scenario.clone());
    let mut agents = spawn_agents(&scenario)?;
    let step_count = (scenario.duration_s / scenario.timestep_s).ceil() as u64;
    let mut trace = vec![snapshot(0, 0.0, &agents)];
    let mut events = Vec::new();
    let mut queued_for_lift_agents = HashSet::new();
    let mut queued_for_gate_agents = HashSet::new();
    let mut gate_tokens: HashMap<String, f64> = scenario
        .gates
        .iter()
        .map(|gate| (gate.id.clone(), 0.0))
        .collect();

    for step in 1..=step_count {
        let time_s = (step as f64) * scenario.timestep_s;
        apply_information(
            &scenario,
            &mut agents,
            time_s,
            scenario.timestep_s,
            &mut events,
        );
        integrate(
            &scenario,
            &mut agents,
            time_s,
            &mut events,
            &mut queued_for_lift_agents,
            &mut queued_for_gate_agents,
            &mut gate_tokens,
        );
        if step % u64::from(options.trace_every_steps) == 0 || step == step_count {
            trace.push(snapshot(step, time_s, &agents));
        }
    }

    let exit_times: Vec<f64> = agents
        .iter()
        .filter_map(|agent| match agent.motion {
            Motion::Evacuated { at_s } => Some(at_s),
            Motion::OnSurface | Motion::Transit { .. } => None,
        })
        .collect();
    let evacuated_agents = u32::try_from(exit_times.len()).expect("agent count fits u32");
    let clearance_time_s = exit_times.iter().copied().reduce(f64::max);
    let mean_exit_time_s =
        (!exit_times.is_empty()).then(|| exit_times.iter().sum::<f64>() / exit_times.len() as f64);
    let metrics = RunMetrics {
        total_agents: u32::try_from(agents.len()).expect("agent count fits u32"),
        evacuated_agents,
        clearance_time_s,
        mean_exit_time_s,
        queued_for_lift_agents: u32::try_from(queued_for_lift_agents.len())
            .expect("agent count fits u32"),
        queued_for_gate_agents: u32::try_from(queued_for_gate_agents.len())
            .expect("agent count fits u32"),
    };
    let options = BTreeMap::from([
        (
            "trace_every_steps".to_owned(),
            options.trace_every_steps.to_string(),
        ),
        (
            "integration".to_owned(),
            "deterministic-euler-0.1".to_owned(),
        ),
    ]);
    Ok(RunBundle::new(canonical, options, trace, events, metrics))
}

fn spawn_agents(scenario: &Scenario) -> Result<Vec<Agent>, RunError> {
    let mut agents = Vec::new();
    for group in &scenario.agents {
        let route =
            route_to_exit(scenario, &group.surface, &group.destination).ok_or_else(|| {
                RunError::NoRoute {
                    agent_group: group.id.clone(),
                    destination: group.destination.clone(),
                }
            })?;
        let columns = (f64::from(group.count).sqrt().ceil() as u32).max(1);
        let spacing = group.radius_m * 2.1;
        for ordinal in 0..group.count {
            let column = ordinal % columns;
            let row = ordinal / columns;
            agents.push(Agent {
                id: format!("{}:{ordinal}", group.id),
                group: group.id.clone(),
                surface: group.surface.clone(),
                position: Point3 {
                    x_m: group.at.x_m + f64::from(column) * spacing,
                    y_m: group.at.y_m + f64::from(row) * spacing,
                    z_m: group.at.z_m,
                },
                destination: group.destination.clone(),
                speed_mps: group.speed_mps,
                radius_m: group.radius_m,
                route: route.clone(),
                route_cursor: 0,
                motion: Motion::OnSurface,
                beliefs: BTreeMap::new(),
                blocked_connectors: HashSet::new(),
                passed_gates: HashSet::new(),
            });
        }
    }
    Ok(agents)
}

fn route_to_exit(scenario: &Scenario, start: &str, exit_id: &str) -> Option<Vec<usize>> {
    route_to_exit_avoiding(scenario, start, exit_id, &HashSet::new())
}

fn route_to_exit_avoiding(
    scenario: &Scenario,
    start: &str,
    exit_id: &str,
    blocked_connectors: &HashSet<String>,
) -> Option<Vec<usize>> {
    let exit = scenario.exits.iter().find(|exit| exit.id == exit_id)?;
    if start == exit.surface {
        return Some(Vec::new());
    }
    let mut queue = VecDeque::from([start.to_owned()]);
    let mut previous: HashMap<String, (String, usize)> = HashMap::new();
    let mut seen = HashSet::from([start.to_owned()]);
    while let Some(surface) = queue.pop_front() {
        for (index, connector) in scenario.connectors.iter().enumerate() {
            if blocked_connectors.contains(connector.id())
                || connector.from_surface() != surface
                || !seen.insert(connector.to_surface().to_owned())
            {
                continue;
            }
            previous.insert(connector.to_surface().to_owned(), (surface.clone(), index));
            if connector.to_surface() == exit.surface {
                let mut route = Vec::new();
                let mut cursor = exit.surface.clone();
                while cursor != start {
                    let (parent, connector_index) = previous.get(&cursor)?.clone();
                    route.push(connector_index);
                    cursor = parent;
                }
                route.reverse();
                return Some(route);
            }
            queue.push_back(connector.to_surface().to_owned());
        }
    }
    None
}

fn apply_information(
    scenario: &Scenario,
    agents: &mut [Agent],
    time_s: f64,
    timestep_s: f64,
    events: &mut Vec<RunEvent>,
) {
    let previous_time = time_s - timestep_s;
    for message in &scenario.messages {
        if !(previous_time < message.at_s && message.at_s <= time_s) {
            continue;
        }
        for agent in agents
            .iter_mut()
            .filter(|agent| agent.surface == message.surface)
        {
            if agent.position.distance(message.origin) <= message.reach_m {
                agent.beliefs.insert(message.id.clone(), message.trust);
                if message.trust >= 0.5 {
                    apply_claim(agent, &message.claim);
                    reroute_after_information(scenario, agent, message.at_s, events);
                }
                events.push(RunEvent {
                    time_s: message.at_s,
                    kind: "message_received".to_owned(),
                    subject: agent.id.clone(),
                    detail: format!(
                        "{}: connector `{}` {} from {} (truthful={})",
                        message.id,
                        message.claim.connector(),
                        if message.claim.is_open() {
                            "open"
                        } else {
                            "closed"
                        },
                        message.source.as_str(),
                        message.truthful
                    ),
                });
            }
        }
    }
    for countermeasure in &scenario.countermeasures {
        if !(previous_time < countermeasure.at_s && countermeasure.at_s <= time_s) {
            continue;
        }
        for agent in agents
            .iter_mut()
            .filter(|agent| agent.surface == countermeasure.surface)
        {
            if agent.position.distance(countermeasure.origin) <= countermeasure.reach_m {
                agent
                    .beliefs
                    .insert(countermeasure.corrects.clone(), countermeasure.trust);
                if countermeasure.trust >= 0.5 {
                    if let Some(message) = scenario
                        .messages
                        .iter()
                        .find(|message| message.id == countermeasure.corrects)
                    {
                        agent.blocked_connectors.remove(message.claim.connector());
                        reroute_after_information(scenario, agent, countermeasure.at_s, events);
                    }
                }
                events.push(RunEvent {
                    time_s: countermeasure.at_s,
                    kind: "countermeasure_received".to_owned(),
                    subject: agent.id.clone(),
                    detail: format!(
                        "{} corrected by {} via {}",
                        countermeasure.corrects,
                        countermeasure.id,
                        countermeasure.source.as_str()
                    ),
                });
            }
        }
    }
}

fn apply_claim(agent: &mut Agent, claim: &crate::model::Proposition) {
    if claim.is_open() {
        agent.blocked_connectors.remove(claim.connector());
    } else {
        agent
            .blocked_connectors
            .insert(claim.connector().to_owned());
    }
}

fn reroute_after_information(
    scenario: &Scenario,
    agent: &mut Agent,
    time_s: f64,
    events: &mut Vec<RunEvent>,
) {
    if !matches!(agent.motion, Motion::OnSurface) {
        return;
    }
    if let Some(route) = route_to_exit_avoiding(
        scenario,
        &agent.surface,
        &agent.destination,
        &agent.blocked_connectors,
    ) {
        agent.route = route;
        agent.route_cursor = 0;
        let mut blocked: Vec<_> = agent.blocked_connectors.iter().cloned().collect();
        blocked.sort_unstable();
        events.push(RunEvent {
            time_s,
            kind: "route_recomputed".to_owned(),
            subject: agent.id.clone(),
            detail: blocked.join(","),
        });
    } else {
        events.push(RunEvent {
            time_s,
            kind: "route_unavailable".to_owned(),
            subject: agent.id.clone(),
            detail: "belief-constrained route does not reach the destination".to_owned(),
        });
    }
}

fn integrate(
    scenario: &Scenario,
    agents: &mut [Agent],
    time_s: f64,
    events: &mut Vec<RunEvent>,
    queued_for_lift_agents: &mut HashSet<String>,
    queued_for_gate_agents: &mut HashSet<String>,
    gate_tokens: &mut HashMap<String, f64>,
) {
    let positions: Vec<(String, Point3, f64)> = agents
        .iter()
        .map(|agent| (agent.surface.clone(), agent.position, agent.radius_m))
        .collect();
    let mut lift_loads = current_lift_loads(agents);
    for gate in &scenario.gates {
        let token = gate_tokens.entry(gate.id.clone()).or_default();
        *token = (*token + gate.service_rate_per_s * scenario.timestep_s)
            .min(gate.service_rate_per_s.max(1.0));
    }
    for (index, agent) in agents.iter_mut().enumerate() {
        match &mut agent.motion {
            Motion::Evacuated { .. } => continue,
            Motion::Transit {
                connector_index,
                elapsed_s,
                duration_s,
                start,
                end,
                next_surface,
            } => {
                *elapsed_s += scenario.timestep_s;
                let ratio = (*elapsed_s / *duration_s).min(1.0);
                agent.position = start.lerp(*end, ratio);
                if ratio >= 1.0 {
                    let connector_id = scenario.connectors[*connector_index].id().to_owned();
                    agent.surface = next_surface.clone();
                    agent.route_cursor += 1;
                    agent.motion = Motion::OnSurface;
                    events.push(RunEvent {
                        time_s,
                        kind: "connector_arrival".to_owned(),
                        subject: agent.id.clone(),
                        detail: connector_id,
                    });
                }
            }
            Motion::OnSurface => {
                let (target, next_connector, final_gate) =
                    if let Some(&connector_index) = agent.route.get(agent.route_cursor) {
                        (
                            scenario.connectors[connector_index].from(),
                            Some(connector_index),
                            None,
                        )
                    } else {
                        let exit = scenario
                            .exits
                            .iter()
                            .find(|exit| exit.id == agent.destination)
                            .expect("validated destination exists");
                        let gate = scenario.gates.iter().find(|gate| {
                            gate.surface == agent.surface
                                && gate.destination == agent.destination
                                && !agent.passed_gates.contains(&gate.id)
                        });
                        match gate {
                            Some(gate) => (gate.at, None, Some(gate.id.clone())),
                            None => (exit.at, None, None),
                        }
                    };
                let surface = scenario
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == agent.surface)
                    .expect("validated surface exists");
                let reached = move_toward(
                    agent,
                    target,
                    scenario.timestep_s,
                    &positions,
                    index,
                    surface,
                );
                if !reached {
                    continue;
                }
                match next_connector {
                    None => {
                        if let Some(gate_id) = final_gate {
                            let tokens = gate_tokens.entry(gate_id.clone()).or_default();
                            if *tokens >= 1.0 {
                                *tokens -= 1.0;
                                agent.passed_gates.insert(gate_id.clone());
                                events.push(RunEvent {
                                    time_s,
                                    kind: "gate_processed".to_owned(),
                                    subject: agent.id.clone(),
                                    detail: gate_id,
                                });
                            } else {
                                queued_for_gate_agents.insert(agent.id.clone());
                            }
                        } else {
                            agent.motion = Motion::Evacuated { at_s: time_s };
                            events.push(RunEvent {
                                time_s,
                                kind: "evacuated".to_owned(),
                                subject: agent.id.clone(),
                                detail: agent.destination.clone(),
                            });
                        }
                    }
                    Some(connector_index) => {
                        let connector = &scenario.connectors[connector_index];
                        if connector.is_lift()
                            && lift_loads
                                .get(&connector_index)
                                .copied()
                                .unwrap_or_default()
                                >= connector.capacity().expect("lift has capacity")
                        {
                            queued_for_lift_agents.insert(agent.id.clone());
                            continue;
                        }
                        let duration_s = connector.traversal_duration_s(agent.speed_mps);
                        if connector.is_lift() {
                            *lift_loads.entry(connector_index).or_insert(0) += 1;
                        }
                        agent.motion = Motion::Transit {
                            connector_index,
                            elapsed_s: 0.0,
                            duration_s,
                            start: connector.from(),
                            end: connector.to(),
                            next_surface: connector.to_surface().to_owned(),
                        };
                        events.push(RunEvent {
                            time_s,
                            kind: "connector_boarding".to_owned(),
                            subject: agent.id.clone(),
                            detail: connector.id().to_owned(),
                        });
                    }
                }
            }
        }
    }
}

fn current_lift_loads(agents: &[Agent]) -> HashMap<usize, u32> {
    let mut loads = HashMap::new();
    for agent in agents {
        if let Motion::Transit {
            connector_index, ..
        } = &agent.motion
        {
            *loads.entry(*connector_index).or_insert(0) += 1;
        }
    }
    loads
}

fn move_toward(
    agent: &mut Agent,
    target: Point3,
    timestep_s: f64,
    positions: &[(String, Point3, f64)],
    own_index: usize,
    surface: &Surface,
) -> bool {
    let dx = target.x_m - agent.position.x_m;
    let dy = target.y_m - agent.position.y_m;
    let dz = target.z_m - agent.position.z_m;
    let distance = (dx.mul_add(dx, dy.mul_add(dy, dz * dz))).sqrt();
    let travel = agent.speed_mps * timestep_s;
    if distance <= travel {
        agent.position = target;
        return true;
    }
    let mut next = Point3 {
        x_m: agent.position.x_m + (dx / distance) * travel,
        y_m: agent.position.y_m + (dy / distance) * travel,
        z_m: agent.position.z_m + (dz / distance) * travel,
    };
    for (index, (surface, position, radius)) in positions.iter().enumerate() {
        if index == own_index || surface != &agent.surface {
            continue;
        }
        let separation = next.distance(*position);
        let minimum = agent.radius_m + radius;
        if separation > 0.0 && separation < minimum {
            let correction = (minimum - separation) / minimum * agent.radius_m;
            next.x_m += (next.x_m - position.x_m) / separation * correction;
            next.y_m += (next.y_m - position.y_m) / separation * correction;
        }
    }
    next.x_m = next
        .x_m
        .clamp(surface.origin.x_m, surface.origin.x_m + surface.width_m);
    next.y_m = next
        .y_m
        .clamp(surface.origin.y_m, surface.origin.y_m + surface.depth_m);
    next.z_m = surface.origin.z_m;
    agent.position = next;
    false
}

fn snapshot(step: u64, time_s: f64, agents: &[Agent]) -> TraceFrame {
    TraceFrame {
        step,
        time_s,
        agents: agents
            .iter()
            .map(|agent| AgentSnapshot {
                id: agent.id.clone(),
                group: agent.group.clone(),
                surface: agent.surface.clone(),
                x_m: agent.position.x_m,
                y_m: agent.position.y_m,
                z_m: agent.position.z_m,
                state: match &agent.motion {
                    Motion::OnSurface => AgentState::Moving,
                    Motion::Transit { .. } => AgentState::InTransit,
                    Motion::Evacuated { .. } => AgentState::Evacuated,
                },
                beliefs: agent.beliefs.clone(),
            })
            .collect(),
    }
}
