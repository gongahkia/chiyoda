use crate::{
    bundle::{
        AgentSnapshot, AgentState, InformationDeliveryMetrics, InformationInterventionKind,
        RunBundle, RunEvent, RunMetrics, TraceFrame,
    },
    model::{
        CanonicalScenario, ConnectorKind, NAVIGATION_CLEARANCE_EPSILON_M, Obstacle, Point3,
        Scenario, Surface,
    },
    validate,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
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
    WaitingToDepart {
        release_at_s: f64,
    },
    WaitingAtWaypoint {
        until_s: f64,
        waypoint_id: String,
    },
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
    /// The final exit selected by the most recent route computation.
    destination: String,
    /// All declared final exits, in source order, for later rerouting.
    exit_candidates: Vec<String>,
    /// The gate selected with the current final-exit plan, if that exit has one.
    final_gate: Option<String>,
    via: Vec<String>,
    via_cursor: usize,
    speed_mps: f64,
    radius_m: f64,
    height_m: f64,
    excluded_connector_kinds: Vec<ConnectorKind>,
    route: Vec<usize>,
    route_cursor: usize,
    motion: Motion,
    beliefs: BTreeMap<String, f64>,
    blocked_connectors: HashSet<String>,
    blocked_exits: HashSet<String>,
    passed_gates: HashSet<String>,
    waiting_connector: Option<ConnectorWait>,
    waiting_for_route: bool,
    waiting_for_exit: bool,
}

#[derive(Debug, Clone, Copy)]
enum ConnectorWait {
    Lift,
    Capacity,
}

#[derive(Debug, Clone)]
struct RouteTarget {
    id: String,
    surface: String,
    at: Point3,
    is_exit: bool,
    dwell_s: f64,
}

#[derive(Debug, Clone)]
struct RoutePlan {
    connector_indices: Vec<usize>,
    nominal_duration_s: f64,
}

#[derive(Debug, Clone)]
struct PlannedTarget {
    target: RouteTarget,
    plan: RoutePlan,
    final_gate: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct RouteStart<'a> {
    surface: &'a str,
    position: Point3,
    walking_speed_mps: f64,
    radius_m: f64,
    height_m: f64,
    excluded_connector_kinds: &'a [ConnectorKind],
}

/// Final-exit constraints that share one route-planning decision.
#[derive(Debug, Clone, Copy)]
struct ExitPlanConstraints<'a> {
    blocked_exits: &'a HashSet<String>,
    closed_gates: &'a HashSet<String>,
    passed_gates: Option<&'a HashSet<String>>,
}

impl RouteStart<'_> {
    fn allows_connector(self, connector: &crate::model::Connector) -> bool {
        connector.supports_height(self.height_m)
            && !self.excluded_connector_kinds.contains(&connector.kind())
    }
}

const SPATIAL_CELL_M: f64 = 1.0;
const NAVIGATION_EPSILON_M: f64 = NAVIGATION_CLEARANCE_EPSILON_M;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CellKey {
    surface: String,
    x: i64,
    y: i64,
}

#[derive(Debug)]
struct SpatialSnapshot {
    positions: Vec<Option<(String, Point3, f64)>>,
    cells: HashMap<CellKey, Vec<usize>>,
    max_radius_m: f64,
}

#[derive(Debug)]
struct RuntimeResources {
    queued_for_lift_agents: HashSet<String>,
    queued_for_connector_agents: HashSet<String>,
    queued_for_gate_agents: HashSet<String>,
    queued_for_exit_agents: HashSet<String>,
    closed_connectors: HashSet<String>,
    closed_exits: HashSet<String>,
    closed_gates: HashSet<String>,
    connector_tokens: HashMap<usize, f64>,
    gate_tokens: HashMap<String, f64>,
    exit_tokens: HashMap<String, f64>,
}

impl RuntimeResources {
    fn for_scenario(scenario: &Scenario) -> Self {
        Self {
            queued_for_lift_agents: HashSet::new(),
            queued_for_connector_agents: HashSet::new(),
            queued_for_gate_agents: HashSet::new(),
            queued_for_exit_agents: HashSet::new(),
            closed_connectors: closed_connectors_at(scenario, 0.0),
            closed_exits: closed_exits_at(scenario, 0.0),
            closed_gates: closed_gates_at(scenario, 0.0),
            connector_tokens: scenario
                .connectors
                .iter()
                .enumerate()
                .filter_map(|(index, connector)| {
                    connector.service_rate_per_s().map(|_| (index, 0.0))
                })
                .collect(),
            gate_tokens: scenario
                .gates
                .iter()
                .map(|gate| (gate.id.clone(), 0.0))
                .collect(),
            exit_tokens: scenario
                .exits
                .iter()
                .filter_map(|exit| exit.capacity_per_s.map(|_| (exit.id.clone(), 0.0)))
                .collect(),
        }
    }
}

fn closed_connectors_at(scenario: &Scenario, time_s: f64) -> HashSet<String> {
    scenario
        .connectors
        .iter()
        .filter(|connector| !scenario.connector_open_at(connector.id(), time_s))
        .map(|connector| connector.id().to_owned())
        .collect()
}

fn closed_exits_at(scenario: &Scenario, time_s: f64) -> HashSet<String> {
    scenario
        .exits
        .iter()
        .filter(|exit| !scenario.exit_open_at(&exit.id, time_s))
        .map(|exit| exit.id.clone())
        .collect()
}

fn closed_gates_at(scenario: &Scenario, time_s: f64) -> HashSet<String> {
    scenario
        .gates
        .iter()
        .filter(|gate| !scenario.gate_open_at(&gate.id, time_s))
        .map(|gate| gate.id.clone())
        .collect()
}

fn initial_state_events(scenario: &Scenario) -> Vec<RunEvent> {
    let mut events: Vec<_> = scenario
        .connector_states
        .iter()
        .filter(|change| change.at_s == 0.0)
        .map(connector_state_event)
        .collect();
    events.extend(
        scenario
            .exit_states
            .iter()
            .filter(|change| change.at_s == 0.0)
            .map(exit_state_event),
    );
    events.extend(
        scenario
            .gate_states
            .iter()
            .filter(|change| change.at_s == 0.0)
            .map(gate_state_event),
    );
    events.extend(
        scenario
            .connector_capacity_states
            .iter()
            .filter(|change| change.at_s == 0.0)
            .map(connector_capacity_event),
    );
    events.extend(
        scenario
            .exit_capacity_states
            .iter()
            .filter(|change| change.at_s == 0.0)
            .map(exit_capacity_event),
    );
    events.extend(
        scenario
            .gate_capacity_states
            .iter()
            .filter(|change| change.at_s == 0.0)
            .map(gate_capacity_event),
    );
    events
}

fn information_delivery_for_scenario(
    scenario: &Scenario,
) -> BTreeMap<String, InformationDeliveryMetrics> {
    let mut delivery = BTreeMap::new();
    for message in &scenario.messages {
        delivery.insert(
            message.id.clone(),
            InformationDeliveryMetrics {
                kind: InformationInterventionKind::Message,
                received_agents: 0,
                accepted_agents: 0,
            },
        );
    }
    for countermeasure in &scenario.countermeasures {
        delivery.insert(
            countermeasure.id.clone(),
            InformationDeliveryMetrics {
                kind: InformationInterventionKind::Countermeasure,
                received_agents: 0,
                accepted_agents: 0,
            },
        );
    }
    delivery
}

enum ScheduledEvent<'a> {
    ConnectorState(&'a crate::model::ConnectorStateChange),
    ExitState(&'a crate::model::ExitStateChange),
    ConnectorCapacity(&'a crate::model::ConnectorCapacityChange),
    ExitCapacity(&'a crate::model::ExitCapacityChange),
    GateCapacity(&'a crate::model::GateCapacityChange),
    GateState(&'a crate::model::GateStateChange),
    Message(&'a crate::model::Message),
    Countermeasure(&'a crate::model::Countermeasure),
}

#[allow(clippy::too_many_lines)] // state events intentionally share one chronological dispatcher
fn apply_scheduled_events(
    scenario: &Scenario,
    agents: &mut [Agent],
    time_s: f64,
    events: &mut Vec<RunEvent>,
    resources: &mut RuntimeResources,
    information_delivery: &mut BTreeMap<String, InformationDeliveryMetrics>,
) {
    let previous_time = time_s - scenario.timestep_s;
    let is_scheduled = |at_s: f64| previous_time < at_s && at_s <= time_s;
    let mut scheduled = Vec::new();
    scheduled.extend(
        scenario
            .connector_states
            .iter()
            .enumerate()
            .filter(|(_, change)| 0.0 < change.at_s && is_scheduled(change.at_s))
            .map(|(index, change)| {
                (
                    change.at_s,
                    0_u8,
                    index,
                    ScheduledEvent::ConnectorState(change),
                )
            }),
    );
    scheduled.extend(
        scenario
            .exit_states
            .iter()
            .enumerate()
            .filter(|(_, change)| is_scheduled(change.at_s))
            .map(|(index, change)| (change.at_s, 1_u8, index, ScheduledEvent::ExitState(change))),
    );
    scheduled.extend(
        scenario
            .connector_capacity_states
            .iter()
            .enumerate()
            .filter(|(_, change)| 0.0 < change.at_s && is_scheduled(change.at_s))
            .map(|(index, change)| {
                (
                    change.at_s,
                    3_u8,
                    index,
                    ScheduledEvent::ConnectorCapacity(change),
                )
            }),
    );
    scheduled.extend(
        scenario
            .exit_capacity_states
            .iter()
            .enumerate()
            .filter(|(_, change)| 0.0 < change.at_s && is_scheduled(change.at_s))
            .map(|(index, change)| {
                (
                    change.at_s,
                    4_u8,
                    index,
                    ScheduledEvent::ExitCapacity(change),
                )
            }),
    );
    scheduled.extend(
        scenario
            .gate_capacity_states
            .iter()
            .enumerate()
            .filter(|(_, change)| 0.0 < change.at_s && is_scheduled(change.at_s))
            .map(|(index, change)| {
                (
                    change.at_s,
                    5_u8,
                    index,
                    ScheduledEvent::GateCapacity(change),
                )
            }),
    );
    scheduled.extend(
        scenario
            .gate_states
            .iter()
            .enumerate()
            .filter(|(_, change)| 0.0 < change.at_s && is_scheduled(change.at_s))
            .map(|(index, change)| (change.at_s, 2_u8, index, ScheduledEvent::GateState(change))),
    );
    scheduled.extend(
        scenario
            .messages
            .iter()
            .enumerate()
            .filter(|(_, message)| is_scheduled(message.at_s))
            .map(|(index, message)| (message.at_s, 6_u8, index, ScheduledEvent::Message(message))),
    );
    scheduled.extend(
        scenario
            .countermeasures
            .iter()
            .enumerate()
            .filter(|(_, countermeasure)| is_scheduled(countermeasure.at_s))
            .map(|(index, countermeasure)| {
                (
                    countermeasure.at_s,
                    7_u8,
                    index,
                    ScheduledEvent::Countermeasure(countermeasure),
                )
            }),
    );
    scheduled.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });
    for (_, _, _, event) in scheduled {
        match event {
            ScheduledEvent::ConnectorState(change) => {
                apply_connector_state_change(scenario, agents, change, events, resources);
            }
            ScheduledEvent::ExitState(change) => {
                apply_exit_state_change(scenario, agents, change, events, resources);
            }
            ScheduledEvent::ConnectorCapacity(change) => {
                events.push(connector_capacity_event(change));
            }
            ScheduledEvent::ExitCapacity(change) => {
                events.push(exit_capacity_event(change));
            }
            ScheduledEvent::GateCapacity(change) => {
                events.push(gate_capacity_event(change));
            }
            ScheduledEvent::GateState(change) => {
                apply_gate_state_change(scenario, agents, change, events, resources);
            }
            ScheduledEvent::Message(message) => {
                deliver_message(
                    scenario,
                    agents,
                    message,
                    events,
                    resources,
                    information_delivery,
                );
            }
            ScheduledEvent::Countermeasure(countermeasure) => {
                deliver_countermeasure(
                    scenario,
                    agents,
                    countermeasure,
                    events,
                    resources,
                    information_delivery,
                );
            }
        }
    }
}

fn apply_connector_state_change(
    scenario: &Scenario,
    agents: &mut [Agent],
    change: &crate::model::ConnectorStateChange,
    events: &mut Vec<RunEvent>,
    resources: &mut RuntimeResources,
) {
    if change.open {
        resources.closed_connectors.remove(&change.connector);
    } else {
        resources.closed_connectors.insert(change.connector.clone());
    }
    events.push(connector_state_event(change));
    for agent in agents.iter_mut() {
        reroute(
            scenario,
            agent,
            change.at_s,
            events,
            &resources.closed_connectors,
            &resources.closed_exits,
            &resources.closed_gates,
        );
    }
}

fn connector_state_event(change: &crate::model::ConnectorStateChange) -> RunEvent {
    RunEvent {
        time_s: change.at_s,
        kind: "connector_state_changed".to_owned(),
        subject: change.connector.clone(),
        detail: format!(
            "{}: {}",
            change.id,
            if change.open { "open" } else { "closed" }
        ),
    }
}

fn apply_exit_state_change(
    scenario: &Scenario,
    agents: &mut [Agent],
    change: &crate::model::ExitStateChange,
    events: &mut Vec<RunEvent>,
    resources: &mut RuntimeResources,
) {
    if change.open {
        resources.closed_exits.remove(&change.exit);
    } else {
        resources.closed_exits.insert(change.exit.clone());
    }
    events.push(exit_state_event(change));
    for agent in agents.iter_mut() {
        reroute(
            scenario,
            agent,
            change.at_s,
            events,
            &resources.closed_connectors,
            &resources.closed_exits,
            &resources.closed_gates,
        );
    }
}

fn exit_state_event(change: &crate::model::ExitStateChange) -> RunEvent {
    RunEvent {
        time_s: change.at_s,
        kind: "exit_state_changed".to_owned(),
        subject: change.exit.clone(),
        detail: format!(
            "{}: {}",
            change.id,
            if change.open { "open" } else { "closed" }
        ),
    }
}

fn connector_capacity_event(change: &crate::model::ConnectorCapacityChange) -> RunEvent {
    RunEvent {
        time_s: change.at_s,
        kind: "connector_capacity_changed".to_owned(),
        subject: change.connector.clone(),
        detail: format!("{}: {}/s", change.id, change.capacity_per_s),
    }
}

fn exit_capacity_event(change: &crate::model::ExitCapacityChange) -> RunEvent {
    RunEvent {
        time_s: change.at_s,
        kind: "exit_capacity_changed".to_owned(),
        subject: change.exit.clone(),
        detail: format!("{}: {}/s", change.id, change.capacity_per_s),
    }
}

fn gate_capacity_event(change: &crate::model::GateCapacityChange) -> RunEvent {
    RunEvent {
        time_s: change.at_s,
        kind: "gate_capacity_changed".to_owned(),
        subject: change.gate.clone(),
        detail: format!("{}: {}/s", change.id, change.capacity_per_s),
    }
}

fn apply_gate_state_change(
    scenario: &Scenario,
    agents: &mut [Agent],
    change: &crate::model::GateStateChange,
    events: &mut Vec<RunEvent>,
    resources: &mut RuntimeResources,
) {
    if change.open {
        resources.closed_gates.remove(&change.gate);
    } else {
        resources.closed_gates.insert(change.gate.clone());
    }
    events.push(gate_state_event(change));
    for agent in agents.iter_mut() {
        reroute(
            scenario,
            agent,
            change.at_s,
            events,
            &resources.closed_connectors,
            &resources.closed_exits,
            &resources.closed_gates,
        );
    }
}

fn gate_state_event(change: &crate::model::GateStateChange) -> RunEvent {
    RunEvent {
        time_s: change.at_s,
        kind: "gate_state_changed".to_owned(),
        subject: change.gate.clone(),
        detail: format!(
            "{}: {}",
            change.id,
            if change.open { "open" } else { "closed" }
        ),
    }
}

impl SpatialSnapshot {
    fn from_agents(agents: &[Agent]) -> Self {
        let positions: Vec<_> = agents
            .iter()
            .map(|agent| {
                matches!(agent.motion, Motion::OnSurface)
                    .then(|| (agent.surface.clone(), agent.position, agent.radius_m))
            })
            .collect();
        let max_radius_m = positions
            .iter()
            .filter_map(|position| position.as_ref().map(|(_, _, radius)| *radius))
            .fold(0.0_f64, f64::max);
        let mut cells: HashMap<CellKey, Vec<usize>> = HashMap::new();
        for (index, position) in positions.iter().enumerate() {
            let Some((surface, point, _)) = position else {
                continue;
            };
            cells
                .entry(cell_key(surface, *point))
                .or_default()
                .push(index);
        }
        Self {
            positions,
            cells,
            max_radius_m,
        }
    }
}

/// Execute the reference small-step semantics deterministically.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // validation requires finite positive durations/counts; conversions bound discrete reference steps
pub fn run(scenario: &Scenario, options: RunOptions) -> Result<RunBundle, RunError> {
    validate(scenario).map_err(RunError::InvalidScenario)?;
    if options.trace_every_steps == 0 {
        return Err(RunError::InvalidOptions(
            "trace_every_steps must be greater than zero".to_owned(),
        ));
    }
    let canonical = CanonicalScenario::from(scenario.clone());
    let mut resources = RuntimeResources::for_scenario(scenario);
    let mut agents = spawn_agents(
        scenario,
        &resources.closed_connectors,
        &resources.closed_exits,
        &resources.closed_gates,
    )?;
    let step_count = (scenario.duration_s / scenario.timestep_s).ceil() as u64;
    let mut trace = vec![snapshot(0, 0.0, &agents)];
    let mut events = initial_state_events(scenario);
    let mut information_delivery = information_delivery_for_scenario(scenario);

    for step in 1..=step_count {
        let time_s = (step as f64) * scenario.timestep_s;
        apply_scheduled_events(
            scenario,
            &mut agents,
            time_s,
            &mut events,
            &mut resources,
            &mut information_delivery,
        );
        integrate(scenario, &mut agents, time_s, &mut events, &mut resources);
        if step % u64::from(options.trace_every_steps) == 0 || step == step_count {
            trace.push(snapshot(step, time_s, &agents));
        }
    }

    let exit_times: Vec<f64> = agents
        .iter()
        .filter_map(|agent| match agent.motion {
            Motion::Evacuated { at_s } => Some(at_s),
            Motion::WaitingToDepart { .. }
            | Motion::WaitingAtWaypoint { .. }
            | Motion::OnSurface
            | Motion::Transit { .. } => None,
        })
        .collect();
    let evacuated_agents = u32::try_from(exit_times.len()).expect("agent count fits u32");
    let mut evacuated_by_exit = BTreeMap::new();
    let mut remaining_by_state = BTreeMap::new();
    for agent in &agents {
        if matches!(agent.motion, Motion::Evacuated { .. }) {
            *evacuated_by_exit
                .entry(agent.destination.clone())
                .or_default() += 1;
        } else {
            *remaining_by_state
                .entry(agent_state_name(agent).to_owned())
                .or_default() += 1;
        }
    }
    let total_agents = u32::try_from(agents.len()).expect("agent count fits u32");
    let last_exit_time_s = exit_times.iter().copied().reduce(f64::max);
    let clearance_time_s = (evacuated_agents == total_agents)
        .then_some(last_exit_time_s)
        .flatten();
    let mean_exit_time_s =
        (!exit_times.is_empty()).then(|| exit_times.iter().sum::<f64>() / exit_times.len() as f64);
    let metrics = RunMetrics {
        total_agents,
        evacuated_agents,
        evacuated_by_exit,
        remaining_by_state,
        information_delivery,
        clearance_time_s,
        last_exit_time_s,
        mean_exit_time_s,
        queued_for_lift_agents: u32::try_from(resources.queued_for_lift_agents.len())
            .expect("agent count fits u32"),
        queued_for_connector_agents: u32::try_from(resources.queued_for_connector_agents.len())
            .expect("agent count fits u32"),
        queued_for_gate_agents: u32::try_from(resources.queued_for_gate_agents.len())
            .expect("agent count fits u32"),
        queued_for_exit_agents: u32::try_from(resources.queued_for_exit_agents.len())
            .expect("agent count fits u32"),
    };
    let options = BTreeMap::from([
        (
            "trace_every_steps".to_owned(),
            options.trace_every_steps.to_string(),
        ),
        (
            "integration".to_owned(),
            "deterministic-euler-0.21".to_owned(),
        ),
    ]);
    Ok(RunBundle::new(canonical, options, trace, events, metrics))
}

fn spawn_agents(
    scenario: &Scenario,
    closed_connectors: &HashSet<String>,
    closed_exits: &HashSet<String>,
    closed_gates: &HashSet<String>,
) -> Result<Vec<Agent>, RunError> {
    let mut agents = Vec::new();
    for group in &scenario.agents {
        for (ordinal, position) in group.spawn_positions().enumerate() {
            let release_at_s =
                group.release_time_for(u32::try_from(ordinal).expect("agent ordinal fits u32"));
            let route_start = RouteStart {
                surface: &group.surface,
                position,
                walking_speed_mps: group.speed_mps,
                radius_m: group.radius_m,
                height_m: group.height_m,
                excluded_connector_kinds: &group.excluded_connector_kinds,
            };
            let planned = group_plan(
                scenario,
                route_start,
                group,
                closed_connectors,
                closed_exits,
                closed_gates,
            );
            let waiting_for_route = planned.is_none();
            let (route, destination, final_gate) = match planned {
                Some(planned) => {
                    let destination = if planned.target.is_exit {
                        planned.target.id
                    } else {
                        group.destination.clone()
                    };
                    (
                        planned.plan.connector_indices,
                        destination,
                        planned.final_gate,
                    )
                }
                None if group_plan_becomes_available(scenario, route_start, group) => {
                    (Vec::new(), group.destination.clone(), None)
                }
                None => {
                    return Err(RunError::NoRoute {
                        agent_group: group.id.clone(),
                        destination: group.destination.clone(),
                    });
                }
            };
            agents.push(Agent {
                id: format!("{}:{ordinal}", group.id),
                group: group.id.clone(),
                surface: group.surface.clone(),
                position,
                destination,
                exit_candidates: group.exit_candidates().map(str::to_owned).collect(),
                final_gate,
                via: group.via.clone(),
                via_cursor: 0,
                speed_mps: group.speed_mps,
                radius_m: group.radius_m,
                height_m: group.height_m,
                excluded_connector_kinds: group.excluded_connector_kinds.clone(),
                route,
                route_cursor: 0,
                motion: if release_at_s == 0.0 {
                    Motion::OnSurface
                } else {
                    Motion::WaitingToDepart { release_at_s }
                },
                beliefs: BTreeMap::new(),
                blocked_connectors: HashSet::new(),
                blocked_exits: HashSet::new(),
                passed_gates: HashSet::new(),
                waiting_connector: None,
                waiting_for_route,
                waiting_for_exit: false,
            });
        }
    }
    Ok(agents)
}

fn group_plan_becomes_available(
    scenario: &Scenario,
    start: RouteStart<'_>,
    group: &crate::model::AgentGroup,
) -> bool {
    scenario
        .connector_states
        .iter()
        .map(|change| change.at_s)
        .chain(scenario.exit_states.iter().map(|change| change.at_s))
        .chain(scenario.gate_states.iter().map(|change| change.at_s))
        .filter(|&at_s| at_s > 0.0)
        .any(|at_s| {
            let closed_connectors = closed_connectors_at(scenario, at_s);
            let closed_exits = closed_exits_at(scenario, at_s);
            let closed_gates = closed_gates_at(scenario, at_s);
            group_plan(
                scenario,
                start,
                group,
                &closed_connectors,
                &closed_exits,
                &closed_gates,
            )
            .is_some()
        })
}

fn group_plan(
    scenario: &Scenario,
    start: RouteStart<'_>,
    group: &crate::model::AgentGroup,
    blocked_connectors: &HashSet<String>,
    closed_exits: &HashSet<String>,
    closed_gates: &HashSet<String>,
) -> Option<PlannedTarget> {
    let exit_constraints = ExitPlanConstraints {
        blocked_exits: closed_exits,
        closed_gates,
        passed_gates: None,
    };
    plan_for_stage(
        scenario,
        start,
        group.via.first().map(String::as_str),
        group.exit_candidates(),
        blocked_connectors,
        exit_constraints,
    )
}

fn agent_plan(
    scenario: &Scenario,
    start: RouteStart<'_>,
    agent: &Agent,
    blocked_connectors: &HashSet<String>,
    closed_exits: &HashSet<String>,
    closed_gates: &HashSet<String>,
) -> Option<PlannedTarget> {
    let blocked_exits = effective_blocked_exits(agent, closed_exits);
    let exit_constraints = ExitPlanConstraints {
        blocked_exits: &blocked_exits,
        closed_gates,
        passed_gates: Some(&agent.passed_gates),
    };
    plan_for_stage(
        scenario,
        start,
        agent.via.get(agent.via_cursor).map(String::as_str),
        agent.exit_candidates.iter().map(String::as_str),
        blocked_connectors,
        exit_constraints,
    )
}

fn plan_for_stage<'a>(
    scenario: &Scenario,
    start: RouteStart<'_>,
    waypoint_id: Option<&str>,
    destinations: impl IntoIterator<Item = &'a str>,
    blocked_connectors: &HashSet<String>,
    exit_constraints: ExitPlanConstraints<'_>,
) -> Option<PlannedTarget> {
    if let Some(waypoint_id) = waypoint_id {
        let target = waypoint_target(scenario, waypoint_id)?;
        let plan = route_to_target_avoiding(scenario, start, &target, blocked_connectors)?;
        return Some(PlannedTarget {
            target,
            plan,
            final_gate: None,
        });
    }
    select_exit_plan(
        scenario,
        start,
        destinations,
        blocked_connectors,
        exit_constraints,
    )
}

fn select_exit_plan<'a>(
    scenario: &Scenario,
    start: RouteStart<'_>,
    destinations: impl IntoIterator<Item = &'a str>,
    blocked_connectors: &HashSet<String>,
    exit_constraints: ExitPlanConstraints<'_>,
) -> Option<PlannedTarget> {
    destinations
        .into_iter()
        .enumerate()
        .filter_map(|(index, destination)| {
            if exit_constraints.blocked_exits.contains(destination) {
                return None;
            }
            let target = exit_target(scenario, destination)?;
            let (plan, final_gate) = route_to_exit_target(
                scenario,
                start,
                &target,
                blocked_connectors,
                exit_constraints,
            )?;
            Some((
                index,
                PlannedTarget {
                    target,
                    plan,
                    final_gate,
                },
            ))
        })
        .min_by(|(left_index, left), (right_index, right)| {
            left.plan
                .nominal_duration_s
                .total_cmp(&right.plan.nominal_duration_s)
                .then(left_index.cmp(right_index))
        })
        .map(|(_, planned)| planned)
}

fn route_to_exit_target(
    scenario: &Scenario,
    start: RouteStart<'_>,
    exit: &RouteTarget,
    blocked_connectors: &HashSet<String>,
    exit_constraints: ExitPlanConstraints<'_>,
) -> Option<(RoutePlan, Option<String>)> {
    let gates: Vec<_> = scenario
        .gates
        .iter()
        .filter(|gate| gate.destination == exit.id)
        .collect();
    if gates.is_empty() {
        return route_to_target_avoiding(scenario, start, exit, blocked_connectors)
            .map(|plan| (plan, None));
    }
    if gates.iter().any(|gate| {
        exit_constraints
            .passed_gates
            .is_some_and(|passed_gates| passed_gates.contains(&gate.id))
    }) {
        return route_to_target_avoiding(scenario, start, exit, blocked_connectors)
            .map(|plan| (plan, None));
    }
    gates
        .into_iter()
        .enumerate()
        .filter(|(_, gate)| !exit_constraints.closed_gates.contains(&gate.id))
        .filter_map(|(index, gate)| {
            let gate_target = RouteTarget {
                id: gate.id.clone(),
                surface: gate.surface.clone(),
                at: gate.at,
                is_exit: false,
                dwell_s: 0.0,
            };
            let mut plan =
                route_to_target_avoiding(scenario, start, &gate_target, blocked_connectors)?;
            let final_walk_s = walking_duration_s(
                scenario,
                &gate.surface,
                gate.at,
                exit.at,
                start.radius_m,
                start.walking_speed_mps,
            )?;
            plan.nominal_duration_s += final_walk_s;
            Some((index, plan, gate.id.clone()))
        })
        .min_by(|(left_index, left_plan, _), (right_index, right_plan, _)| {
            left_plan
                .nominal_duration_s
                .total_cmp(&right_plan.nominal_duration_s)
                .then(left_index.cmp(right_index))
        })
        .map(|(_, plan, gate_id)| (plan, Some(gate_id)))
}

fn agent_target(scenario: &Scenario, agent: &Agent) -> Option<RouteTarget> {
    if let Some(waypoint_id) = agent.via.get(agent.via_cursor) {
        return waypoint_target(scenario, waypoint_id);
    }
    exit_target(scenario, &agent.destination)
}

fn waypoint_target(scenario: &Scenario, waypoint_id: &str) -> Option<RouteTarget> {
    let waypoint = scenario
        .waypoints
        .iter()
        .find(|waypoint| waypoint.id == waypoint_id)?;
    Some(RouteTarget {
        id: waypoint.id.clone(),
        surface: waypoint.surface.clone(),
        at: waypoint.at,
        is_exit: false,
        dwell_s: waypoint.dwell_s,
    })
}

fn exit_target(scenario: &Scenario, exit_id: &str) -> Option<RouteTarget> {
    let exit = scenario.exits.iter().find(|exit| exit.id == exit_id)?;
    Some(RouteTarget {
        id: exit.id.clone(),
        surface: exit.surface.clone(),
        at: exit.at,
        is_exit: true,
        dwell_s: 0.0,
    })
}

fn route_to_target_avoiding(
    scenario: &Scenario,
    start: RouteStart<'_>,
    target: &RouteTarget,
    blocked_connectors: &HashSet<String>,
) -> Option<RoutePlan> {
    if start.surface == target.surface {
        return walking_duration_s(
            scenario,
            start.surface,
            start.position,
            target.at,
            start.radius_m,
            start.walking_speed_mps,
        )
        .map(|nominal_duration_s| RoutePlan {
            connector_indices: Vec::new(),
            nominal_duration_s,
        });
    }

    let mut durations = initial_connector_durations(scenario, start, blocked_connectors);
    let connector_count = scenario.connectors.len();
    let mut previous = vec![None; connector_count];
    let mut settled = vec![false; connector_count];
    // Connector nodes retain the arrival point needed to price the following
    // surface walk. This is a time-weighted directed route, not a hop-count
    // graph: a nominal lift cycle can be slower than multiple stairs.

    while let Some(current) = next_unsettled(&durations, &settled) {
        settled[current] = true;
        let duration = durations[current]?;
        let arrival = scenario.connectors[current].to();
        let surface = scenario.connectors[current].to_surface();
        for (next, connector) in scenario.connectors.iter().enumerate() {
            if settled[next]
                || blocked_connectors.contains(connector.id())
                || !start.allows_connector(connector)
                || connector.from_surface() != surface
            {
                continue;
            }
            let Some(walking_duration) = walking_duration_s(
                scenario,
                surface,
                arrival,
                connector.from(),
                start.radius_m,
                start.walking_speed_mps,
            ) else {
                continue;
            };
            let candidate = duration
                + walking_duration
                + connector.traversal_duration_s(start.walking_speed_mps);
            if durations[next].is_none_or(|current_duration| candidate < current_duration) {
                durations[next] = Some(candidate);
                previous[next] = Some(current);
            }
        }
    }

    let (terminal, nominal_duration_s) = scenario
        .connectors
        .iter()
        .enumerate()
        .filter(|(_, connector)| connector.to_surface() == target.surface)
        .filter_map(|(index, connector)| {
            durations[index]
                .zip(walking_duration_s(
                    scenario,
                    connector.to_surface(),
                    connector.to(),
                    target.at,
                    start.radius_m,
                    start.walking_speed_mps,
                ))
                .map(|(duration, walking_duration)| (index, duration + walking_duration))
        })
        .min_by(
            |(left_index, left_duration), (right_index, right_duration)| {
                left_duration
                    .total_cmp(right_duration)
                    .then(left_index.cmp(right_index))
            },
        )?;
    let mut route = vec![terminal];
    let mut cursor = terminal;
    while let Some(parent) = previous[cursor] {
        route.push(parent);
        cursor = parent;
    }
    route.reverse();
    Some(RoutePlan {
        connector_indices: route,
        nominal_duration_s,
    })
}

fn initial_connector_durations(
    scenario: &Scenario,
    start: RouteStart<'_>,
    blocked_connectors: &HashSet<String>,
) -> Vec<Option<f64>> {
    scenario
        .connectors
        .iter()
        .map(|connector| {
            if blocked_connectors.contains(connector.id())
                || !start.allows_connector(connector)
                || connector.from_surface() != start.surface
            {
                return None;
            }
            let walking_duration = walking_duration_s(
                scenario,
                start.surface,
                start.position,
                connector.from(),
                start.radius_m,
                start.walking_speed_mps,
            )?;
            Some(walking_duration + connector.traversal_duration_s(start.walking_speed_mps))
        })
        .collect()
}

fn next_unsettled(durations: &[Option<f64>], settled: &[bool]) -> Option<usize> {
    let mut next: Option<usize> = None;
    for (index, duration) in durations.iter().enumerate() {
        if settled[index] || duration.is_none() {
            continue;
        }
        if next
            .is_none_or(|current| duration.expect("checked") < durations[current].expect("checked"))
        {
            next = Some(index);
        }
    }
    next
}

#[derive(Debug, Clone, Copy)]
struct InflatedObstacle {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

impl InflatedObstacle {
    fn from_obstacle(obstacle: &Obstacle, radius_m: f64) -> Self {
        let clearance_m = radius_m + NAVIGATION_EPSILON_M;
        Self {
            min_x: obstacle.at.x_m - clearance_m,
            max_x: obstacle.at.x_m + obstacle.width_m + clearance_m,
            min_y: obstacle.at.y_m - clearance_m,
            max_y: obstacle.at.y_m + obstacle.depth_m + clearance_m,
        }
    }

    fn contains(self, point: Point3) -> bool {
        point.x_m >= self.min_x
            && point.x_m <= self.max_x
            && point.y_m >= self.min_y
            && point.y_m <= self.max_y
    }

    fn corners(self, z_m: f64) -> [Point3; 4] {
        let outer = NAVIGATION_EPSILON_M;
        [
            Point3 {
                x_m: self.min_x - outer,
                y_m: self.min_y - outer,
                z_m,
            },
            Point3 {
                x_m: self.min_x - outer,
                y_m: self.max_y + outer,
                z_m,
            },
            Point3 {
                x_m: self.max_x + outer,
                y_m: self.min_y - outer,
                z_m,
            },
            Point3 {
                x_m: self.max_x + outer,
                y_m: self.max_y + outer,
                z_m,
            },
        ]
    }
}

fn walking_duration_s(
    scenario: &Scenario,
    surface_id: &str,
    start: Point3,
    target: Point3,
    radius_m: f64,
    walking_speed_mps: f64,
) -> Option<f64> {
    let surface = scenario
        .surfaces
        .iter()
        .find(|surface| surface.id == surface_id)?;
    let path =
        shortest_walk_path_on_surface(surface, &scenario.obstacles, start, target, radius_m)?;
    Some(
        path.windows(2)
            .map(|segment| segment[0].distance(segment[1]))
            .sum::<f64>()
            / walking_speed_mps,
    )
}

/// Compute a deterministic Euclidean shortest path in a rectangular surface
/// with rectangular no-go zones. Obstacles are expanded by the agent radius,
/// then a visibility graph over their clearance corners is solved exactly.
fn shortest_walk_path_on_surface(
    surface: &Surface,
    obstacles: &[Obstacle],
    start: Point3,
    target: Point3,
    radius_m: f64,
) -> Option<Vec<Point3>> {
    if !surface.contains(start) || !surface.contains(target) {
        return None;
    }
    if start.distance(target) <= NAVIGATION_EPSILON_M {
        return Some(vec![start]);
    }
    let obstacles: Vec<_> = obstacles
        .iter()
        .filter(|obstacle| obstacle.surface == surface.id)
        .map(|obstacle| InflatedObstacle::from_obstacle(obstacle, radius_m))
        .collect();
    if obstacles
        .iter()
        .any(|obstacle| obstacle.contains(start) || obstacle.contains(target))
    {
        return None;
    }

    let mut nodes = vec![start, target];
    for obstacle in &obstacles {
        for corner in obstacle.corners(surface.origin.z_m) {
            if surface.contains(corner) {
                nodes.push(corner);
            }
        }
    }

    let mut distances = vec![None; nodes.len()];
    let mut previous = vec![None; nodes.len()];
    let mut settled = vec![false; nodes.len()];
    distances[0] = Some(0.0);
    while let Some(current) = next_unsettled(&distances, &settled) {
        if current == 1 {
            break;
        }
        settled[current] = true;
        let duration = distances[current].expect("selected nodes have a distance");
        for next in 0..nodes.len() {
            if settled[next]
                || next == current
                || !segment_is_clear(nodes[current], nodes[next], &obstacles)
            {
                continue;
            }
            let candidate = duration + nodes[current].distance(nodes[next]);
            if distances[next].is_none_or(|current_duration| candidate < current_duration) {
                distances[next] = Some(candidate);
                previous[next] = Some(current);
            }
        }
    }
    distances[1]?;
    let mut path = vec![target];
    let mut cursor = 1;
    while cursor != 0 {
        cursor = previous[cursor]?;
        path.push(nodes[cursor]);
    }
    path.reverse();
    Some(path)
}

fn segment_is_clear(start: Point3, end: Point3, obstacles: &[InflatedObstacle]) -> bool {
    !obstacles
        .iter()
        .any(|obstacle| segment_intersects_obstacle(start, end, *obstacle))
}

fn segment_intersects_obstacle(start: Point3, end: Point3, obstacle: InflatedObstacle) -> bool {
    let mut entry = 0.0_f64;
    let mut exit = 1.0_f64;
    for (start_coordinate, delta, minimum, maximum) in [
        (
            start.x_m,
            end.x_m - start.x_m,
            obstacle.min_x,
            obstacle.max_x,
        ),
        (
            start.y_m,
            end.y_m - start.y_m,
            obstacle.min_y,
            obstacle.max_y,
        ),
    ] {
        if delta.abs() <= NAVIGATION_EPSILON_M {
            if start_coordinate < minimum || start_coordinate > maximum {
                return false;
            }
            continue;
        }
        let first = (minimum - start_coordinate) / delta;
        let second = (maximum - start_coordinate) / delta;
        entry = entry.max(first.min(second));
        exit = exit.min(first.max(second));
        if entry > exit {
            return false;
        }
    }
    true
}

fn deliver_message(
    scenario: &Scenario,
    agents: &mut [Agent],
    message: &crate::model::Message,
    events: &mut Vec<RunEvent>,
    resources: &RuntimeResources,
    information_delivery: &mut BTreeMap<String, InformationDeliveryMetrics>,
) {
    for agent in agents.iter_mut().filter(|agent| {
        agent.surface == message.surface && matches!(agent.motion, Motion::OnSurface)
    }) {
        if agent.position.distance(message.origin) <= message.reach_m {
            agent.beliefs.insert(message.id.clone(), message.trust);
            let sampling_key = message.sampling_key.as_deref().unwrap_or(&message.id);
            let accepted =
                accepts_information(scenario.seed, &agent.id, sampling_key, message.trust);
            let delivery = information_delivery
                .get_mut(&message.id)
                .expect("every scheduled message has delivery metrics");
            delivery.received_agents += 1;
            if accepted {
                delivery.accepted_agents += 1;
            }
            if accepted {
                apply_claim(agent, &message.claim);
                reroute(
                    scenario,
                    agent,
                    message.at_s,
                    events,
                    &resources.closed_connectors,
                    &resources.closed_exits,
                    &resources.closed_gates,
                );
            }
            events.push(RunEvent {
                time_s: message.at_s,
                kind: "message_received".to_owned(),
                subject: agent.id.clone(),
                detail: format!(
                    "{}: {} `{}` {} from {} (truthful={}, accepted={accepted}, sample={sampling_key})",
                    message.id,
                    message.claim.subject_kind(),
                    message.claim.subject(),
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

fn deliver_countermeasure(
    scenario: &Scenario,
    agents: &mut [Agent],
    countermeasure: &crate::model::Countermeasure,
    events: &mut Vec<RunEvent>,
    resources: &RuntimeResources,
    information_delivery: &mut BTreeMap<String, InformationDeliveryMetrics>,
) {
    for agent in agents.iter_mut().filter(|agent| {
        agent.surface == countermeasure.surface && matches!(agent.motion, Motion::OnSurface)
    }) {
        if agent.position.distance(countermeasure.origin) <= countermeasure.reach_m {
            agent
                .beliefs
                .insert(countermeasure.corrects.clone(), countermeasure.trust);
            let sampling_key = countermeasure
                .sampling_key
                .as_deref()
                .unwrap_or(&countermeasure.id);
            let accepted =
                accepts_information(scenario.seed, &agent.id, sampling_key, countermeasure.trust);
            let delivery = information_delivery
                .get_mut(&countermeasure.id)
                .expect("every scheduled countermeasure has delivery metrics");
            delivery.received_agents += 1;
            if accepted {
                delivery.accepted_agents += 1;
            }
            if accepted
                && let Some(message) = scenario
                    .messages
                    .iter()
                    .find(|message| message.id == countermeasure.corrects)
            {
                apply_claim_to_physical_state(
                    agent,
                    &message.claim,
                    &resources.closed_connectors,
                    &resources.closed_exits,
                );
                reroute(
                    scenario,
                    agent,
                    countermeasure.at_s,
                    events,
                    &resources.closed_connectors,
                    &resources.closed_exits,
                    &resources.closed_gates,
                );
            }
            events.push(RunEvent {
                time_s: countermeasure.at_s,
                kind: "countermeasure_received".to_owned(),
                subject: agent.id.clone(),
                detail: format!(
                    "{} corrected by {} via {} (accepted={accepted}, sample={sampling_key})",
                    countermeasure.corrects,
                    countermeasure.id,
                    countermeasure.source.as_str()
                ),
            });
        }
    }
}

fn accepts_information(seed: u64, agent_id: &str, intervention_id: &str, trust: f64) -> bool {
    if trust <= 0.0 {
        return false;
    }
    if trust >= 1.0 {
        return true;
    }
    deterministic_unit_interval(seed, agent_id, intervention_id) < trust
}

fn deterministic_unit_interval(seed: u64, agent_id: &str, intervention_id: &str) -> f64 {
    let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
    for byte in agent_id
        .bytes()
        .chain(std::iter::once(0))
        .chain(intervention_id.bytes())
    {
        state ^= u64::from(byte);
        state = state.wrapping_mul(0x1000_0000_01b3);
    }
    let mixed = splitmix64(state);
    let numerator = u32::try_from(mixed >> 40).expect("top 24 bits fit u32");
    f64::from(numerator) / f64::from(1_u32 << 24)
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn apply_claim(agent: &mut Agent, claim: &crate::model::Proposition) {
    match claim {
        crate::model::Proposition::ConnectorAvailability { connector, open } => {
            apply_connector_belief(agent, connector, *open);
        }
        crate::model::Proposition::ExitAvailability { exit, open } => {
            apply_exit_belief(agent, exit, *open);
        }
    }
}

fn apply_claim_to_physical_state(
    agent: &mut Agent,
    claim: &crate::model::Proposition,
    closed_connectors: &HashSet<String>,
    closed_exits: &HashSet<String>,
) {
    match claim {
        crate::model::Proposition::ConnectorAvailability { connector, .. } => {
            apply_connector_belief(agent, connector, !closed_connectors.contains(connector));
        }
        crate::model::Proposition::ExitAvailability { exit, .. } => {
            apply_exit_belief(agent, exit, !closed_exits.contains(exit));
        }
    }
}

fn apply_connector_belief(agent: &mut Agent, connector: &str, open: bool) {
    if open {
        agent.blocked_connectors.remove(connector);
    } else {
        agent.blocked_connectors.insert(connector.to_owned());
    }
}

fn apply_exit_belief(agent: &mut Agent, exit: &str, open: bool) {
    if open {
        agent.blocked_exits.remove(exit);
    } else {
        agent.blocked_exits.insert(exit.to_owned());
    }
}

fn reroute(
    scenario: &Scenario,
    agent: &mut Agent,
    time_s: f64,
    events: &mut Vec<RunEvent>,
    closed_connectors: &HashSet<String>,
    closed_exits: &HashSet<String>,
    closed_gates: &HashSet<String>,
) {
    if !matches!(agent.motion, Motion::OnSurface) {
        return;
    }
    let blocked_connectors = effective_blocked_connectors(agent, closed_connectors);
    let blocked_exits = effective_blocked_exits(agent, closed_exits);
    if let Some(planned) = agent_plan(
        scenario,
        RouteStart {
            surface: &agent.surface,
            position: agent.position,
            walking_speed_mps: agent.speed_mps,
            radius_m: agent.radius_m,
            height_m: agent.height_m,
            excluded_connector_kinds: &agent.excluded_connector_kinds,
        },
        agent,
        &blocked_connectors,
        closed_exits,
        closed_gates,
    ) {
        if planned.target.is_exit {
            agent.destination.clone_from(&planned.target.id);
        }
        agent.final_gate = planned.final_gate;
        agent.route = planned.plan.connector_indices;
        agent.route_cursor = 0;
        agent.waiting_connector = None;
        agent.waiting_for_route = false;
        agent.waiting_for_exit = false;
        let mut blocked: Vec<_> = blocked_connectors
            .into_iter()
            .map(|connector| format!("connector:{connector}"))
            .chain(blocked_exits.into_iter().map(|exit| format!("exit:{exit}")))
            .chain(closed_gates.iter().map(|gate| format!("gate:{gate}")))
            .collect();
        blocked.sort_unstable();
        events.push(RunEvent {
            time_s,
            kind: "route_recomputed".to_owned(),
            subject: agent.id.clone(),
            detail: blocked.join(","),
        });
    } else {
        agent.waiting_for_route = true;
        events.push(RunEvent {
            time_s,
            kind: "route_unavailable".to_owned(),
            subject: agent.id.clone(),
            detail: "current physical and believed constraints do not reach the destination"
                .to_owned(),
        });
    }
}

fn effective_blocked_connectors(
    agent: &Agent,
    closed_connectors: &HashSet<String>,
) -> HashSet<String> {
    let mut blocked_connectors = closed_connectors.clone();
    blocked_connectors.extend(agent.blocked_connectors.iter().cloned());
    blocked_connectors
}

fn effective_blocked_exits(agent: &Agent, closed_exits: &HashSet<String>) -> HashSet<String> {
    let mut blocked_exits = closed_exits.clone();
    blocked_exits.extend(agent.blocked_exits.iter().cloned());
    blocked_exits
}

#[allow(clippy::too_many_lines)] // this is the reference small-step transition relation
fn integrate(
    scenario: &Scenario,
    agents: &mut [Agent],
    time_s: f64,
    events: &mut Vec<RunEvent>,
    resources: &mut RuntimeResources,
) {
    for agent in agents.iter_mut() {
        match &agent.motion {
            Motion::WaitingToDepart { release_at_s } if *release_at_s <= time_s => {
                let release_at_s = *release_at_s;
                agent.motion = Motion::OnSurface;
                events.push(RunEvent {
                    time_s: release_at_s,
                    kind: "agent_released".to_owned(),
                    subject: agent.id.clone(),
                    detail: agent.group.clone(),
                });
            }
            Motion::WaitingAtWaypoint {
                until_s,
                waypoint_id,
            } if *until_s <= time_s => {
                let until_s = *until_s;
                let waypoint_id = waypoint_id.clone();
                agent.motion = Motion::OnSurface;
                events.push(RunEvent {
                    time_s: until_s,
                    kind: "waypoint_wait_ended".to_owned(),
                    subject: agent.id.clone(),
                    detail: waypoint_id,
                });
            }
            _ => {}
        }
    }
    let spatial_snapshot = SpatialSnapshot::from_agents(agents);
    let mut lift_loads = current_lift_loads(agents);
    for (index, connector) in scenario.connectors.iter().enumerate() {
        let Some(service_rate_per_s) = scenario.connector_service_rate_at(connector.id(), time_s)
        else {
            continue;
        };
        let token = resources.connector_tokens.entry(index).or_default();
        *token =
            (*token + service_rate_per_s * scenario.timestep_s).min(service_rate_per_s.max(1.0));
    }
    for gate in &scenario.gates {
        let service_rate_per_s = scenario
            .gate_service_rate_at(&gate.id, time_s)
            .expect("validated gate exists");
        let token = resources.gate_tokens.entry(gate.id.clone()).or_default();
        *token =
            (*token + service_rate_per_s * scenario.timestep_s).min(service_rate_per_s.max(1.0));
    }
    for exit in &scenario.exits {
        let Some(capacity_per_s) = scenario.exit_capacity_at(&exit.id, time_s) else {
            continue;
        };
        let token = resources.exit_tokens.entry(exit.id.clone()).or_default();
        *token = (*token + capacity_per_s * scenario.timestep_s).min(capacity_per_s.max(1.0));
    }
    for (index, agent) in agents.iter_mut().enumerate() {
        match &mut agent.motion {
            Motion::WaitingToDepart { .. }
            | Motion::WaitingAtWaypoint { .. }
            | Motion::Evacuated { .. } => {}
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
                    if scenario.connectors[*connector_index].is_lift() {
                        let load = lift_loads.entry(*connector_index).or_default();
                        *load = load.saturating_sub(1);
                    }
                    agent.surface = next_surface.clone();
                    agent.route_cursor += 1;
                    agent.motion = Motion::OnSurface;
                    events.push(RunEvent {
                        time_s,
                        kind: "connector_arrival".to_owned(),
                        subject: agent.id.clone(),
                        detail: connector_id,
                    });
                    let remaining_connector_is_closed =
                        agent.route[agent.route_cursor..].iter().any(|index| {
                            resources
                                .closed_connectors
                                .contains(scenario.connectors[*index].id())
                        });
                    let final_exit_is_closed = agent.route_cursor == agent.route.len()
                        && resources.closed_exits.contains(&agent.destination);
                    if remaining_connector_is_closed || final_exit_is_closed {
                        reroute(
                            scenario,
                            agent,
                            time_s,
                            events,
                            &resources.closed_connectors,
                            &resources.closed_exits,
                            &resources.closed_gates,
                        );
                    }
                }
            }
            Motion::OnSurface => {
                if agent.waiting_for_route {
                    continue;
                }
                let (target, next_connector, final_gate, reached_waypoint) =
                    if let Some(&connector_index) = agent.route.get(agent.route_cursor) {
                        (
                            scenario.connectors[connector_index].from(),
                            Some(connector_index),
                            None,
                            None,
                        )
                    } else {
                        let target =
                            agent_target(scenario, agent).expect("validated journey target exists");
                        if target.is_exit {
                            let gate = agent
                                .final_gate
                                .as_deref()
                                .filter(|gate_id| !agent.passed_gates.contains(*gate_id))
                                .and_then(|gate_id| {
                                    scenario.gates.iter().find(|gate| gate.id == gate_id)
                                });
                            match gate {
                                Some(gate) => (gate.at, None, Some(gate.id.clone()), None),
                                None => (target.at, None, None, None),
                            }
                        } else {
                            (target.at, None, None, Some((target.id, target.dwell_s)))
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
                    &spatial_snapshot,
                    index,
                    surface,
                    &scenario.obstacles,
                );
                if !reached {
                    continue;
                }
                match next_connector {
                    None => {
                        if let Some((waypoint_id, dwell_s)) = reached_waypoint {
                            agent.via_cursor += 1;
                            let blocked_connectors =
                                effective_blocked_connectors(agent, &resources.closed_connectors);
                            if let Some(planned) = agent_plan(
                                scenario,
                                RouteStart {
                                    surface: &agent.surface,
                                    position: agent.position,
                                    walking_speed_mps: agent.speed_mps,
                                    radius_m: agent.radius_m,
                                    height_m: agent.height_m,
                                    excluded_connector_kinds: &agent.excluded_connector_kinds,
                                },
                                agent,
                                &blocked_connectors,
                                &resources.closed_exits,
                                &resources.closed_gates,
                            ) {
                                if planned.target.is_exit {
                                    agent.destination.clone_from(&planned.target.id);
                                }
                                agent.final_gate = planned.final_gate;
                                agent.route = planned.plan.connector_indices;
                                agent.route_cursor = 0;
                                events.push(RunEvent {
                                    time_s,
                                    kind: "waypoint_reached".to_owned(),
                                    subject: agent.id.clone(),
                                    detail: waypoint_id,
                                });
                                if dwell_s > 0.0 {
                                    agent.motion = Motion::WaitingAtWaypoint {
                                        until_s: time_s + dwell_s,
                                        waypoint_id: agent
                                            .via
                                            .get(agent.via_cursor.saturating_sub(1))
                                            .expect("reached waypoint remains in journey")
                                            .clone(),
                                    };
                                    events.push(RunEvent {
                                        time_s,
                                        kind: "waypoint_wait_started".to_owned(),
                                        subject: agent.id.clone(),
                                        detail: format!("{dwell_s}s"),
                                    });
                                }
                            } else {
                                agent.waiting_for_route = true;
                                events.push(RunEvent {
                                    time_s,
                                    kind: "route_unavailable".to_owned(),
                                    subject: agent.id.clone(),
                                    detail: "waypoint reached but destination is unavailable"
                                        .to_owned(),
                                });
                            }
                        } else if let Some(gate_id) = final_gate {
                            if resources.closed_gates.contains(&gate_id) {
                                reroute(
                                    scenario,
                                    agent,
                                    time_s,
                                    events,
                                    &resources.closed_connectors,
                                    &resources.closed_exits,
                                    &resources.closed_gates,
                                );
                                continue;
                            }
                            let tokens = resources.gate_tokens.entry(gate_id.clone()).or_default();
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
                                resources.queued_for_gate_agents.insert(agent.id.clone());
                            }
                        } else {
                            if effective_blocked_exits(agent, &resources.closed_exits)
                                .contains(&agent.destination)
                            {
                                reroute(
                                    scenario,
                                    agent,
                                    time_s,
                                    events,
                                    &resources.closed_connectors,
                                    &resources.closed_exits,
                                    &resources.closed_gates,
                                );
                                continue;
                            }
                            let exit = scenario
                                .exits
                                .iter()
                                .find(|exit| exit.id == agent.destination)
                                .expect("validated exit exists");
                            if scenario.exit_capacity_at(&exit.id, time_s).is_some() {
                                let tokens =
                                    resources.exit_tokens.entry(exit.id.clone()).or_default();
                                if *tokens < 1.0 {
                                    resources.queued_for_exit_agents.insert(agent.id.clone());
                                    agent.waiting_for_exit = true;
                                    continue;
                                }
                                *tokens -= 1.0;
                            }
                            agent.waiting_for_exit = false;
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
                        if !connector.supports_height(agent.height_m)
                            || agent.excluded_connector_kinds.contains(&connector.kind())
                        {
                            agent.waiting_for_route = true;
                            continue;
                        }
                        if resources.closed_connectors.contains(connector.id()) {
                            agent.waiting_for_route = true;
                            continue;
                        }
                        if connector.is_lift()
                            && lift_loads
                                .get(&connector_index)
                                .copied()
                                .unwrap_or_default()
                                >= connector.capacity().expect("lift has capacity")
                        {
                            resources.queued_for_lift_agents.insert(agent.id.clone());
                            agent.waiting_connector = Some(ConnectorWait::Lift);
                            continue;
                        }
                        if scenario
                            .connector_service_rate_at(connector.id(), time_s)
                            .is_some()
                        {
                            let tokens = resources
                                .connector_tokens
                                .entry(connector_index)
                                .or_default();
                            if *tokens < 1.0 {
                                resources
                                    .queued_for_connector_agents
                                    .insert(agent.id.clone());
                                agent.waiting_connector = Some(ConnectorWait::Capacity);
                                continue;
                            }
                            *tokens -= 1.0;
                        }
                        let duration_s = connector.traversal_duration_s(agent.speed_mps);
                        agent.waiting_connector = None;
                        agent.waiting_for_route = false;
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

#[allow(clippy::cast_possible_truncation)] // validated finite radii determine a bounded local-cell query
fn move_toward(
    agent: &mut Agent,
    target: Point3,
    timestep_s: f64,
    spatial_snapshot: &SpatialSnapshot,
    own_index: usize,
    surface: &Surface,
    obstacles: &[Obstacle],
) -> bool {
    let Some(path) =
        shortest_walk_path_on_surface(surface, obstacles, agent.position, target, agent.radius_m)
    else {
        return false;
    };
    let waypoint = path.get(1).copied().unwrap_or(target);
    let dx = waypoint.x_m - agent.position.x_m;
    let dy = waypoint.y_m - agent.position.y_m;
    let dz = waypoint.z_m - agent.position.z_m;
    let distance = (dx.mul_add(dx, dy.mul_add(dy, dz * dz))).sqrt();
    let travel = agent.speed_mps * timestep_s;
    if distance <= travel {
        agent.position = waypoint;
        return path.len() <= 2;
    }
    let mut next = Point3 {
        x_m: agent.position.x_m + (dx / distance) * travel,
        y_m: agent.position.y_m + (dy / distance) * travel,
        z_m: agent.position.z_m + (dz / distance) * travel,
    };
    let planned_next = next;
    let cell_range =
        ((agent.radius_m + spatial_snapshot.max_radius_m) / SPATIAL_CELL_M).ceil() as i64;
    let center = cell_key(&agent.surface, next);
    for offset_y in -cell_range..=cell_range {
        for offset_x in -cell_range..=cell_range {
            let key = CellKey {
                surface: agent.surface.clone(),
                x: center.x + offset_x,
                y: center.y + offset_y,
            };
            let Some(candidates) = spatial_snapshot.cells.get(&key) else {
                continue;
            };
            for candidate in candidates {
                if *candidate == own_index {
                    continue;
                }
                let Some((_, position, radius)) = &spatial_snapshot.positions[*candidate] else {
                    continue;
                };
                let separation = next.distance(*position);
                let minimum = agent.radius_m + radius;
                if separation > 0.0 && separation < minimum {
                    let correction = (minimum - separation) / minimum * agent.radius_m;
                    next.x_m += (next.x_m - position.x_m) / separation * correction;
                    next.y_m += (next.y_m - position.y_m) / separation * correction;
                }
            }
        }
    }
    next.x_m = next
        .x_m
        .clamp(surface.origin.x_m, surface.origin.x_m + surface.width_m);
    next.y_m = next
        .y_m
        .clamp(surface.origin.y_m, surface.origin.y_m + surface.depth_m);
    next.z_m = surface.origin.z_m;
    if !point_is_clear(next, surface, obstacles, agent.radius_m) {
        next = planned_next;
    }
    agent.position = next;
    false
}

fn point_is_clear(point: Point3, surface: &Surface, obstacles: &[Obstacle], radius_m: f64) -> bool {
    surface.contains(point)
        && obstacles
            .iter()
            .filter(|obstacle| obstacle.surface == surface.id)
            .all(|obstacle| !InflatedObstacle::from_obstacle(obstacle, radius_m).contains(point))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn cell_key(surface: &str, point: Point3) -> CellKey {
    CellKey {
        surface: surface.to_owned(),
        x: (point.x_m / SPATIAL_CELL_M).floor() as i64,
        y: (point.y_m / SPATIAL_CELL_M).floor() as i64,
    }
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
                state: agent_state(agent),
                beliefs: agent.beliefs.clone(),
            })
            .collect(),
    }
}

fn agent_state(agent: &Agent) -> AgentState {
    match &agent.motion {
        Motion::WaitingToDepart { .. } => AgentState::WaitingToDepart,
        Motion::WaitingAtWaypoint { .. } => AgentState::WaitingAtWaypoint,
        Motion::OnSurface if agent.waiting_for_route => AgentState::WaitingForRoute,
        Motion::OnSurface if agent.waiting_for_exit => AgentState::WaitingForExit,
        Motion::OnSurface => match agent.waiting_connector {
            Some(ConnectorWait::Lift) => AgentState::WaitingForLift,
            Some(ConnectorWait::Capacity) => AgentState::WaitingForConnector,
            None => AgentState::Moving,
        },
        Motion::Transit { .. } => AgentState::InTransit,
        Motion::Evacuated { .. } => AgentState::Evacuated,
    }
}

fn agent_state_name(agent: &Agent) -> &'static str {
    match agent_state(agent) {
        AgentState::Moving => "moving",
        AgentState::WaitingToDepart => "waiting_to_depart",
        AgentState::WaitingAtWaypoint => "waiting_at_waypoint",
        AgentState::WaitingForRoute => "waiting_for_route",
        AgentState::WaitingForLift => "waiting_for_lift",
        AgentState::WaitingForConnector => "waiting_for_connector",
        AgentState::WaitingForExit => "waiting_for_exit",
        AgentState::InTransit => "in_transit",
        AgentState::Evacuated => "evacuated",
    }
}
