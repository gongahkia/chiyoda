//! Exact continuous-time reference-disc conflict detection for coordinated
//! on-surface plans.
//!
//! This module deliberately models only a plan's declared linear segments.
//! It is not a physical-contact model and does not turn a conflict-free plan
//! into a safety certificate. Its purpose is to give a future queue-grid
//! coordinator one deterministic, geometry-aware definition of a conflict,
//! rather than letting a planner and the runtime use incompatible endpoint
//! checks.

use crate::model::{NAVIGATION_CLEARANCE_EPSILON_M, Obstacle, Point3, Surface};
use std::{
    cmp::{Ordering, Reverse},
    collections::{BinaryHeap, HashMap, HashSet},
};
use thiserror::Error;

/// One linear on-surface movement or wait interval for a circular reference
/// disc. The endpoints are interpreted at `starts_at_s` and `ends_at_s`.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedDiscSegment {
    pub surface: String,
    pub starts_at_s: f64,
    pub ends_at_s: f64,
    pub start: Point3,
    pub end: Point3,
    pub radius_m: f64,
}

/// An ordered sequence of declared on-surface reference-disc segments.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedDiscTrajectory {
    pub agent_id: String,
    pub segments: Vec<TimedDiscSegment>,
}

/// One maximal open time interval whose two reference discs overlap beyond the
/// configured numerical clearance epsilon.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedDiscConflict {
    pub first_agent_id: String,
    pub second_agent_id: String,
    pub surface: String,
    pub first_segment_index: usize,
    pub second_segment_index: usize,
    pub unsafe_from_s: f64,
    pub unsafe_until_s: f64,
    pub maximum_overlap_m: f64,
}

/// A statically cleared roadmap vertex supplied to the coordination planner.
/// The planner does not infer walkability: callers must create nodes and edges
/// only after applying the runtime's surface and obstacle-clearance rules.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinationRoadmapNode {
    pub surface: String,
    pub position: Point3,
}

/// An undirected statically cleared transition between two roadmap vertices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoordinationRoadmapEdge {
    pub first_node: usize,
    pub second_node: usize,
}

/// A finite static roadmap for one or more on-surface coordination problems.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinationRoadmap {
    nodes: Vec<CoordinationRoadmapNode>,
    adjacency: Vec<Vec<usize>>,
}

/// The deterministic lattice roadmap and the node selected for each requested
/// anchor, in anchor input order.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinationLatticeRoadmap {
    pub roadmap: CoordinationRoadmap,
    pub anchor_nodes: Vec<usize>,
}

/// One bounded single-agent search request. `occupied_trajectories` must
/// represent other agents only; each candidate wait or move is checked against
/// their continuous reference-disc paths.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeExpandedPlanRequest<'a> {
    pub agent_id: String,
    pub start_node: usize,
    pub goal_node: usize,
    pub radius_m: f64,
    pub earliest_start_s: f64,
    pub reserve_until_s: f64,
    pub speed_mps: f64,
    pub timestep_s: f64,
    pub maximum_expansions: u64,
    pub clearance_epsilon_m: f64,
    pub occupied_trajectories: &'a [TimedDiscTrajectory],
}

/// A conflict-free-on-the-roadmap candidate returned by the bounded planner.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeExpandedPlan {
    pub trajectory: TimedDiscTrajectory,
    pub explored_states: u64,
}

/// One exact roadmap target that is active over a queue-grid task window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedRoadmapTarget {
    pub node: usize,
    pub starts_at_s: f64,
    pub ends_at_s: f64,
}

/// One bounded multi-stage request, such as a ticket advancing through
/// successive queue-grid slot ranks after FIFO service handoffs.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiStagePlanRequest<'a> {
    pub agent_id: String,
    pub start_node: usize,
    pub radius_m: f64,
    pub earliest_start_s: f64,
    pub speed_mps: f64,
    pub timestep_s: f64,
    pub maximum_expansions_per_stage: u64,
    pub clearance_epsilon_m: f64,
    pub targets: Vec<TimedRoadmapTarget>,
    pub occupied_trajectories: &'a [TimedDiscTrajectory],
}

/// A task-window-conforming trajectory from the multi-stage planner.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiStagePlan {
    pub trajectory: TimedDiscTrajectory,
    pub explored_states: u64,
}

/// The physical objective for one agent in a coordinated roadmap solve.
///
/// `TimedTargets` is the queue-grid form: every window is a distinct rank
/// task, so a service handoff cannot be flattened into a permanent goal.
#[derive(Debug, Clone, PartialEq)]
pub enum CoordinationAgentTask {
    Goal {
        goal_node: usize,
        reserve_until_s: f64,
    },
    TimedTargets {
        targets: Vec<TimedRoadmapTarget>,
    },
}

/// One agent's immutable request within a coordinated roadmap solve.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinationAgentRequest {
    pub agent_id: String,
    pub start_node: usize,
    pub radius_m: f64,
    pub earliest_start_s: f64,
    pub speed_mps: f64,
    pub task: CoordinationAgentTask,
}

/// Inputs for a bounded conflict-repair solve. Initial paths are independently
/// planned, then every branch replans one participant against the other
/// currently declared paths. The final candidate is independently checked by
/// [`timed_disc_conflicts`].
#[derive(Debug, Clone, PartialEq)]
pub struct ConflictRepairRequest<'a> {
    pub agents: Vec<CoordinationAgentRequest>,
    /// Earlier accepted trajectories that this solve must preserve. This
    /// supports bounded rolling-window planning without re-opening a complete
    /// historical conflict tree.
    pub occupied_trajectories: &'a [TimedDiscTrajectory],
    pub timestep_s: f64,
    pub maximum_low_level_expansions: u64,
    pub maximum_conflict_tree_nodes: u64,
    pub clearance_epsilon_m: f64,
    pub roadmap: &'a CoordinationRoadmap,
}

/// A conflict-free result from the bounded repair tree.
#[derive(Debug, Clone, PartialEq)]
pub struct ConflictRepairPlan {
    pub trajectories: Vec<TimedDiscTrajectory>,
    pub explored_conflict_tree_nodes: u64,
    pub low_level_explored_states: u64,
}

/// Immutable physical input for one FIFO ticket in a coordinated queue-grid
/// solve. `activation_at_s` is when the ticket becomes part of the physical
/// queue rank; it is not a prediction of when that agent will receive service.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueGridTicketRequest {
    pub ticket: u64,
    pub agent_id: String,
    pub start_node: usize,
    pub radius_m: f64,
    pub activation_at_s: f64,
    pub speed_mps: f64,
}

/// Complete inputs for a bounded queue-grid solve. Service departures are
/// explicit scheduling facts supplied by the caller; this planner does not
/// infer a service rate, capacity, or human queueing behaviour.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueGridCoordinationRequest<'a> {
    /// Roadmap node by authored FIFO rank, with index zero at the service head.
    pub slot_nodes: &'a [usize],
    pub tickets: Vec<QueueGridTicketRequest>,
    pub departures: Vec<QueueGridServiceDeparture>,
    pub horizon_s: f64,
    pub occupied_trajectories: &'a [TimedDiscTrajectory],
    pub timestep_s: f64,
    pub maximum_low_level_expansions: u64,
    pub maximum_conflict_tree_nodes: u64,
    pub clearance_epsilon_m: f64,
    pub roadmap: &'a CoordinationRoadmap,
}

/// The exact FIFO task timeline and bounded conflict-repair result for one
/// queue-grid solve.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueGridCoordinationPlan {
    pub slot_windows: Vec<QueueGridSlotWindow>,
    pub repair_plan: ConflictRepairPlan,
}

/// A bounded rolling variant of [`QueueGridCoordinationRequest`]. Tickets are
/// planned from the back of the authored queue toward the service head, while
/// every later cohort treats all earlier accepted trajectories as immutable
/// occupancy. This formation order never changes FIFO service eligibility.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueGridRollingCoordinationRequest<'a> {
    pub queue: QueueGridCoordinationRequest<'a>,
    pub maximum_tickets_per_cohort: usize,
}

/// The bounded outcome of a rolling queue-grid solve. `NoPlan` identifies the
/// exact formation cohort that could not be added without reopening prior
/// accepted trajectories; it does not imply that no physical solution exists.
#[derive(Debug, Clone, PartialEq)]
pub enum QueueGridRollingOutcome {
    Planned(QueueGridCoordinationPlan),
    NoPlan {
        cohort_tickets: Vec<u64>,
    },
    Unresolved {
        cohort_tickets: Vec<u64>,
        reason: QueueGridUnresolvedReason,
    },
}

/// The explicit finite resource that prevented a rolling solve from reaching
/// either a plan or a bounded no-plan result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueGridUnresolvedReason {
    LowLevelSearchBoundExceeded {
        agent_id: String,
        target_index: usize,
        maximum_expansions: u64,
    },
    ConflictRepairBoundExceeded {
        maximum_conflict_tree_nodes: u64,
    },
}

/// An explicit, uncalibrated assumption used to construct a provisional FIFO
/// service schedule. `first_departure_at_s` is the earliest possible first
/// service completion; `headway_s` is the assumed interval between later
/// service completions while tickets are active.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueueGridServiceAssumption {
    pub first_departure_at_s: f64,
    pub headway_s: f64,
}

/// The instant at which a preallocated queue-grid ticket participates in the
/// physical FIFO rank. An unactivated ticket has no slot target yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueueGridTicketActivation {
    pub ticket: u64,
    pub at_s: f64,
}

/// A FIFO service departure. The ticket must be the active queue head at the
/// declared instant; callers may not use this type to reorder service.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueueGridServiceDeparture {
    pub ticket: u64,
    pub at_s: f64,
}

/// One interval during which an active ticket targets one authored queue-grid
/// slot rank. Consecutive windows encode the forward rank shifts that occur
/// when earlier FIFO tickets take service.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueueGridSlotWindow {
    pub ticket: u64,
    pub starts_at_s: f64,
    pub ends_at_s: f64,
    pub slot_rank: u32,
}

/// Inputs rejected before conflict detection.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoordinationError {
    #[error("trajectory agent id must not be empty")]
    EmptyAgentId,
    #[error("trajectory `{agent_id}` repeats an agent id")]
    DuplicateAgentId { agent_id: String },
    #[error("trajectory `{agent_id}` segment {segment_index} has an empty surface id")]
    EmptySurface {
        agent_id: String,
        segment_index: usize,
    },
    #[error("trajectory `{agent_id}` segment {segment_index} has non-finite values")]
    NonFiniteSegment {
        agent_id: String,
        segment_index: usize,
    },
    #[error("trajectory `{agent_id}` segment {segment_index} has a non-positive duration")]
    NonPositiveDuration {
        agent_id: String,
        segment_index: usize,
    },
    #[error("trajectory `{agent_id}` segment {segment_index} has a negative radius")]
    NegativeRadius {
        agent_id: String,
        segment_index: usize,
    },
    #[error("clearance epsilon must be finite and non-negative")]
    InvalidClearanceEpsilon,
    #[error("roadmap node {node_index} has an empty surface id")]
    EmptyRoadmapSurface { node_index: usize },
    #[error("roadmap node {node_index} has non-finite coordinates")]
    NonFiniteRoadmapNode { node_index: usize },
    #[error("roadmap edge {edge_index} references an unknown node")]
    UnknownRoadmapNode { edge_index: usize },
    #[error("roadmap edge {edge_index} is a self-loop")]
    RoadmapSelfLoop { edge_index: usize },
    #[error("roadmap edge {edge_index} crosses surfaces")]
    RoadmapCrossSurfaceEdge { edge_index: usize },
    #[error("roadmap lattice spacing must be finite and positive")]
    InvalidRoadmapSpacing,
    #[error("roadmap lattice clearance radius must be finite and non-negative")]
    InvalidRoadmapClearanceRadius,
    #[error("roadmap lattice needs at least one node")]
    ZeroRoadmapNodeBound,
    #[error("roadmap lattice has no statically clear nodes")]
    EmptyRoadmapLattice,
    #[error("roadmap lattice exceeded its {maximum_nodes}-node bound")]
    RoadmapNodeBoundExceeded { maximum_nodes: usize },
    #[error("roadmap anchor {anchor_index} lacks static clearance")]
    UnsafeRoadmapAnchor { anchor_index: usize },
    #[error("plan request references an unknown {role} node {node_index}")]
    UnknownPlanNode {
        role: &'static str,
        node_index: usize,
    },
    #[error("plan request has non-finite values")]
    NonFinitePlanRequest,
    #[error("plan request has a negative radius")]
    NegativePlanRadius,
    #[error("plan request reserve horizon precedes its earliest start")]
    InvalidPlanHorizon,
    #[error("plan request requires a positive speed and timestep")]
    NonPositivePlanMotion,
    #[error("plan request needs at least one search expansion")]
    ZeroSearchBound,
    #[error("multi-stage request needs at least one target")]
    EmptyMultiStageTargets,
    #[error("multi-stage target {target_index} references an unknown node {node_index}")]
    UnknownMultiStageTargetNode {
        target_index: usize,
        node_index: usize,
    },
    #[error("multi-stage target {target_index} has an invalid time window")]
    InvalidMultiStageTargetWindow { target_index: usize },
    #[error("multi-stage target {target_index} does not begin at the prior handoff")]
    NonContiguousMultiStageTarget { target_index: usize },
    #[error("plan request agent `{agent_id}` is also present in occupied trajectories")]
    SelfOccupiedTrajectory { agent_id: String },
    #[error("time-expanded roadmap search exceeded its {maximum_expansions}-state bound")]
    SearchBoundExceeded { maximum_expansions: u64 },
    #[error(
        "multi-stage plan for `{agent_id}` target {target_index} exceeded its {maximum_expansions}-state bound"
    )]
    MultiStageSearchBoundExceeded {
        agent_id: String,
        target_index: usize,
        maximum_expansions: u64,
    },
    #[error("coordination request must include at least one agent")]
    EmptyCoordinationRequest,
    #[error("conflict-repair request needs at least one conflict-tree node")]
    ZeroConflictTreeBound,
    #[error("conflict-repair tree exceeded its {maximum_conflict_tree_nodes}-node bound")]
    ConflictRepairBoundExceeded { maximum_conflict_tree_nodes: u64 },
    #[error("occupied trajectories already contain a timed-disc conflict")]
    OccupiedTrajectoryConflict,
    #[error("queue-grid slot count must be positive")]
    ZeroQueueGridSlots,
    #[error("queue-grid slot count exceeds the supported u32 rank range")]
    QueueGridSlotCountOutOfRange,
    #[error("queue-grid rolling coordination needs at least one ticket per cohort")]
    ZeroQueueGridCohortSize,
    #[error("queue-grid timeline horizon must be finite and non-negative")]
    InvalidQueueGridHorizon,
    #[error(
        "queue-grid service assumption needs a finite non-negative first departure and a finite positive headway"
    )]
    InvalidQueueGridServiceAssumption,
    #[error("queue-grid ticket {ticket} has an invalid activation time")]
    InvalidQueueGridActivation { ticket: u64 },
    #[error("queue-grid ticket {ticket} is activated more than once")]
    DuplicateQueueGridTicket { ticket: u64 },
    #[error("queue-grid departure for ticket {ticket} has an invalid time")]
    InvalidQueueGridDepartureTime { ticket: u64 },
    #[error("queue-grid ticket {ticket} departs more than once")]
    DuplicateQueueGridDeparture { ticket: u64 },
    #[error("queue-grid departure references unknown ticket {ticket}")]
    UnknownQueueGridDeparture { ticket: u64 },
    #[error("queue-grid departure for ticket {ticket} violates active FIFO order")]
    NonFifoQueueGridDeparture { ticket: u64 },
    #[error("queue-grid active tickets exceed the {slot_count}-slot geometry")]
    QueueGridSlotCapacityExceeded { slot_count: u32 },
    #[error("queue-grid ticket {ticket} has no active slot windows")]
    MissingQueueGridTicketWindows { ticket: u64 },
    #[error("queue-grid ticket {ticket} references unbound slot rank {slot_rank}")]
    UnboundQueueGridSlotRank { ticket: u64, slot_rank: u32 },
}

impl CoordinationRoadmap {
    /// Validate and build one undirected static roadmap. Edges are not tested
    /// against obstacles here because obstacle expansion is radius-dependent;
    /// the runtime integration layer must author only statically clear edges.
    pub fn new(
        nodes: Vec<CoordinationRoadmapNode>,
        edges: &[CoordinationRoadmapEdge],
    ) -> Result<Self, CoordinationError> {
        for (node_index, node) in nodes.iter().enumerate() {
            if node.surface.is_empty() {
                return Err(CoordinationError::EmptyRoadmapSurface { node_index });
            }
            if !point_is_finite(node.position) {
                return Err(CoordinationError::NonFiniteRoadmapNode { node_index });
            }
        }
        let mut adjacency = vec![Vec::new(); nodes.len()];
        for (edge_index, edge) in edges.iter().enumerate() {
            let Some(first) = nodes.get(edge.first_node) else {
                return Err(CoordinationError::UnknownRoadmapNode { edge_index });
            };
            let Some(second) = nodes.get(edge.second_node) else {
                return Err(CoordinationError::UnknownRoadmapNode { edge_index });
            };
            if edge.first_node == edge.second_node {
                return Err(CoordinationError::RoadmapSelfLoop { edge_index });
            }
            if first.surface != second.surface {
                return Err(CoordinationError::RoadmapCrossSurfaceEdge { edge_index });
            }
            adjacency[edge.first_node].push(edge.second_node);
            adjacency[edge.second_node].push(edge.first_node);
        }
        for neighbours in &mut adjacency {
            neighbours.sort_unstable();
            neighbours.dedup();
        }
        Ok(Self { nodes, adjacency })
    }

    /// Build a bounded square-lattice roadmap for one statically cleared
    /// surface. `anchors` are retained as exact nodes, so callers can map
    /// released positions and authored queue slots without snapping their
    /// coordinates. The returned lattice is deliberately conservative: both
    /// nodes and edges must clear the supplied radius-expanded obstacles and
    /// surface boundary.
    pub fn lattice(
        surface: &Surface,
        obstacles: &[Obstacle],
        clearance_radius_m: f64,
        spacing_m: f64,
        maximum_nodes: usize,
        anchors: &[Point3],
    ) -> Result<CoordinationLatticeRoadmap, CoordinationError> {
        if !spacing_m.is_finite() || spacing_m <= 0.0 {
            return Err(CoordinationError::InvalidRoadmapSpacing);
        }
        if !clearance_radius_m.is_finite() || clearance_radius_m < 0.0 {
            return Err(CoordinationError::InvalidRoadmapClearanceRadius);
        }
        if maximum_nodes == 0 {
            return Err(CoordinationError::ZeroRoadmapNodeBound);
        }
        let mut nodes = Vec::new();
        let horizontal_limits = (
            surface.origin.x_m + clearance_radius_m,
            surface.origin.x_m + surface.width_m - clearance_radius_m,
        );
        let vertical_limits = (
            surface.origin.y_m + clearance_radius_m,
            surface.origin.y_m + surface.depth_m - clearance_radius_m,
        );
        if horizontal_limits.0 <= horizontal_limits.1 && vertical_limits.0 <= vertical_limits.1 {
            let mut row = 0_u64;
            loop {
                #[allow(clippy::cast_precision_loss)]
                let y_m = spacing_m.mul_add(row as f64, vertical_limits.0);
                if y_m > vertical_limits.1 + NAVIGATION_CLEARANCE_EPSILON_M {
                    break;
                }
                let mut column = 0_u64;
                loop {
                    #[allow(clippy::cast_precision_loss)]
                    let x_m = spacing_m.mul_add(column as f64, horizontal_limits.0);
                    if x_m > horizontal_limits.1 + NAVIGATION_CLEARANCE_EPSILON_M {
                        break;
                    }
                    let point = Point3 {
                        x_m: x_m.min(horizontal_limits.1),
                        y_m: y_m.min(vertical_limits.1),
                        z_m: surface.origin.z_m,
                    };
                    if point_has_static_clearance(point, surface, obstacles, clearance_radius_m) {
                        push_roadmap_node(&mut nodes, surface, point, maximum_nodes)?;
                    }
                    let Some(next_column) = column.checked_add(1) else {
                        return Err(CoordinationError::InvalidRoadmapSpacing);
                    };
                    column = next_column;
                }
                let Some(next_row) = row.checked_add(1) else {
                    return Err(CoordinationError::InvalidRoadmapSpacing);
                };
                row = next_row;
            }
        }
        let mut anchor_nodes = Vec::with_capacity(anchors.len());
        for (anchor_index, anchor) in anchors.iter().copied().enumerate() {
            if !point_has_static_clearance(anchor, surface, obstacles, clearance_radius_m) {
                return Err(CoordinationError::UnsafeRoadmapAnchor { anchor_index });
            }
            let existing = nodes.iter().position(|node| {
                point_xy_distance(node.position, anchor) <= NAVIGATION_CLEARANCE_EPSILON_M
            });
            let node_index = if let Some(node_index) = existing {
                node_index
            } else {
                let node_index = nodes.len();
                push_roadmap_node(&mut nodes, surface, anchor, maximum_nodes)?;
                node_index
            };
            anchor_nodes.push(node_index);
        }
        if nodes.is_empty() {
            return Err(CoordinationError::EmptyRoadmapLattice);
        }
        let mut edges = Vec::new();
        let maximum_transition_m = spacing_m * 1.5;
        for first_node in 0..nodes.len() {
            for second_node in first_node + 1..nodes.len() {
                if point_xy_distance(nodes[first_node].position, nodes[second_node].position)
                    > maximum_transition_m + NAVIGATION_CLEARANCE_EPSILON_M
                {
                    continue;
                }
                if segment_has_static_clearance(
                    nodes[first_node].position,
                    nodes[second_node].position,
                    surface,
                    obstacles,
                    clearance_radius_m,
                ) {
                    edges.push(CoordinationRoadmapEdge {
                        first_node,
                        second_node,
                    });
                }
            }
        }
        Ok(CoordinationLatticeRoadmap {
            roadmap: Self::new(nodes, &edges)?,
            anchor_nodes,
        })
    }

    #[must_use]
    pub fn nodes(&self) -> &[CoordinationRoadmapNode] {
        &self.nodes
    }

    /// Search this finite roadmap in timestep increments. Every action uses an
    /// analytic continuous-time disc test against the occupied trajectories;
    /// the time lattice bounds the search, not the collision calculation.
    ///
    /// `Ok(None)` means no plan exists within the roadmap and requested time
    /// horizon. A `SearchBoundExceeded` error means that result is unknown:
    /// callers must not conflate it with infeasibility.
    pub fn plan_time_expanded(
        &self,
        request: &TimeExpandedPlanRequest<'_>,
    ) -> Result<Option<TimeExpandedPlan>, CoordinationError> {
        validate_plan_request(self, request)?;
        let occupied_index =
            TimedDiscSegmentIndex::from_trajectories(request.occupied_trajectories);
        self.plan_time_expanded_indexed(request, &occupied_index)
    }

    fn plan_time_expanded_indexed(
        &self,
        request: &TimeExpandedPlanRequest<'_>,
        occupied_index: &TimedDiscSegmentIndex<'_>,
    ) -> Result<Option<TimeExpandedPlan>, CoordinationError> {
        if let Some(plan) = static_stage_plan(self, request, occupied_index)? {
            return Ok(Some(plan));
        }
        if let Some(plan) = reservation_aware_stage_plan(self, request, occupied_index)? {
            return Ok(Some(plan));
        }
        self.plan_time_expanded_lattice(request, occupied_index)
    }

    /// Fall back to a full time-expanded search only when both the shortest
    /// static route and an earliest-arrival reservation-aware route fail. In
    /// an unoccupied open surface, enumerating every node at every possible
    /// wait time is needless and can exhaust the bounded search before a
    /// spatial route is found.
    fn plan_time_expanded_lattice(
        &self,
        request: &TimeExpandedPlanRequest<'_>,
        occupied_index: &TimedDiscSegmentIndex<'_>,
    ) -> Result<Option<TimeExpandedPlan>, CoordinationError> {
        let horizon_steps = duration_to_horizon_steps(
            request.reserve_until_s - request.earliest_start_s,
            request.timestep_s,
        )?;
        let start = TimeExpandedState {
            node: request.start_node,
            step: 0,
        };
        let mut frontier = BinaryHeap::new();
        frontier.push((
            Reverse(search_priority(self, request, start)?),
            Reverse(0_u64),
            Reverse(request.start_node),
            start,
        ));
        let mut predecessors: HashMap<TimeExpandedState, TimeExpandedState> = HashMap::new();
        let mut discovered = HashSet::new();
        discovered.insert(start);
        let mut explored_states = 0_u64;

        while let Some((Reverse(_), Reverse(_), Reverse(_), state)) = frontier.pop() {
            explored_states = explored_states.saturating_add(1);
            if explored_states > request.maximum_expansions {
                return Err(CoordinationError::SearchBoundExceeded {
                    maximum_expansions: request.maximum_expansions,
                });
            }
            if state.node == request.goal_node
                && final_wait_is_clear(self, request, state, occupied_index)
            {
                return Ok(Some(TimeExpandedPlan {
                    trajectory: reconstruct_trajectory(self, request, state, &predecessors),
                    explored_states,
                }));
            }
            let Some(next_wait_step) = state.step.checked_add(1) else {
                continue;
            };
            if next_wait_step <= horizon_steps {
                let next = TimeExpandedState {
                    node: state.node,
                    step: next_wait_step,
                };
                if !discovered.contains(&next)
                    && action_is_clear(self, request, state, next, occupied_index)
                {
                    predecessors.insert(next, state);
                    discovered.insert(next);
                    frontier.push((
                        Reverse(search_priority(self, request, next)?),
                        Reverse(next.step),
                        Reverse(next.node),
                        next,
                    ));
                }
            }
            for &neighbour in &self.adjacency[state.node] {
                let movement_steps = movement_steps(self, request, state.node, neighbour)?;
                let Some(next_step) = state.step.checked_add(movement_steps) else {
                    continue;
                };
                if next_step > horizon_steps {
                    continue;
                }
                let next = TimeExpandedState {
                    node: neighbour,
                    step: next_step,
                };
                if discovered.contains(&next)
                    || !action_is_clear(self, request, state, next, occupied_index)
                {
                    continue;
                }
                predecessors.insert(next, state);
                discovered.insert(next);
                frontier.push((
                    Reverse(search_priority(self, request, next)?),
                    Reverse(next.step),
                    Reverse(next.node),
                    next,
                ));
            }
        }
        Ok(None)
    }

    /// Plan a continuous sequence of queue-grid rank windows for one agent.
    ///
    /// The first window may begin after `earliest_start_s`; the returned
    /// trajectory holds the released position until that activation time. Each
    /// later window must begin exactly when its predecessor ends. A stage may
    /// move only during its own active window and holds its target through the
    /// handoff. Consequently, `Ok(None)` means at least one required rank
    /// window is unavailable on this bounded roadmap; it is not permission to
    /// retain the preceding static slot target.
    pub fn plan_multi_stage(
        &self,
        request: &MultiStagePlanRequest<'_>,
    ) -> Result<Option<MultiStagePlan>, CoordinationError> {
        validate_multi_stage_plan_request(self, request)?;
        let occupied_index =
            TimedDiscSegmentIndex::from_trajectories(request.occupied_trajectories);
        let first_target = request
            .targets
            .first()
            .expect("validated multi-stage request has a target");
        let mut current_node = request.start_node;
        let mut trajectory = TimedDiscTrajectory {
            agent_id: request.agent_id.clone(),
            segments: Vec::new(),
        };
        let mut explored_states = 0_u64;

        if request.earliest_start_s < first_target.starts_at_s {
            let node = &self.nodes[current_node];
            let wait = TimedDiscSegment {
                surface: node.surface.clone(),
                starts_at_s: request.earliest_start_s,
                ends_at_s: first_target.starts_at_s,
                start: node.position,
                end: node.position,
                radius_m: request.radius_m,
            };
            if !occupied_index.is_clear(&wait, request.clearance_epsilon_m) {
                return Ok(None);
            }
            trajectory.segments.push(wait);
        }

        for (target_index, target) in request.targets.iter().enumerate() {
            let stage_request = TimeExpandedPlanRequest {
                agent_id: request.agent_id.clone(),
                start_node: current_node,
                goal_node: target.node,
                radius_m: request.radius_m,
                earliest_start_s: target.starts_at_s,
                reserve_until_s: target.ends_at_s,
                speed_mps: request.speed_mps,
                timestep_s: request.timestep_s,
                maximum_expansions: request.maximum_expansions_per_stage,
                clearance_epsilon_m: request.clearance_epsilon_m,
                occupied_trajectories: request.occupied_trajectories,
            };
            let stage_plan =
                if let Some(plan) = direct_stage_plan(self, &stage_request, &occupied_index)? {
                    plan
                } else {
                    let Some(plan) = self
                        .plan_time_expanded_indexed(&stage_request, &occupied_index)
                        .map_err(|error| match error {
                            CoordinationError::SearchBoundExceeded { maximum_expansions } => {
                                CoordinationError::MultiStageSearchBoundExceeded {
                                    agent_id: request.agent_id.clone(),
                                    target_index,
                                    maximum_expansions,
                                }
                            }
                            other => other,
                        })?
                    else {
                        return Ok(None);
                    };
                    plan
                };
            explored_states = explored_states.saturating_add(stage_plan.explored_states);
            trajectory.segments.extend(stage_plan.trajectory.segments);
            current_node = target.node;
        }

        let mut all_trajectories = request.occupied_trajectories.to_vec();
        all_trajectories.push(trajectory.clone());
        let final_conflicts = timed_disc_conflicts(&all_trajectories, request.clearance_epsilon_m)?;
        if !final_conflicts.is_empty() {
            return Ok(None);
        }

        Ok(Some(MultiStagePlan {
            trajectory,
            explored_states,
        }))
    }

    /// Resolve conflicts by branching deterministically on either participant
    /// in the earliest remaining conflict. A branch replans that participant
    /// against the currently declared paths of all other agents, then the
    /// exact conflict kernel evaluates the result again.
    ///
    /// This bounded repair tree is sound for a returned plan because the final
    /// candidate has no timed-disc conflicts. It is not complete: a finite
    /// roadmap, finite horizon, and finite tree bound can all exclude a valid
    /// physical solution. `Ok(None)` means an initial individual route is
    /// unavailable; a tree-bound error leaves the result unknown.
    #[allow(clippy::too_many_lines)]
    pub fn repair_conflicts(
        &self,
        request: &ConflictRepairRequest<'_>,
    ) -> Result<Option<ConflictRepairPlan>, CoordinationError> {
        validate_conflict_repair_request(request)?;
        let mut low_level_explored_states = 0_u64;
        let mut initial_trajectories = Vec::with_capacity(request.agents.len());
        for agent in &request.agents {
            let Some(plan) =
                plan_coordination_agent(self, request, agent, request.occupied_trajectories)?
            else {
                return Ok(None);
            };
            low_level_explored_states =
                low_level_explored_states.saturating_add(plan.explored_states);
            initial_trajectories.push(plan.trajectory);
        }
        if !timed_disc_conflicts(request.occupied_trajectories, request.clearance_epsilon_m)?
            .is_empty()
        {
            return Err(CoordinationError::OccupiedTrajectoryConflict);
        }
        let mut all_initial_trajectories = request.occupied_trajectories.to_vec();
        all_initial_trajectories.extend(initial_trajectories.iter().cloned());
        let initial_conflicts =
            timed_disc_conflicts(&all_initial_trajectories, request.clearance_epsilon_m)?;
        if initial_conflicts.is_empty() {
            return Ok(Some(ConflictRepairPlan {
                trajectories: initial_trajectories,
                explored_conflict_tree_nodes: 1,
                low_level_explored_states,
            }));
        }

        if let Some(sequential_plan) = plan_agents_sequentially(self, request)? {
            return Ok(Some(sequential_plan));
        }

        let mut nodes = vec![ConflictRepairNode {
            trajectories: initial_trajectories,
            conflicts: initial_conflicts,
            constraints: vec![Vec::new(); request.agents.len()],
        }];
        let mut frontier = BinaryHeap::new();
        frontier.push((Reverse(nodes[0].conflicts.len()), Reverse(0_usize)));
        // The root is the first conflict-tree node. Every viable replan below
        // creates one child, including a child that immediately resolves all
        // conflicts.
        let mut conflict_tree_nodes = 1_u64;

        while let Some((Reverse(_), Reverse(node_index))) = frontier.pop() {
            let node = nodes[node_index].clone();
            let conflict = node
                .conflicts
                .first()
                .expect("only conflicting nodes enter the repair frontier");
            let mut branch_agent_ids = [
                conflict.first_agent_id.as_str(),
                conflict.second_agent_id.as_str(),
            ];
            branch_agent_ids.sort_unstable();
            for agent_id in branch_agent_ids {
                let Some(agent_index) = request
                    .agents
                    .iter()
                    .position(|agent| agent.agent_id == agent_id)
                else {
                    // A rolling cohort cannot revise an earlier accepted
                    // trajectory. If a candidate conflicts with one, this
                    // finite decomposition has no plan; it must not panic or
                    // pretend that the immutable participant can be repaired.
                    return Ok(None);
                };
                let (other_agent_id, other_segment_index) = if agent_id == conflict.first_agent_id {
                    (
                        conflict.second_agent_id.as_str(),
                        conflict.second_segment_index,
                    )
                } else {
                    (
                        conflict.first_agent_id.as_str(),
                        conflict.first_segment_index,
                    )
                };
                let Some(other_agent_index) = request
                    .agents
                    .iter()
                    .position(|agent| agent.agent_id == other_agent_id)
                else {
                    return Ok(None);
                };
                let Some(forbidden_segment) = node.trajectories[other_agent_index]
                    .segments
                    .get(other_segment_index)
                    .cloned()
                else {
                    return Ok(None);
                };
                let mut constraints = node.constraints.clone();
                constraints[agent_index].push(forbidden_segment);
                let Some(replanned) = plan_coordination_agent_with_constraints(
                    self,
                    request,
                    &request.agents[agent_index],
                    request.occupied_trajectories,
                    &constraints[agent_index],
                )?
                else {
                    continue;
                };
                low_level_explored_states =
                    low_level_explored_states.saturating_add(replanned.explored_states);
                if replanned.trajectory == node.trajectories[agent_index] {
                    continue;
                }
                conflict_tree_nodes = conflict_tree_nodes.saturating_add(1);
                if conflict_tree_nodes > request.maximum_conflict_tree_nodes {
                    return Err(CoordinationError::ConflictRepairBoundExceeded {
                        maximum_conflict_tree_nodes: request.maximum_conflict_tree_nodes,
                    });
                }
                let mut trajectories = node.trajectories.clone();
                trajectories[agent_index] = replanned.trajectory;
                let mut all_trajectories = request.occupied_trajectories.to_vec();
                all_trajectories.extend(trajectories.iter().cloned());
                let conflicts =
                    timed_disc_conflicts(&all_trajectories, request.clearance_epsilon_m)?;
                if conflicts.is_empty() {
                    return Ok(Some(ConflictRepairPlan {
                        trajectories,
                        explored_conflict_tree_nodes: conflict_tree_nodes,
                        low_level_explored_states,
                    }));
                }
                let new_node_index = nodes.len();
                nodes.push(ConflictRepairNode {
                    trajectories,
                    conflicts,
                    constraints,
                });
                frontier.push((
                    Reverse(nodes[new_node_index].conflicts.len()),
                    Reverse(new_node_index),
                ));
            }
        }
        Ok(None)
    }
}

/// Build FIFO rank windows, bind them to the exact authored slot nodes, and
/// solve the resulting timed tasks through the bounded repair tree.
///
/// `Ok(None)` means that this finite roadmap and time horizon have no plan for
/// at least one ticket. A bound-exhausted error remains distinct from that
/// result. The returned trajectories are reference-disc clear according to
/// `timed_disc_conflicts`, but are not a physical safety certificate.
pub fn plan_queue_grid(
    request: &QueueGridCoordinationRequest<'_>,
) -> Result<Option<QueueGridCoordinationPlan>, CoordinationError> {
    let slot_windows = queue_grid_slot_windows_for_request(request)?;
    let agents = queue_grid_agents(&request.tickets, &slot_windows, request.slot_nodes)?;
    let repair_request = ConflictRepairRequest {
        agents,
        occupied_trajectories: request.occupied_trajectories,
        timestep_s: request.timestep_s,
        maximum_low_level_expansions: request.maximum_low_level_expansions,
        maximum_conflict_tree_nodes: request.maximum_conflict_tree_nodes,
        clearance_epsilon_m: request.clearance_epsilon_m,
        roadmap: request.roadmap,
    };
    request
        .roadmap
        .repair_conflicts(&repair_request)
        .map(|repair_plan| {
            repair_plan.map(|repair_plan| QueueGridCoordinationPlan {
                slot_windows,
                repair_plan,
            })
        })
}

/// Solve the same timed queue-grid tasks in deterministic back-to-front
/// formation cohorts.
///
/// Each cohort is fully checked against all trajectories accepted before it;
/// by induction, the returned trajectories are reference-disc clear relative
/// to one another and to the request's occupied trajectories. The method does
/// not perform an additional monolithic conflict scan, because that would
/// defeat the bounded rolling execution model for large queues.
pub fn plan_queue_grid_rolling(
    request: &QueueGridRollingCoordinationRequest<'_>,
) -> Result<Option<QueueGridCoordinationPlan>, CoordinationError> {
    assess_queue_grid_rolling(request).map(|outcome| match outcome {
        QueueGridRollingOutcome::Planned(plan) => Some(plan),
        QueueGridRollingOutcome::NoPlan { .. } | QueueGridRollingOutcome::Unresolved { .. } => None,
    })
}

/// Run a rolling queue-grid solve while retaining the first cohort that lacks
/// a plan or exhausts an explicit planning bound. Use
/// [`plan_queue_grid_rolling`] when only the ordinary `Some`/`None` result is
/// needed.
pub fn assess_queue_grid_rolling(
    request: &QueueGridRollingCoordinationRequest<'_>,
) -> Result<QueueGridRollingOutcome, CoordinationError> {
    if request.maximum_tickets_per_cohort == 0 {
        return Err(CoordinationError::ZeroQueueGridCohortSize);
    }
    let slot_windows = queue_grid_slot_windows_for_request(&request.queue)?;
    let mut tickets = request.queue.tickets.clone();
    tickets.sort_by_key(|ticket| Reverse(ticket.ticket));
    let agents = queue_grid_agents(&tickets, &slot_windows, request.queue.slot_nodes)?;
    let mut occupied_trajectories = request.queue.occupied_trajectories.to_vec();
    let mut trajectories = Vec::with_capacity(agents.len());
    let mut explored_conflict_tree_nodes = 0_u64;
    let mut low_level_explored_states = 0_u64;
    for cohort in agents.chunks(request.maximum_tickets_per_cohort) {
        let cohort_tickets = cohort
            .iter()
            .filter_map(|agent| {
                tickets
                    .iter()
                    .find(|ticket| ticket.agent_id == agent.agent_id)
                    .map(|ticket| ticket.ticket)
            })
            .collect::<Vec<_>>();
        let repair_request = ConflictRepairRequest {
            agents: cohort.to_vec(),
            occupied_trajectories: &occupied_trajectories,
            timestep_s: request.queue.timestep_s,
            maximum_low_level_expansions: request.queue.maximum_low_level_expansions,
            maximum_conflict_tree_nodes: request.queue.maximum_conflict_tree_nodes,
            clearance_epsilon_m: request.queue.clearance_epsilon_m,
            roadmap: request.queue.roadmap,
        };
        let repair_plan = match request.queue.roadmap.repair_conflicts(&repair_request) {
            Ok(Some(plan)) => plan,
            Ok(None) => {
                return Ok(QueueGridRollingOutcome::NoPlan { cohort_tickets });
            }
            Err(CoordinationError::MultiStageSearchBoundExceeded {
                agent_id,
                target_index,
                maximum_expansions,
            }) => {
                return Ok(QueueGridRollingOutcome::Unresolved {
                    cohort_tickets,
                    reason: QueueGridUnresolvedReason::LowLevelSearchBoundExceeded {
                        agent_id,
                        target_index,
                        maximum_expansions,
                    },
                });
            }
            Err(CoordinationError::ConflictRepairBoundExceeded {
                maximum_conflict_tree_nodes,
            }) => {
                return Ok(QueueGridRollingOutcome::Unresolved {
                    cohort_tickets,
                    reason: QueueGridUnresolvedReason::ConflictRepairBoundExceeded {
                        maximum_conflict_tree_nodes,
                    },
                });
            }
            Err(error) => return Err(error),
        };
        explored_conflict_tree_nodes =
            explored_conflict_tree_nodes.saturating_add(repair_plan.explored_conflict_tree_nodes);
        low_level_explored_states =
            low_level_explored_states.saturating_add(repair_plan.low_level_explored_states);
        occupied_trajectories.extend(repair_plan.trajectories.iter().cloned());
        trajectories.extend(repair_plan.trajectories);
    }
    Ok(QueueGridRollingOutcome::Planned(
        QueueGridCoordinationPlan {
            slot_windows,
            repair_plan: ConflictRepairPlan {
                trajectories,
                explored_conflict_tree_nodes,
                low_level_explored_states,
            },
        },
    ))
}

fn queue_grid_slot_windows_for_request(
    request: &QueueGridCoordinationRequest<'_>,
) -> Result<Vec<QueueGridSlotWindow>, CoordinationError> {
    let slot_count = u32::try_from(request.slot_nodes.len())
        .map_err(|_| CoordinationError::QueueGridSlotCountOutOfRange)?;
    let activations = request
        .tickets
        .iter()
        .map(|ticket| QueueGridTicketActivation {
            ticket: ticket.ticket,
            at_s: ticket.activation_at_s,
        })
        .collect::<Vec<_>>();
    queue_grid_slot_windows(
        slot_count,
        &activations,
        &request.departures,
        request.horizon_s,
    )
}

fn queue_grid_agents(
    tickets: &[QueueGridTicketRequest],
    slot_windows: &[QueueGridSlotWindow],
    slot_nodes: &[usize],
) -> Result<Vec<CoordinationAgentRequest>, CoordinationError> {
    tickets
        .iter()
        .map(|ticket| {
            Ok(CoordinationAgentRequest {
                agent_id: ticket.agent_id.clone(),
                start_node: ticket.start_node,
                radius_m: ticket.radius_m,
                earliest_start_s: ticket.activation_at_s,
                speed_mps: ticket.speed_mps,
                task: CoordinationAgentTask::TimedTargets {
                    targets: queue_grid_timed_targets(ticket.ticket, slot_windows, slot_nodes)?,
                },
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct TimeExpandedState {
    node: usize,
    step: u64,
}

#[derive(Debug, Clone)]
struct ConflictRepairNode {
    trajectories: Vec<TimedDiscTrajectory>,
    conflicts: Vec<TimedDiscConflict>,
    constraints: Vec<Vec<TimedDiscSegment>>,
}

#[derive(Debug, Clone)]
struct CoordinationAgentPlan {
    trajectory: TimedDiscTrajectory,
    explored_states: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticRoadmapRoute {
    nodes: Vec<usize>,
    explored_nodes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SafeStepInterval {
    starts_at_step: u64,
    ends_at_step: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct SafeIntervalState {
    node: usize,
    interval_index: usize,
    arrival_step: u64,
}

#[derive(Debug, Clone, Copy)]
struct SafeIntervalPredecessor {
    state: SafeIntervalState,
    departure_step: u64,
}

/// A read-only temporal index for one low-level plan. It preserves the exact
/// segment kernel while avoiding scans of reservations that begin after a
/// candidate action ends or live on another surface.
#[derive(Debug)]
struct TimedDiscSegmentIndex<'a> {
    by_surface: HashMap<&'a str, Vec<&'a TimedDiscSegment>>,
}

impl<'a> TimedDiscSegmentIndex<'a> {
    fn from_trajectories(trajectories: &'a [TimedDiscTrajectory]) -> Self {
        let mut by_surface: HashMap<&str, Vec<&TimedDiscSegment>> = HashMap::new();
        for trajectory in trajectories {
            for segment in &trajectory.segments {
                by_surface
                    .entry(segment.surface.as_str())
                    .or_default()
                    .push(segment);
            }
        }
        for segments in by_surface.values_mut() {
            segments.sort_by(|left, right| left.starts_at_s.total_cmp(&right.starts_at_s));
        }
        Self { by_surface }
    }

    fn is_clear(&self, candidate: &TimedDiscSegment, clearance_epsilon_m: f64) -> bool {
        let Some(segments) = self.by_surface.get(candidate.surface.as_str()) else {
            return true;
        };
        let ending_index = segments
            .partition_point(|segment| segment.starts_at_s.total_cmp(&candidate.ends_at_s).is_lt());
        segments[..ending_index].iter().all(|occupied| {
            !segments_may_overlap(candidate, occupied, clearance_epsilon_m)
                || segment_unsafe_interval(candidate, occupied, clearance_epsilon_m).is_none()
        })
    }
}

/// Find every continuous-time reference-disc conflict, sorted by the start of
/// its unsafe interval and then by stable agent/segment identity.
///
/// A segment pair on different surfaces is intentionally ignored. A caller
/// that needs connector or lift-transit geometry must represent it explicitly
/// in another model; this on-surface kernel must not silently invent one.
pub fn timed_disc_conflicts(
    trajectories: &[TimedDiscTrajectory],
    clearance_epsilon_m: f64,
) -> Result<Vec<TimedDiscConflict>, CoordinationError> {
    validate_trajectories(trajectories, clearance_epsilon_m)?;
    let mut conflicts = Vec::new();
    for (first_index, first) in trajectories.iter().enumerate() {
        for second in trajectories.iter().skip(first_index + 1) {
            for (first_segment_index, first_segment) in first.segments.iter().enumerate() {
                for (second_segment_index, second_segment) in second.segments.iter().enumerate() {
                    let Some(conflict) = segment_conflict(
                        first,
                        first_segment_index,
                        first_segment,
                        second,
                        second_segment_index,
                        second_segment,
                        clearance_epsilon_m,
                    ) else {
                        continue;
                    };
                    conflicts.push(conflict);
                }
            }
        }
    }
    conflicts.sort_by(conflict_order);
    Ok(conflicts)
}

/// Return the earliest continuous-time reference-disc conflict, if one
/// exists. This is the deterministic branch point for a future CBS layer.
pub fn first_timed_disc_conflict(
    trajectories: &[TimedDiscTrajectory],
    clearance_epsilon_m: f64,
) -> Result<Option<TimedDiscConflict>, CoordinationError> {
    Ok(timed_disc_conflicts(trajectories, clearance_epsilon_m)?
        .into_iter()
        .next())
}

/// Derive the rank-target timeline implied by ticket activation and FIFO
/// service departure events. This is a queue-state task graph, not a movement
/// plan: a caller maps each `slot_rank` to its authored grid position and
/// plans the transitions between successive windows.
///
/// Events at one instant are processed deterministically: all activations in
/// ticket order occur before all departures in ticket order. That matches the
/// reference runtime's preallocation-before-service phase while preserving the
/// invariant that only the current active head can depart.
pub fn queue_grid_slot_windows(
    slot_count: u32,
    activations: &[QueueGridTicketActivation],
    departures: &[QueueGridServiceDeparture],
    horizon_s: f64,
) -> Result<Vec<QueueGridSlotWindow>, CoordinationError> {
    if slot_count == 0 {
        return Err(CoordinationError::ZeroQueueGridSlots);
    }
    if !horizon_s.is_finite() || horizon_s < 0.0 {
        return Err(CoordinationError::InvalidQueueGridHorizon);
    }
    let mut activation_times = HashMap::new();
    for activation in activations {
        if !activation.at_s.is_finite() || activation.at_s < 0.0 || activation.at_s > horizon_s {
            return Err(CoordinationError::InvalidQueueGridActivation {
                ticket: activation.ticket,
            });
        }
        if activation_times
            .insert(activation.ticket, activation.at_s)
            .is_some()
        {
            return Err(CoordinationError::DuplicateQueueGridTicket {
                ticket: activation.ticket,
            });
        }
    }
    let mut departure_times = HashSet::new();
    for departure in departures {
        if !departure.at_s.is_finite() || departure.at_s < 0.0 || departure.at_s > horizon_s {
            return Err(CoordinationError::InvalidQueueGridDepartureTime {
                ticket: departure.ticket,
            });
        }
        if !activation_times.contains_key(&departure.ticket) {
            return Err(CoordinationError::UnknownQueueGridDeparture {
                ticket: departure.ticket,
            });
        }
        if !departure_times.insert(departure.ticket) {
            return Err(CoordinationError::DuplicateQueueGridDeparture {
                ticket: departure.ticket,
            });
        }
    }
    let mut event_times = activations
        .iter()
        .map(|activation| activation.at_s)
        .chain(departures.iter().map(|departure| departure.at_s))
        .collect::<Vec<_>>();
    event_times.sort_by(f64::total_cmp);
    event_times.dedup_by(|left, right| left.total_cmp(right).is_eq());

    let mut active_tickets = Vec::new();
    let mut open_windows: HashMap<u64, (f64, u32)> = HashMap::new();
    let mut windows = Vec::new();
    for time_s in event_times {
        close_queue_grid_windows(time_s, &mut open_windows, &mut windows);
        let mut activating = activations
            .iter()
            .filter(|activation| activation.at_s.total_cmp(&time_s).is_eq())
            .map(|activation| activation.ticket)
            .collect::<Vec<_>>();
        activating.sort_unstable();
        active_tickets.extend(activating);
        active_tickets.sort_unstable();
        if active_tickets.len() > usize::try_from(slot_count).expect("slot count fits usize") {
            return Err(CoordinationError::QueueGridSlotCapacityExceeded { slot_count });
        }
        let mut departing = departures
            .iter()
            .filter(|departure| departure.at_s.total_cmp(&time_s).is_eq())
            .map(|departure| departure.ticket)
            .collect::<Vec<_>>();
        departing.sort_unstable();
        for ticket in departing {
            if active_tickets.first().copied() != Some(ticket) {
                return Err(CoordinationError::NonFifoQueueGridDeparture { ticket });
            }
            active_tickets.remove(0);
        }
        open_queue_grid_windows(time_s, &active_tickets, &mut open_windows);
    }
    close_queue_grid_windows(horizon_s, &mut open_windows, &mut windows);
    windows.sort_by(|left, right| {
        left.ticket
            .cmp(&right.ticket)
            .then_with(|| left.starts_at_s.total_cmp(&right.starts_at_s))
            .then_with(|| left.slot_rank.cmp(&right.slot_rank))
    });
    let mut merged_windows: Vec<QueueGridSlotWindow> = Vec::with_capacity(windows.len());
    for window in windows {
        if let Some(previous) = merged_windows.last_mut()
            && previous.ticket == window.ticket
            && previous.slot_rank == window.slot_rank
            && previous.ends_at_s.total_cmp(&window.starts_at_s).is_eq()
        {
            previous.ends_at_s = window.ends_at_s;
        } else {
            merged_windows.push(window);
        }
    }
    Ok(merged_windows)
}

/// Construct a provisional FIFO service-departure sequence from explicitly
/// supplied, uncalibrated timing assumptions.
///
/// The sequence honours the active queue at each departure instant. It does
/// not make a service claim: callers must record the assumption and must use
/// [`plan_queue_grid`] to reject a schedule whose target windows cannot be
/// physically reached on the declared roadmap.
pub fn estimate_queue_grid_departures(
    tickets: &[QueueGridTicketRequest],
    assumption: QueueGridServiceAssumption,
    horizon_s: f64,
) -> Result<Vec<QueueGridServiceDeparture>, CoordinationError> {
    if !assumption.first_departure_at_s.is_finite()
        || assumption.first_departure_at_s < 0.0
        || !assumption.headway_s.is_finite()
        || assumption.headway_s <= 0.0
    {
        return Err(CoordinationError::InvalidQueueGridServiceAssumption);
    }
    if !horizon_s.is_finite() || horizon_s < 0.0 {
        return Err(CoordinationError::InvalidQueueGridHorizon);
    }
    let mut activations = Vec::with_capacity(tickets.len());
    let mut seen_tickets = HashSet::new();
    for ticket in tickets {
        if !ticket.activation_at_s.is_finite()
            || ticket.activation_at_s < 0.0
            || ticket.activation_at_s > horizon_s
        {
            return Err(CoordinationError::InvalidQueueGridActivation {
                ticket: ticket.ticket,
            });
        }
        if !seen_tickets.insert(ticket.ticket) {
            return Err(CoordinationError::DuplicateQueueGridTicket {
                ticket: ticket.ticket,
            });
        }
        activations.push(QueueGridTicketActivation {
            ticket: ticket.ticket,
            at_s: ticket.activation_at_s,
        });
    }
    activations.sort_by(|left, right| {
        left.at_s
            .total_cmp(&right.at_s)
            .then_with(|| left.ticket.cmp(&right.ticket))
    });

    let mut departures = Vec::new();
    let mut active_tickets = Vec::new();
    let mut activation_index = 0_usize;
    let mut next_departure_at_s = assumption.first_departure_at_s;
    while activation_index < activations.len() || !active_tickets.is_empty() {
        if active_tickets.is_empty() {
            let activation = activations
                .get(activation_index)
                .expect("unprocessed activation exists when no ticket is active");
            next_departure_at_s = next_departure_at_s.max(activation.at_s);
        }
        while let Some(activation) = activations.get(activation_index)
            && activation.at_s <= next_departure_at_s
        {
            active_tickets.push(activation.ticket);
            activation_index += 1;
        }
        active_tickets.sort_unstable();
        let Some(ticket) = active_tickets.first().copied() else {
            continue;
        };
        if next_departure_at_s > horizon_s {
            break;
        }
        departures.push(QueueGridServiceDeparture {
            ticket,
            at_s: next_departure_at_s,
        });
        active_tickets.remove(0);
        next_departure_at_s += assumption.headway_s;
        if !next_departure_at_s.is_finite() {
            return Err(CoordinationError::InvalidQueueGridServiceAssumption);
        }
    }
    Ok(departures)
}

/// Map one ticket's FIFO rank windows to the statically clear roadmap nodes
/// authored for the matching queue-grid slots. This is deliberately separate
/// from timeline generation: the latter owns FIFO semantics, while this
/// function makes the geometric rank-to-node binding explicit and rejects an
/// incomplete binding instead of selecting a nearby substitute vertex.
pub fn queue_grid_timed_targets(
    ticket: u64,
    windows: &[QueueGridSlotWindow],
    slot_nodes: &[usize],
) -> Result<Vec<TimedRoadmapTarget>, CoordinationError> {
    let mut ticket_windows = windows
        .iter()
        .copied()
        .filter(|window| window.ticket == ticket)
        .collect::<Vec<_>>();
    if ticket_windows.is_empty() {
        return Err(CoordinationError::MissingQueueGridTicketWindows { ticket });
    }
    ticket_windows.sort_by(|left, right| {
        left.starts_at_s
            .total_cmp(&right.starts_at_s)
            .then_with(|| left.ends_at_s.total_cmp(&right.ends_at_s))
            .then_with(|| left.slot_rank.cmp(&right.slot_rank))
    });
    ticket_windows
        .into_iter()
        .map(|window| {
            let node = slot_nodes
                .get(usize::try_from(window.slot_rank).expect("slot rank fits usize"))
                .copied()
                .ok_or(CoordinationError::UnboundQueueGridSlotRank {
                    ticket,
                    slot_rank: window.slot_rank,
                })?;
            Ok(TimedRoadmapTarget {
                node,
                starts_at_s: window.starts_at_s,
                ends_at_s: window.ends_at_s,
            })
        })
        .collect()
}

fn close_queue_grid_windows(
    ends_at_s: f64,
    open_windows: &mut HashMap<u64, (f64, u32)>,
    windows: &mut Vec<QueueGridSlotWindow>,
) {
    for (ticket, (starts_at_s, slot_rank)) in std::mem::take(open_windows) {
        if starts_at_s < ends_at_s {
            windows.push(QueueGridSlotWindow {
                ticket,
                starts_at_s,
                ends_at_s,
                slot_rank,
            });
        }
    }
}

fn open_queue_grid_windows(
    starts_at_s: f64,
    active_tickets: &[u64],
    open_windows: &mut HashMap<u64, (f64, u32)>,
) {
    for (slot_rank, ticket) in active_tickets.iter().copied().enumerate() {
        open_windows.insert(
            ticket,
            (
                starts_at_s,
                u32::try_from(slot_rank).expect("active queue rank fits u32"),
            ),
        );
    }
}

/// The numerical rule used by the runtime's reference-disc audits. Exposing it
/// keeps a caller from choosing a stricter or weaker default by accident.
#[must_use]
pub const fn reference_clearance_epsilon_m() -> f64 {
    NAVIGATION_CLEARANCE_EPSILON_M
}

fn validate_plan_request(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
) -> Result<(), CoordinationError> {
    if request.agent_id.is_empty() {
        return Err(CoordinationError::EmptyAgentId);
    }
    if roadmap.nodes.get(request.start_node).is_none() {
        return Err(CoordinationError::UnknownPlanNode {
            role: "start",
            node_index: request.start_node,
        });
    }
    if roadmap.nodes.get(request.goal_node).is_none() {
        return Err(CoordinationError::UnknownPlanNode {
            role: "goal",
            node_index: request.goal_node,
        });
    }
    if !request.radius_m.is_finite() {
        return Err(CoordinationError::NonFinitePlanRequest);
    }
    if request.radius_m < 0.0 {
        return Err(CoordinationError::NegativePlanRadius);
    }
    if !request.earliest_start_s.is_finite()
        || !request.reserve_until_s.is_finite()
        || !request.speed_mps.is_finite()
        || !request.timestep_s.is_finite()
        || !request.clearance_epsilon_m.is_finite()
    {
        return Err(CoordinationError::NonFinitePlanRequest);
    }
    if request.reserve_until_s < request.earliest_start_s {
        return Err(CoordinationError::InvalidPlanHorizon);
    }
    if request.speed_mps <= 0.0 || request.timestep_s <= 0.0 {
        return Err(CoordinationError::NonPositivePlanMotion);
    }
    if request.clearance_epsilon_m < 0.0 {
        return Err(CoordinationError::InvalidClearanceEpsilon);
    }
    if request.maximum_expansions == 0 {
        return Err(CoordinationError::ZeroSearchBound);
    }
    validate_trajectories(request.occupied_trajectories, request.clearance_epsilon_m)?;
    if request
        .occupied_trajectories
        .iter()
        .any(|trajectory| trajectory.agent_id == request.agent_id)
    {
        return Err(CoordinationError::SelfOccupiedTrajectory {
            agent_id: request.agent_id.clone(),
        });
    }
    Ok(())
}

fn validate_multi_stage_plan_request(
    roadmap: &CoordinationRoadmap,
    request: &MultiStagePlanRequest<'_>,
) -> Result<(), CoordinationError> {
    let Some(first_target) = request.targets.first() else {
        return Err(CoordinationError::EmptyMultiStageTargets);
    };
    if roadmap.nodes.get(request.start_node).is_none() {
        return Err(CoordinationError::UnknownPlanNode {
            role: "start",
            node_index: request.start_node,
        });
    }
    if request.agent_id.is_empty() {
        return Err(CoordinationError::EmptyAgentId);
    }
    if !request.earliest_start_s.is_finite()
        || !request.radius_m.is_finite()
        || !request.speed_mps.is_finite()
        || !request.timestep_s.is_finite()
        || !request.clearance_epsilon_m.is_finite()
    {
        return Err(CoordinationError::NonFinitePlanRequest);
    }
    if request.radius_m < 0.0 {
        return Err(CoordinationError::NegativePlanRadius);
    }
    if request.speed_mps <= 0.0 || request.timestep_s <= 0.0 {
        return Err(CoordinationError::NonPositivePlanMotion);
    }
    if request.clearance_epsilon_m < 0.0 {
        return Err(CoordinationError::InvalidClearanceEpsilon);
    }
    if request.maximum_expansions_per_stage == 0 {
        return Err(CoordinationError::ZeroSearchBound);
    }
    validate_trajectories(request.occupied_trajectories, request.clearance_epsilon_m)?;
    if request
        .occupied_trajectories
        .iter()
        .any(|trajectory| trajectory.agent_id == request.agent_id)
    {
        return Err(CoordinationError::SelfOccupiedTrajectory {
            agent_id: request.agent_id.clone(),
        });
    }
    if first_target.starts_at_s < request.earliest_start_s {
        return Err(CoordinationError::InvalidMultiStageTargetWindow { target_index: 0 });
    }
    let mut previous_ends_at_s = None;
    for (target_index, target) in request.targets.iter().enumerate() {
        if roadmap.nodes.get(target.node).is_none() {
            return Err(CoordinationError::UnknownMultiStageTargetNode {
                target_index,
                node_index: target.node,
            });
        }
        if !target.starts_at_s.is_finite()
            || !target.ends_at_s.is_finite()
            || target.ends_at_s <= target.starts_at_s
        {
            return Err(CoordinationError::InvalidMultiStageTargetWindow { target_index });
        }
        if let Some(previous_ends_at_s) = previous_ends_at_s
            && target.starts_at_s.total_cmp(&previous_ends_at_s).is_ne()
        {
            return Err(CoordinationError::NonContiguousMultiStageTarget { target_index });
        }
        previous_ends_at_s = Some(target.ends_at_s);
    }
    Ok(())
}

fn validate_conflict_repair_request(
    request: &ConflictRepairRequest<'_>,
) -> Result<(), CoordinationError> {
    if request.agents.is_empty() {
        return Err(CoordinationError::EmptyCoordinationRequest);
    }
    if request.maximum_conflict_tree_nodes == 0 {
        return Err(CoordinationError::ZeroConflictTreeBound);
    }
    validate_trajectories(request.occupied_trajectories, request.clearance_epsilon_m)?;
    let mut agent_ids = HashSet::new();
    for agent in &request.agents {
        if !agent_ids.insert(&agent.agent_id) {
            return Err(CoordinationError::DuplicateAgentId {
                agent_id: agent.agent_id.clone(),
            });
        }
        validate_coordination_agent_request(request, agent)?;
    }
    Ok(())
}

fn validate_coordination_agent_request(
    request: &ConflictRepairRequest<'_>,
    agent: &CoordinationAgentRequest,
) -> Result<(), CoordinationError> {
    match &agent.task {
        CoordinationAgentTask::Goal {
            goal_node,
            reserve_until_s,
        } => validate_plan_request(
            request.roadmap,
            &TimeExpandedPlanRequest {
                agent_id: agent.agent_id.clone(),
                start_node: agent.start_node,
                goal_node: *goal_node,
                radius_m: agent.radius_m,
                earliest_start_s: agent.earliest_start_s,
                reserve_until_s: *reserve_until_s,
                speed_mps: agent.speed_mps,
                timestep_s: request.timestep_s,
                maximum_expansions: request.maximum_low_level_expansions,
                clearance_epsilon_m: request.clearance_epsilon_m,
                occupied_trajectories: request.occupied_trajectories,
            },
        ),
        CoordinationAgentTask::TimedTargets { targets } => validate_multi_stage_plan_request(
            request.roadmap,
            &MultiStagePlanRequest {
                agent_id: agent.agent_id.clone(),
                start_node: agent.start_node,
                radius_m: agent.radius_m,
                earliest_start_s: agent.earliest_start_s,
                speed_mps: agent.speed_mps,
                timestep_s: request.timestep_s,
                maximum_expansions_per_stage: request.maximum_low_level_expansions,
                clearance_epsilon_m: request.clearance_epsilon_m,
                targets: targets.clone(),
                occupied_trajectories: request.occupied_trajectories,
            },
        ),
    }
}

fn plan_coordination_agent(
    roadmap: &CoordinationRoadmap,
    request: &ConflictRepairRequest<'_>,
    agent: &CoordinationAgentRequest,
    occupied_trajectories: &[TimedDiscTrajectory],
) -> Result<Option<CoordinationAgentPlan>, CoordinationError> {
    match &agent.task {
        CoordinationAgentTask::Goal {
            goal_node,
            reserve_until_s,
        } => roadmap
            .plan_time_expanded(&TimeExpandedPlanRequest {
                agent_id: agent.agent_id.clone(),
                start_node: agent.start_node,
                goal_node: *goal_node,
                radius_m: agent.radius_m,
                earliest_start_s: agent.earliest_start_s,
                reserve_until_s: *reserve_until_s,
                speed_mps: agent.speed_mps,
                timestep_s: request.timestep_s,
                maximum_expansions: request.maximum_low_level_expansions,
                clearance_epsilon_m: request.clearance_epsilon_m,
                occupied_trajectories,
            })
            .map(|plan| {
                plan.map(|plan| CoordinationAgentPlan {
                    trajectory: plan.trajectory,
                    explored_states: plan.explored_states,
                })
            }),
        CoordinationAgentTask::TimedTargets { targets } => roadmap
            .plan_multi_stage(&MultiStagePlanRequest {
                agent_id: agent.agent_id.clone(),
                start_node: agent.start_node,
                radius_m: agent.radius_m,
                earliest_start_s: agent.earliest_start_s,
                speed_mps: agent.speed_mps,
                timestep_s: request.timestep_s,
                maximum_expansions_per_stage: request.maximum_low_level_expansions,
                clearance_epsilon_m: request.clearance_epsilon_m,
                targets: targets.clone(),
                occupied_trajectories,
            })
            .map(|plan| {
                plan.map(|plan| CoordinationAgentPlan {
                    trajectory: plan.trajectory,
                    explored_states: plan.explored_states,
                })
            }),
    }
}

/// Replan one CBS participant against immutable historical trajectories and
/// only the segment-level prohibitions accumulated for that participant. Peer
/// trajectories remain unconstrained until a concrete conflict introduces the
/// next branch; treating all of them as permanent occupancy would turn this
/// back into priority planning rather than conflict-based search.
fn plan_coordination_agent_with_constraints(
    roadmap: &CoordinationRoadmap,
    request: &ConflictRepairRequest<'_>,
    agent: &CoordinationAgentRequest,
    occupied_trajectories: &[TimedDiscTrajectory],
    constraints: &[TimedDiscSegment],
) -> Result<Option<CoordinationAgentPlan>, CoordinationError> {
    if constraints.is_empty() {
        return plan_coordination_agent(roadmap, request, agent, occupied_trajectories);
    }
    let mut constrained_occupancy = occupied_trajectories.to_vec();
    constrained_occupancy.extend(constraints.iter().enumerate().map(
        |(constraint_index, segment)| TimedDiscTrajectory {
            agent_id: format!("constraint:{}:{constraint_index}", agent.agent_id),
            segments: vec![segment.clone()],
        },
    ));
    plan_coordination_agent(roadmap, request, agent, &constrained_occupancy)
}

/// Seed a repair solve with deterministic priority formation. Unlike the
/// independent seed, every later agent sees the trajectories accepted earlier
/// in this cohort. This is not complete, so [`CoordinationRoadmap::repair_conflicts`]
/// retains its conflict tree when the seed cannot form the cohort.
fn plan_agents_sequentially(
    roadmap: &CoordinationRoadmap,
    request: &ConflictRepairRequest<'_>,
) -> Result<Option<ConflictRepairPlan>, CoordinationError> {
    let mut occupied_trajectories = request.occupied_trajectories.to_vec();
    let mut trajectories = Vec::with_capacity(request.agents.len());
    let mut low_level_explored_states = 0_u64;
    for agent in &request.agents {
        let Some(plan) = plan_coordination_agent(roadmap, request, agent, &occupied_trajectories)?
        else {
            return Ok(None);
        };
        low_level_explored_states = low_level_explored_states.saturating_add(plan.explored_states);
        occupied_trajectories.push(plan.trajectory.clone());
        trajectories.push(plan.trajectory);
    }
    if !timed_disc_conflicts(&occupied_trajectories, request.clearance_epsilon_m)?.is_empty() {
        return Ok(None);
    }
    Ok(Some(ConflictRepairPlan {
        trajectories,
        explored_conflict_tree_nodes: 1,
        low_level_explored_states,
    }))
}

fn push_roadmap_node(
    nodes: &mut Vec<CoordinationRoadmapNode>,
    surface: &Surface,
    point: Point3,
    maximum_nodes: usize,
) -> Result<(), CoordinationError> {
    if nodes.len() >= maximum_nodes {
        return Err(CoordinationError::RoadmapNodeBoundExceeded { maximum_nodes });
    }
    nodes.push(CoordinationRoadmapNode {
        surface: surface.id.clone(),
        position: point,
    });
    Ok(())
}

fn point_has_static_clearance(
    point: Point3,
    surface: &Surface,
    obstacles: &[Obstacle],
    radius_m: f64,
) -> bool {
    surface.contains(point)
        && point.x_m - radius_m >= surface.origin.x_m - NAVIGATION_CLEARANCE_EPSILON_M
        && point.x_m + radius_m
            <= surface.origin.x_m + surface.width_m + NAVIGATION_CLEARANCE_EPSILON_M
        && point.y_m - radius_m >= surface.origin.y_m - NAVIGATION_CLEARANCE_EPSILON_M
        && point.y_m + radius_m
            <= surface.origin.y_m + surface.depth_m + NAVIGATION_CLEARANCE_EPSILON_M
        && obstacles.iter().all(|obstacle| {
            obstacle.surface != surface.id || !obstacle.contains_with_clearance(point, radius_m)
        })
}

fn segment_has_static_clearance(
    start: Point3,
    end: Point3,
    surface: &Surface,
    obstacles: &[Obstacle],
    radius_m: f64,
) -> bool {
    point_has_static_clearance(start, surface, obstacles, radius_m)
        && point_has_static_clearance(end, surface, obstacles, radius_m)
        && obstacles.iter().all(|obstacle| {
            obstacle.surface != surface.id
                || !segment_intersects_expanded_obstacle(start, end, obstacle, radius_m)
        })
}

fn segment_intersects_expanded_obstacle(
    start: Point3,
    end: Point3,
    obstacle: &Obstacle,
    clearance_m: f64,
) -> bool {
    let mut entry = 0.0_f64;
    let mut exit = 1.0_f64;
    for (start_coordinate, delta, minimum, maximum) in [
        (
            start.x_m,
            end.x_m - start.x_m,
            obstacle.at.x_m - clearance_m,
            obstacle.at.x_m + obstacle.width_m + clearance_m,
        ),
        (
            start.y_m,
            end.y_m - start.y_m,
            obstacle.at.y_m - clearance_m,
            obstacle.at.y_m + obstacle.depth_m + clearance_m,
        ),
    ] {
        if delta.abs() <= NAVIGATION_CLEARANCE_EPSILON_M {
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
    entry <= 1.0 && exit >= 0.0
}

fn point_xy_distance(first: Point3, second: Point3) -> f64 {
    (first.x_m - second.x_m).hypot(first.y_m - second.y_m)
}

/// Find the shortest quantised-duration route without enumerating time states.
///
/// The route is only a candidate: [`static_stage_plan`] still validates every
/// continuous move and target wait against the timed occupancy index. If any
/// reservation blocks it, the caller falls back to the time-expanded planner,
/// which may select another route or insert waits.
fn shortest_static_route(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
) -> Result<Option<StaticRoadmapRoute>, CoordinationError> {
    let mut frontier = BinaryHeap::new();
    frontier.push((
        Reverse(search_priority(
            roadmap,
            request,
            TimeExpandedState {
                node: request.start_node,
                step: 0,
            },
        )?),
        Reverse(0_u64),
        Reverse(request.start_node),
    ));
    let mut best_steps = HashMap::from([(request.start_node, 0_u64)]);
    let mut predecessors = HashMap::new();
    let mut explored_nodes = 0_u64;

    while let Some((Reverse(_), Reverse(steps), Reverse(node))) = frontier.pop() {
        if best_steps.get(&node).is_none_or(|best| *best != steps) {
            continue;
        }
        explored_nodes = explored_nodes.saturating_add(1);
        if explored_nodes > request.maximum_expansions {
            return Err(CoordinationError::SearchBoundExceeded {
                maximum_expansions: request.maximum_expansions,
            });
        }
        if node == request.goal_node {
            let mut nodes = vec![node];
            let mut current = node;
            while let Some(previous) = predecessors.get(&current).copied() {
                nodes.push(previous);
                current = previous;
            }
            nodes.reverse();
            return Ok(Some(StaticRoadmapRoute {
                nodes,
                explored_nodes,
            }));
        }
        for &neighbour in &roadmap.adjacency[node] {
            let edge_steps = movement_steps(roadmap, request, node, neighbour)?;
            let candidate_steps = steps.saturating_add(edge_steps);
            if best_steps
                .get(&neighbour)
                .is_some_and(|best| *best <= candidate_steps)
            {
                continue;
            }
            best_steps.insert(neighbour, candidate_steps);
            predecessors.insert(neighbour, node);
            frontier.push((
                Reverse(search_priority(
                    roadmap,
                    request,
                    TimeExpandedState {
                        node: neighbour,
                        step: candidate_steps,
                    },
                )?),
                Reverse(candidate_steps),
                Reverse(neighbour),
            ));
        }
    }
    Ok(None)
}

/// Turn the shortest spatial route into the earliest continuous trajectory
/// that follows it. A failed occupancy check deliberately returns no candidate
/// rather than treating a static route as a reservation-aware solution.
fn static_stage_plan(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
    occupied_index: &TimedDiscSegmentIndex<'_>,
) -> Result<Option<TimeExpandedPlan>, CoordinationError> {
    let Some(route) = shortest_static_route(roadmap, request)? else {
        return Ok(None);
    };
    let mut predecessors = HashMap::new();
    let mut current = TimeExpandedState {
        node: request.start_node,
        step: 0,
    };
    for &next_node in route.nodes.iter().skip(1) {
        let movement_steps = movement_steps(roadmap, request, current.node, next_node)?;
        let Some(next_step) = current.step.checked_add(movement_steps) else {
            return Ok(None);
        };
        let next = TimeExpandedState {
            node: next_node,
            step: next_step,
        };
        if time_for_step(request, next.step) > request.reserve_until_s
            || !action_is_clear(roadmap, request, current, next, occupied_index)
        {
            return Ok(None);
        }
        predecessors.insert(next, current);
        current = next;
    }
    if !final_wait_is_clear(roadmap, request, current, occupied_index) {
        return Ok(None);
    }
    Ok(Some(TimeExpandedPlan {
        trajectory: reconstruct_trajectory(roadmap, request, current, &predecessors),
        explored_states: route.explored_nodes,
    }))
}

/// Search one earliest-arrival label per continuous safe interval at a roadmap
/// node. This represents a temporary reservation as a finite number of safe
/// intervals instead of repeatedly expanding every `(node, time)` wait state.
///
/// Every stationary interval, move, and target reservation remains checked by
/// the exact continuous disc kernel. The full time-expanded search remains a
/// bounded fallback for a case this safe-interval formulation cannot express.
#[allow(clippy::too_many_lines)]
fn reservation_aware_stage_plan(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
    occupied_index: &TimedDiscSegmentIndex<'_>,
) -> Result<Option<TimeExpandedPlan>, CoordinationError> {
    let horizon_steps = duration_to_horizon_steps(
        request.reserve_until_s - request.earliest_start_s,
        request.timestep_s,
    )?;
    let mut safe_intervals_by_node = HashMap::new();
    let start_intervals = safe_intervals_by_node
        .entry(request.start_node)
        .or_insert_with(|| {
            safe_step_intervals_for_node(
                roadmap,
                request,
                request.start_node,
                horizon_steps,
                occupied_index,
            )
        });
    let Some(start_interval_index) = start_intervals
        .iter()
        .position(|interval| interval.starts_at_step == 0)
    else {
        return Ok(None);
    };
    let start = SafeIntervalState {
        node: request.start_node,
        interval_index: start_interval_index,
        arrival_step: 0,
    };
    let mut frontier = BinaryHeap::new();
    frontier.push((
        Reverse(search_priority(
            roadmap,
            request,
            TimeExpandedState {
                node: start.node,
                step: start.arrival_step,
            },
        )?),
        Reverse(0_u64),
        Reverse(start.node),
        start,
    ));
    let mut earliest_arrival = HashMap::from([((start.node, start.interval_index), 0_u64)]);
    let mut predecessors = HashMap::new();
    let mut explored_states = 0_u64;

    while let Some((Reverse(_), Reverse(_), Reverse(_), state)) = frontier.pop() {
        if earliest_arrival
            .get(&(state.node, state.interval_index))
            .is_none_or(|earliest| *earliest != state.arrival_step)
        {
            continue;
        }
        explored_states = explored_states.saturating_add(1);
        if explored_states > request.maximum_expansions {
            return Err(CoordinationError::SearchBoundExceeded {
                maximum_expansions: request.maximum_expansions,
            });
        }
        let Some(current_interval) = safe_intervals_by_node
            .get(&state.node)
            .and_then(|intervals| intervals.get(state.interval_index))
            .copied()
        else {
            return Ok(None);
        };
        if state.node == request.goal_node
            && final_wait_is_clear(
                roadmap,
                request,
                TimeExpandedState {
                    node: state.node,
                    step: state.arrival_step,
                },
                occupied_index,
            )
        {
            return Ok(Some(TimeExpandedPlan {
                trajectory: reconstruct_safe_interval_trajectory(
                    roadmap,
                    request,
                    state,
                    &predecessors,
                ),
                explored_states,
            }));
        }
        for &neighbour in &roadmap.adjacency[state.node] {
            let movement_duration_steps = movement_steps(roadmap, request, state.node, neighbour)?;
            let Some(latest_departure_step) = horizon_steps.checked_sub(movement_duration_steps)
            else {
                continue;
            };
            let latest_departure_step = latest_departure_step.min(current_interval.ends_at_step);
            if state.arrival_step > latest_departure_step {
                continue;
            }
            let mut departure_step = state.arrival_step;
            while departure_step <= latest_departure_step {
                let Some(next_step) = departure_step.checked_add(movement_duration_steps) else {
                    break;
                };
                let move_from = TimeExpandedState {
                    node: state.node,
                    step: departure_step,
                };
                let move_to = TimeExpandedState {
                    node: neighbour,
                    step: next_step,
                };
                let neighbour_intervals =
                    safe_intervals_by_node.entry(neighbour).or_insert_with(|| {
                        safe_step_intervals_for_node(
                            roadmap,
                            request,
                            neighbour,
                            horizon_steps,
                            occupied_index,
                        )
                    });
                let next_interval_index = neighbour_intervals.iter().position(|interval| {
                    interval.starts_at_step <= next_step && next_step <= interval.ends_at_step
                });
                let reaches_reservable_goal = neighbour != request.goal_node
                    || final_wait_is_clear(roadmap, request, move_to, occupied_index);
                if let Some(next_interval_index) = next_interval_index
                    && reaches_reservable_goal
                    && action_is_clear(roadmap, request, move_from, move_to, occupied_index)
                {
                    let next = SafeIntervalState {
                        node: neighbour,
                        interval_index: next_interval_index,
                        arrival_step: next_step,
                    };
                    if earliest_arrival
                        .get(&(next.node, next.interval_index))
                        .is_none_or(|earliest| next.arrival_step < *earliest)
                    {
                        earliest_arrival
                            .insert((next.node, next.interval_index), next.arrival_step);
                        predecessors.insert(
                            next,
                            SafeIntervalPredecessor {
                                state,
                                departure_step,
                            },
                        );
                        frontier.push((
                            Reverse(search_priority(
                                roadmap,
                                request,
                                TimeExpandedState {
                                    node: next.node,
                                    step: next.arrival_step,
                                },
                            )?),
                            Reverse(next.arrival_step),
                            Reverse(next.node),
                            next,
                        ));
                    }
                    break;
                }
                departure_step = departure_step.saturating_add(1);
            }
        }
    }
    Ok(None)
}

fn reconstruct_safe_interval_trajectory(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
    goal: SafeIntervalState,
    predecessors: &HashMap<SafeIntervalState, SafeIntervalPredecessor>,
) -> TimedDiscTrajectory {
    let mut transitions = Vec::new();
    let mut current = goal;
    while let Some(predecessor) = predecessors.get(&current).copied() {
        transitions.push((current, predecessor));
        current = predecessor.state;
    }
    transitions.reverse();
    let mut segments = Vec::new();
    for (next, predecessor) in transitions {
        if predecessor.departure_step > predecessor.state.arrival_step {
            segments.push(segment_for_action(
                roadmap,
                request,
                TimeExpandedState {
                    node: predecessor.state.node,
                    step: predecessor.state.arrival_step,
                },
                TimeExpandedState {
                    node: predecessor.state.node,
                    step: predecessor.departure_step,
                },
            ));
        }
        segments.push(segment_for_action(
            roadmap,
            request,
            TimeExpandedState {
                node: predecessor.state.node,
                step: predecessor.departure_step,
            },
            TimeExpandedState {
                node: next.node,
                step: next.arrival_step,
            },
        ));
    }
    let goal_time_s = time_for_step(request, goal.arrival_step);
    if goal_time_s < request.reserve_until_s {
        let node = &roadmap.nodes[goal.node];
        segments.push(TimedDiscSegment {
            surface: node.surface.clone(),
            starts_at_s: goal_time_s,
            ends_at_s: request.reserve_until_s,
            start: node.position,
            end: node.position,
            radius_m: request.radius_m,
        });
    }
    TimedDiscTrajectory {
        agent_id: request.agent_id.clone(),
        segments,
    }
}

/// Derive discrete time-lattice points that belong to each continuously clear
/// stationary interval for one roadmap node. The interval boundaries come from
/// the exact continuous conflict kernel; discretisation only limits departure
/// choices to the request's declared planning grid.
fn safe_step_intervals_for_node(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
    node_index: usize,
    horizon_steps: u64,
    occupied_index: &TimedDiscSegmentIndex<'_>,
) -> Vec<SafeStepInterval> {
    let node = &roadmap.nodes[node_index];
    let stationary = TimedDiscSegment {
        surface: node.surface.clone(),
        starts_at_s: request.earliest_start_s,
        ends_at_s: request.reserve_until_s,
        start: node.position,
        end: node.position,
        radius_m: request.radius_m,
    };
    let Some(occupied_segments) = occupied_index.by_surface.get(node.surface.as_str()) else {
        return vec![SafeStepInterval {
            starts_at_step: 0,
            ends_at_step: horizon_steps,
        }];
    };
    let mut unsafe_intervals = occupied_segments
        .iter()
        .filter_map(|occupied| {
            (segments_may_overlap(&stationary, occupied, request.clearance_epsilon_m))
                .then(|| {
                    segment_unsafe_interval(&stationary, occupied, request.clearance_epsilon_m)
                })
                .flatten()
                .map(|(starts_at_s, ends_at_s, _)| (starts_at_s, ends_at_s))
        })
        .collect::<Vec<_>>();
    unsafe_intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut merged_unsafe_intervals: Vec<(f64, f64)> = Vec::new();
    for (starts_at_s, ends_at_s) in unsafe_intervals {
        if let Some((_, previous_ends_at_s)) = merged_unsafe_intervals.last_mut()
            && starts_at_s <= *previous_ends_at_s
        {
            *previous_ends_at_s = previous_ends_at_s.max(ends_at_s);
            continue;
        }
        merged_unsafe_intervals.push((starts_at_s, ends_at_s));
    }

    let mut safe_intervals = Vec::new();
    let mut safe_starts_at_s = request.earliest_start_s;
    for (unsafe_starts_at_s, unsafe_ends_at_s) in merged_unsafe_intervals {
        let unsafe_starts_at_s = unsafe_starts_at_s.max(request.earliest_start_s);
        let unsafe_ends_at_s = unsafe_ends_at_s.min(request.reserve_until_s);
        if safe_starts_at_s <= unsafe_starts_at_s {
            append_safe_step_interval(
                &mut safe_intervals,
                request,
                horizon_steps,
                safe_starts_at_s,
                unsafe_starts_at_s,
            );
        }
        safe_starts_at_s = safe_starts_at_s.max(unsafe_ends_at_s);
    }
    if safe_starts_at_s <= request.reserve_until_s {
        append_safe_step_interval(
            &mut safe_intervals,
            request,
            horizon_steps,
            safe_starts_at_s,
            request.reserve_until_s,
        );
    }
    safe_intervals
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn append_safe_step_interval(
    intervals: &mut Vec<SafeStepInterval>,
    request: &TimeExpandedPlanRequest<'_>,
    horizon_steps: u64,
    starts_at_s: f64,
    ends_at_s: f64,
) {
    let starts_at_step = ((starts_at_s - request.earliest_start_s) / request.timestep_s)
        .ceil()
        .max(0.0);
    let ends_at_step = ((ends_at_s - request.earliest_start_s) / request.timestep_s)
        .floor()
        .min(horizon_steps as f64);
    if starts_at_step > ends_at_step {
        return;
    }
    let interval = SafeStepInterval {
        starts_at_step: starts_at_step as u64,
        ends_at_step: ends_at_step as u64,
    };
    if let Some(previous) = intervals.last_mut()
        && interval.starts_at_step <= previous.ends_at_step.saturating_add(1)
    {
        previous.ends_at_step = previous.ends_at_step.max(interval.ends_at_step);
        return;
    }
    intervals.push(interval);
}

/// Return the exact direct transition when the roadmap already contains the
/// requested edge. A caller falls back to the general time-expanded search if
/// this immediate move or target reservation conflicts with occupancy.
fn direct_stage_plan(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
    occupied_index: &TimedDiscSegmentIndex<'_>,
) -> Result<Option<TimeExpandedPlan>, CoordinationError> {
    let start = TimeExpandedState {
        node: request.start_node,
        step: 0,
    };
    if request.start_node == request.goal_node {
        return Ok(
            final_wait_is_clear(roadmap, request, start, occupied_index).then(|| {
                TimeExpandedPlan {
                    trajectory: reconstruct_trajectory(roadmap, request, start, &HashMap::new()),
                    explored_states: 0,
                }
            }),
        );
    }
    if roadmap.adjacency[request.start_node]
        .binary_search(&request.goal_node)
        .is_err()
    {
        return Ok(None);
    }
    let movement_steps = movement_steps(roadmap, request, request.start_node, request.goal_node)?;
    let goal = TimeExpandedState {
        node: request.goal_node,
        step: movement_steps,
    };
    if time_for_step(request, goal.step) > request.reserve_until_s
        || !action_is_clear(roadmap, request, start, goal, occupied_index)
        || !final_wait_is_clear(roadmap, request, goal, occupied_index)
    {
        return Ok(None);
    }
    let predecessors = HashMap::from([(goal, start)]);
    Ok(Some(TimeExpandedPlan {
        trajectory: reconstruct_trajectory(roadmap, request, goal, &predecessors),
        explored_states: 0,
    }))
}

fn final_wait_is_clear(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
    state: TimeExpandedState,
    occupied_index: &TimedDiscSegmentIndex<'_>,
) -> bool {
    let starts_at_s = time_for_step(request, state.step);
    if starts_at_s >= request.reserve_until_s {
        return true;
    }
    let node = &roadmap.nodes[state.node];
    let candidate = TimedDiscSegment {
        surface: node.surface.clone(),
        starts_at_s,
        ends_at_s: request.reserve_until_s,
        start: node.position,
        end: node.position,
        radius_m: request.radius_m,
    };
    occupied_index.is_clear(&candidate, request.clearance_epsilon_m)
}

fn action_is_clear(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
    from: TimeExpandedState,
    to: TimeExpandedState,
    occupied_index: &TimedDiscSegmentIndex<'_>,
) -> bool {
    let candidate = segment_for_action(roadmap, request, from, to);
    occupied_index.is_clear(&candidate, request.clearance_epsilon_m)
}

fn segment_for_action(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
    from: TimeExpandedState,
    to: TimeExpandedState,
) -> TimedDiscSegment {
    let first = &roadmap.nodes[from.node];
    let second = &roadmap.nodes[to.node];
    TimedDiscSegment {
        surface: first.surface.clone(),
        starts_at_s: time_for_step(request, from.step),
        ends_at_s: time_for_step(request, to.step),
        start: first.position,
        end: second.position,
        radius_m: request.radius_m,
    }
}

fn reconstruct_trajectory(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
    goal: TimeExpandedState,
    predecessors: &HashMap<TimeExpandedState, TimeExpandedState>,
) -> TimedDiscTrajectory {
    let mut states = vec![goal];
    let mut current = goal;
    while let Some(previous) = predecessors.get(&current).copied() {
        states.push(previous);
        current = previous;
    }
    states.reverse();
    let mut segments = states
        .windows(2)
        .map(|window| segment_for_action(roadmap, request, window[0], window[1]))
        .collect::<Vec<_>>();
    let goal_time_s = time_for_step(request, goal.step);
    if goal_time_s < request.reserve_until_s {
        let node = &roadmap.nodes[goal.node];
        segments.push(TimedDiscSegment {
            surface: node.surface.clone(),
            starts_at_s: goal_time_s,
            ends_at_s: request.reserve_until_s,
            start: node.position,
            end: node.position,
            radius_m: request.radius_m,
        });
    }
    TimedDiscTrajectory {
        agent_id: request.agent_id.clone(),
        segments,
    }
}

fn movement_steps(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
    first_node: usize,
    second_node: usize,
) -> Result<u64, CoordinationError> {
    let first = &roadmap.nodes[first_node];
    let second = &roadmap.nodes[second_node];
    let distance_m =
        (first.position.x_m - second.position.x_m).hypot(first.position.y_m - second.position.y_m);
    duration_to_steps(distance_m / request.speed_mps, request.timestep_s)
}

fn search_priority(
    roadmap: &CoordinationRoadmap,
    request: &TimeExpandedPlanRequest<'_>,
    state: TimeExpandedState,
) -> Result<u64, CoordinationError> {
    let distance_to_goal_m = point_xy_distance(
        roadmap.nodes[state.node].position,
        roadmap.nodes[request.goal_node].position,
    );
    let optimistic_steps =
        duration_to_optimistic_steps(distance_to_goal_m / request.speed_mps, request.timestep_s)?;
    Ok(state.step.saturating_add(optimistic_steps))
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn duration_to_steps(duration_s: f64, timestep_s: f64) -> Result<u64, CoordinationError> {
    debug_assert!(duration_s.is_finite() && duration_s >= 0.0);
    debug_assert!(timestep_s.is_finite() && timestep_s > 0.0);
    let steps = (duration_s / timestep_s).ceil().max(1.0);
    if steps > u64::MAX as f64 {
        return Err(CoordinationError::NonFinitePlanRequest);
    }
    Ok(steps as u64)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn duration_to_optimistic_steps(
    duration_s: f64,
    timestep_s: f64,
) -> Result<u64, CoordinationError> {
    debug_assert!(duration_s.is_finite() && duration_s >= 0.0);
    debug_assert!(timestep_s.is_finite() && timestep_s > 0.0);
    let steps = (duration_s / timestep_s).floor();
    if steps > u64::MAX as f64 {
        return Err(CoordinationError::NonFinitePlanRequest);
    }
    Ok(steps as u64)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn duration_to_horizon_steps(duration_s: f64, timestep_s: f64) -> Result<u64, CoordinationError> {
    debug_assert!(duration_s.is_finite() && duration_s >= 0.0);
    debug_assert!(timestep_s.is_finite() && timestep_s > 0.0);
    let steps = (duration_s / timestep_s).floor();
    if steps > u64::MAX as f64 {
        return Err(CoordinationError::NonFinitePlanRequest);
    }
    Ok(steps as u64)
}

#[allow(clippy::cast_precision_loss)]
fn time_for_step(request: &TimeExpandedPlanRequest<'_>, step: u64) -> f64 {
    request.earliest_start_s + step as f64 * request.timestep_s
}

fn validate_trajectories(
    trajectories: &[TimedDiscTrajectory],
    clearance_epsilon_m: f64,
) -> Result<(), CoordinationError> {
    if !clearance_epsilon_m.is_finite() || clearance_epsilon_m < 0.0 {
        return Err(CoordinationError::InvalidClearanceEpsilon);
    }
    let mut agent_ids = std::collections::BTreeSet::new();
    for trajectory in trajectories {
        if trajectory.agent_id.is_empty() {
            return Err(CoordinationError::EmptyAgentId);
        }
        if !agent_ids.insert(&trajectory.agent_id) {
            return Err(CoordinationError::DuplicateAgentId {
                agent_id: trajectory.agent_id.clone(),
            });
        }
        for (segment_index, segment) in trajectory.segments.iter().enumerate() {
            if segment.surface.is_empty() {
                return Err(CoordinationError::EmptySurface {
                    agent_id: trajectory.agent_id.clone(),
                    segment_index,
                });
            }
            if !segment.starts_at_s.is_finite()
                || !segment.ends_at_s.is_finite()
                || !point_is_finite(segment.start)
                || !point_is_finite(segment.end)
                || !segment.radius_m.is_finite()
            {
                return Err(CoordinationError::NonFiniteSegment {
                    agent_id: trajectory.agent_id.clone(),
                    segment_index,
                });
            }
            if segment.ends_at_s <= segment.starts_at_s {
                return Err(CoordinationError::NonPositiveDuration {
                    agent_id: trajectory.agent_id.clone(),
                    segment_index,
                });
            }
            if segment.radius_m < 0.0 {
                return Err(CoordinationError::NegativeRadius {
                    agent_id: trajectory.agent_id.clone(),
                    segment_index,
                });
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn segment_conflict(
    first_trajectory: &TimedDiscTrajectory,
    first_segment_index: usize,
    first: &TimedDiscSegment,
    second_trajectory: &TimedDiscTrajectory,
    second_segment_index: usize,
    second: &TimedDiscSegment,
    clearance_epsilon_m: f64,
) -> Option<TimedDiscConflict> {
    let (unsafe_from_s, unsafe_until_s, maximum_overlap_m) =
        segment_unsafe_interval(first, second, clearance_epsilon_m)?;
    Some(TimedDiscConflict {
        first_agent_id: first_trajectory.agent_id.clone(),
        second_agent_id: second_trajectory.agent_id.clone(),
        surface: first.surface.clone(),
        first_segment_index,
        second_segment_index,
        unsafe_from_s,
        unsafe_until_s,
        maximum_overlap_m,
    })
}

fn segment_unsafe_interval(
    first: &TimedDiscSegment,
    second: &TimedDiscSegment,
    clearance_epsilon_m: f64,
) -> Option<(f64, f64, f64)> {
    if first.surface != second.surface {
        return None;
    }
    let starts_at_s = first.starts_at_s.max(second.starts_at_s);
    let ends_at_s = first.ends_at_s.min(second.ends_at_s);
    if ends_at_s <= starts_at_s {
        return None;
    }
    let first_start = position_at(first, starts_at_s);
    let second_start = position_at(second, starts_at_s);
    let duration_s = ends_at_s - starts_at_s;
    let first_velocity = velocity_xy(first);
    let second_velocity = velocity_xy(second);
    let relative_start = (
        first_start.x_m - second_start.x_m,
        first_start.y_m - second_start.y_m,
    );
    let relative_velocity = (
        first_velocity.0 - second_velocity.0,
        first_velocity.1 - second_velocity.1,
    );
    let radius_m = first.radius_m + second.radius_m - clearance_epsilon_m;
    if radius_m <= 0.0 {
        return None;
    }
    let (unsafe_from_s, unsafe_until_s) =
        unsafe_interval(relative_start, relative_velocity, radius_m, duration_s)?;
    let closest_at_s = closest_approach(relative_start, relative_velocity, duration_s);
    let closest = (
        relative_start.0 + relative_velocity.0 * closest_at_s,
        relative_start.1 + relative_velocity.1 * closest_at_s,
    );
    let maximum_overlap_m = first.radius_m + second.radius_m - closest.0.hypot(closest.1);
    Some((
        starts_at_s + unsafe_from_s,
        starts_at_s + unsafe_until_s,
        maximum_overlap_m,
    ))
}

fn segments_may_overlap(
    first: &TimedDiscSegment,
    second: &TimedDiscSegment,
    clearance_epsilon_m: f64,
) -> bool {
    let interaction_radius_m = first.radius_m + second.radius_m - clearance_epsilon_m;
    if interaction_radius_m <= 0.0 {
        return false;
    }
    let first_horizontal_bounds = (
        first.start.x_m.min(first.end.x_m),
        first.start.x_m.max(first.end.x_m),
    );
    let first_vertical_bounds = (
        first.start.y_m.min(first.end.y_m),
        first.start.y_m.max(first.end.y_m),
    );
    let second_horizontal_bounds = (
        second.start.x_m.min(second.end.x_m),
        second.start.x_m.max(second.end.x_m),
    );
    let second_vertical_bounds = (
        second.start.y_m.min(second.end.y_m),
        second.start.y_m.max(second.end.y_m),
    );
    first_horizontal_bounds.1 + interaction_radius_m >= second_horizontal_bounds.0
        && second_horizontal_bounds.1 + interaction_radius_m >= first_horizontal_bounds.0
        && first_vertical_bounds.1 + interaction_radius_m >= second_vertical_bounds.0
        && second_vertical_bounds.1 + interaction_radius_m >= first_vertical_bounds.0
}

/// The open interval `0 < t < duration_s` in which the relative reference
/// discs overlap beyond `radius_m`. Endpoint tangencies are clear, matching
/// the runtime audit's `overlap > epsilon` rule.
fn unsafe_interval(
    relative_start: (f64, f64),
    relative_velocity: (f64, f64),
    radius_m: f64,
    duration_s: f64,
) -> Option<(f64, f64)> {
    let a = relative_velocity
        .0
        .mul_add(relative_velocity.0, relative_velocity.1.powi(2));
    let b = 2.0
        * relative_start
            .0
            .mul_add(relative_velocity.0, relative_start.1 * relative_velocity.1);
    let c = relative_start
        .0
        .mul_add(relative_start.0, relative_start.1.powi(2))
        - radius_m.powi(2);
    if a <= f64::EPSILON {
        return (c < 0.0).then_some((0.0, duration_s));
    }
    let discriminant = b.mul_add(b, -4.0 * a * c);
    if discriminant <= 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let lower = (-b - root) / (2.0 * a);
    let upper = (-b + root) / (2.0 * a);
    let unsafe_from_s = lower.max(0.0);
    let unsafe_until_s = upper.min(duration_s);
    (unsafe_from_s < unsafe_until_s).then_some((unsafe_from_s, unsafe_until_s))
}

fn closest_approach(
    relative_start: (f64, f64),
    relative_velocity: (f64, f64),
    duration_s: f64,
) -> f64 {
    let velocity_squared = relative_velocity
        .0
        .mul_add(relative_velocity.0, relative_velocity.1.powi(2));
    if velocity_squared <= f64::EPSILON {
        return 0.0;
    }
    (-relative_start
        .0
        .mul_add(relative_velocity.0, relative_start.1 * relative_velocity.1)
        / velocity_squared)
        .clamp(0.0, duration_s)
}

fn position_at(segment: &TimedDiscSegment, time_s: f64) -> Point3 {
    let fraction = (time_s - segment.starts_at_s) / (segment.ends_at_s - segment.starts_at_s);
    segment.start.lerp(segment.end, fraction)
}

fn velocity_xy(segment: &TimedDiscSegment) -> (f64, f64) {
    let duration_s = segment.ends_at_s - segment.starts_at_s;
    (
        (segment.end.x_m - segment.start.x_m) / duration_s,
        (segment.end.y_m - segment.start.y_m) / duration_s,
    )
}

fn point_is_finite(point: Point3) -> bool {
    point.x_m.is_finite() && point.y_m.is_finite() && point.z_m.is_finite()
}

fn conflict_order(left: &TimedDiscConflict, right: &TimedDiscConflict) -> Ordering {
    left.unsafe_from_s
        .total_cmp(&right.unsafe_from_s)
        .then_with(|| left.unsafe_until_s.total_cmp(&right.unsafe_until_s))
        .then_with(|| left.first_agent_id.cmp(&right.first_agent_id))
        .then_with(|| left.second_agent_id.cmp(&right.second_agent_id))
        .then_with(|| left.first_segment_index.cmp(&right.first_segment_index))
        .then_with(|| left.second_segment_index.cmp(&right.second_segment_index))
}

#[cfg(test)]
mod tests {
    use super::{
        ConflictRepairRequest, CoordinationAgentRequest, CoordinationAgentTask, CoordinationError,
        CoordinationRoadmap, CoordinationRoadmapEdge, CoordinationRoadmapNode,
        MultiStagePlanRequest, QueueGridCoordinationRequest, QueueGridRollingCoordinationRequest,
        QueueGridRollingOutcome, QueueGridServiceAssumption, QueueGridServiceDeparture,
        QueueGridSlotWindow, QueueGridTicketActivation, QueueGridTicketRequest,
        TimeExpandedPlanRequest, TimedDiscSegment, TimedDiscTrajectory, TimedRoadmapTarget,
        assess_queue_grid_rolling, estimate_queue_grid_departures, first_timed_disc_conflict,
        plan_queue_grid, plan_queue_grid_rolling, point_has_static_clearance,
        queue_grid_slot_windows, queue_grid_timed_targets, reference_clearance_epsilon_m,
        segment_has_static_clearance, timed_disc_conflicts,
    };
    use crate::model::{Obstacle, Point3, Surface};

    fn segment(
        start: (f64, f64),
        end: (f64, f64),
        starts_at_s: f64,
        ends_at_s: f64,
    ) -> TimedDiscSegment {
        TimedDiscSegment {
            surface: "concourse".to_owned(),
            starts_at_s,
            ends_at_s,
            start: Point3 {
                x_m: start.0,
                y_m: start.1,
                z_m: 0.0,
            },
            end: Point3 {
                x_m: end.0,
                y_m: end.1,
                z_m: 0.0,
            },
            radius_m: 0.3,
        }
    }

    fn roadmap() -> CoordinationRoadmap {
        CoordinationRoadmap::new(
            vec![
                CoordinationRoadmapNode {
                    surface: "concourse".to_owned(),
                    position: Point3 {
                        x_m: 0.0,
                        y_m: 0.0,
                        z_m: 0.0,
                    },
                },
                CoordinationRoadmapNode {
                    surface: "concourse".to_owned(),
                    position: Point3 {
                        x_m: 1.0,
                        y_m: 0.0,
                        z_m: 0.0,
                    },
                },
                CoordinationRoadmapNode {
                    surface: "concourse".to_owned(),
                    position: Point3 {
                        x_m: 2.0,
                        y_m: 0.0,
                        z_m: 0.0,
                    },
                },
            ],
            &[
                CoordinationRoadmapEdge {
                    first_node: 0,
                    second_node: 1,
                },
                CoordinationRoadmapEdge {
                    first_node: 1,
                    second_node: 2,
                },
            ],
        )
        .expect("valid test roadmap")
    }

    fn repair_roadmap() -> CoordinationRoadmap {
        CoordinationRoadmap::new(
            vec![
                CoordinationRoadmapNode {
                    surface: "concourse".to_owned(),
                    position: Point3 {
                        x_m: 0.0,
                        y_m: 0.0,
                        z_m: 0.0,
                    },
                },
                CoordinationRoadmapNode {
                    surface: "concourse".to_owned(),
                    position: Point3 {
                        x_m: 1.0,
                        y_m: 0.0,
                        z_m: 0.0,
                    },
                },
                CoordinationRoadmapNode {
                    surface: "concourse".to_owned(),
                    position: Point3 {
                        x_m: 2.0,
                        y_m: 0.0,
                        z_m: 0.0,
                    },
                },
                CoordinationRoadmapNode {
                    surface: "concourse".to_owned(),
                    position: Point3 {
                        x_m: 1.0,
                        y_m: 1.0,
                        z_m: 0.0,
                    },
                },
            ],
            &[
                CoordinationRoadmapEdge {
                    first_node: 0,
                    second_node: 1,
                },
                CoordinationRoadmapEdge {
                    first_node: 1,
                    second_node: 2,
                },
                CoordinationRoadmapEdge {
                    first_node: 0,
                    second_node: 3,
                },
                CoordinationRoadmapEdge {
                    first_node: 3,
                    second_node: 2,
                },
            ],
        )
        .expect("valid repair roadmap")
    }

    fn lattice_surface() -> Surface {
        Surface {
            id: "concourse".to_owned(),
            origin: Point3 {
                x_m: 0.0,
                y_m: 0.0,
                z_m: 0.0,
            },
            width_m: 4.0,
            depth_m: 4.0,
        }
    }

    fn plan_request(occupied_trajectories: &[TimedDiscTrajectory]) -> TimeExpandedPlanRequest<'_> {
        TimeExpandedPlanRequest {
            agent_id: "planned".to_owned(),
            start_node: 0,
            goal_node: 2,
            radius_m: 0.3,
            earliest_start_s: 0.0,
            reserve_until_s: 4.0,
            speed_mps: 1.0,
            timestep_s: 0.5,
            maximum_expansions: 100,
            clearance_epsilon_m: 0.0,
            occupied_trajectories,
        }
    }

    #[test]
    fn finds_an_interior_crossing_with_its_exact_unsafe_interval() {
        let trajectories = vec![
            TimedDiscTrajectory {
                agent_id: "a".to_owned(),
                segments: vec![segment((-1.0, 0.0), (1.0, 0.0), 0.0, 2.0)],
            },
            TimedDiscTrajectory {
                agent_id: "b".to_owned(),
                segments: vec![segment((0.0, -1.0), (0.0, 1.0), 0.0, 2.0)],
            },
        ];

        let conflict = first_timed_disc_conflict(&trajectories, 0.0)
            .expect("valid trajectories")
            .expect("crossing conflicts");

        assert_eq!(conflict.first_agent_id, "a");
        assert_eq!(conflict.second_agent_id, "b");
        assert!((conflict.unsafe_from_s - 0.575_735_931_3).abs() < 1.0e-9);
        assert!((conflict.unsafe_until_s - 1.424_264_068_7).abs() < 1.0e-9);
        assert!((conflict.maximum_overlap_m - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn accepts_a_timed_separation_of_geometrically_crossing_paths() {
        let trajectories = vec![
            TimedDiscTrajectory {
                agent_id: "a".to_owned(),
                segments: vec![segment((-1.0, 0.0), (1.0, 0.0), 0.0, 1.0)],
            },
            TimedDiscTrajectory {
                agent_id: "b".to_owned(),
                segments: vec![segment((0.0, -1.0), (0.0, 1.0), 2.0, 3.0)],
            },
        ];

        assert!(
            timed_disc_conflicts(&trajectories, 0.0)
                .expect("valid trajectories")
                .is_empty()
        );
    }

    #[test]
    fn ignores_segments_on_different_surfaces() {
        let mut second = segment((-1.0, 0.0), (1.0, 0.0), 0.0, 2.0);
        second.surface = "platform".to_owned();
        let trajectories = vec![
            TimedDiscTrajectory {
                agent_id: "a".to_owned(),
                segments: vec![segment((-1.0, 0.0), (1.0, 0.0), 0.0, 2.0)],
            },
            TimedDiscTrajectory {
                agent_id: "b".to_owned(),
                segments: vec![second],
            },
        ];

        assert!(
            timed_disc_conflicts(&trajectories, 0.0)
                .expect("valid trajectories")
                .is_empty()
        );
    }

    #[test]
    fn applies_the_runtime_clearance_epsilon_to_near_tangencies() {
        let trajectories = vec![
            TimedDiscTrajectory {
                agent_id: "a".to_owned(),
                segments: vec![segment((0.0, 0.0), (1.0, 0.0), 0.0, 1.0)],
            },
            TimedDiscTrajectory {
                agent_id: "b".to_owned(),
                segments: vec![segment((0.0, 0.6), (1.0, 0.6), 0.0, 1.0)],
            },
        ];

        assert!(
            timed_disc_conflicts(&trajectories, reference_clearance_epsilon_m())
                .expect("valid trajectories")
                .is_empty()
        );
    }

    #[test]
    fn rejects_duplicate_agent_ids() {
        let trajectories = vec![
            TimedDiscTrajectory {
                agent_id: "a".to_owned(),
                segments: vec![segment((0.0, 0.0), (1.0, 0.0), 0.0, 1.0)],
            },
            TimedDiscTrajectory {
                agent_id: "a".to_owned(),
                segments: vec![segment((1.0, 0.0), (2.0, 0.0), 0.0, 1.0)],
            },
        ];

        assert_eq!(
            timed_disc_conflicts(&trajectories, 0.0),
            Err(CoordinationError::DuplicateAgentId {
                agent_id: "a".to_owned(),
            })
        );
    }

    #[test]
    fn time_expanded_roadmap_plans_moves_and_a_goal_reservation() {
        let plan = roadmap()
            .plan_time_expanded(&plan_request(&[]))
            .expect("valid request")
            .expect("roadmap has a route");

        assert_eq!(plan.trajectory.segments.len(), 3);
        assert!(plan.trajectory.segments[0].starts_at_s.abs() < f64::EPSILON);
        assert!((plan.trajectory.segments[0].ends_at_s - 1.0).abs() < f64::EPSILON);
        assert!((plan.trajectory.segments[1].ends_at_s - 2.0).abs() < f64::EPSILON);
        assert_eq!(
            plan.trajectory.segments[2].start,
            plan.trajectory.segments[2].end
        );
        assert!((plan.trajectory.segments[2].starts_at_s - 2.0).abs() < f64::EPSILON);
        assert!((plan.trajectory.segments[2].ends_at_s - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn time_expanded_roadmap_preserves_a_nonintegral_reservation_horizon() {
        let mut request = plan_request(&[]);
        request.reserve_until_s = 4.1;
        let plan = roadmap()
            .plan_time_expanded(&request)
            .expect("valid request")
            .expect("roadmap has a route");

        let final_segment = plan.trajectory.segments.last().expect("goal wait exists");
        assert!((final_segment.ends_at_s - 4.1).abs() < f64::EPSILON);
    }

    #[test]
    fn time_expanded_roadmap_waits_until_a_reserved_vertex_clears() {
        let occupied = vec![TimedDiscTrajectory {
            agent_id: "occupied".to_owned(),
            segments: vec![segment((1.0, 0.0), (1.0, 0.0), 0.0, 1.1)],
        }];
        let plan = roadmap()
            .plan_time_expanded(&plan_request(&occupied))
            .expect("valid request")
            .expect("roadmap can wait and route");

        let first_move = plan
            .trajectory
            .segments
            .iter()
            .find(|segment| segment.start != segment.end)
            .expect("route contains a move");
        assert!(first_move.starts_at_s >= 1.0);
        assert!(
            timed_disc_conflicts(&[plan.trajectory, occupied[0].clone()], 0.0)
                .expect("valid paths")
                .is_empty()
        );
    }

    #[test]
    fn time_expanded_roadmap_reports_a_search_bound_distinctly() {
        let mut request = plan_request(&[]);
        request.maximum_expansions = 1;

        assert_eq!(
            roadmap().plan_time_expanded(&request),
            Err(CoordinationError::SearchBoundExceeded {
                maximum_expansions: 1,
            })
        );
    }

    #[test]
    fn multi_stage_plan_holds_each_queue_rank_through_its_handoff() {
        let request = MultiStagePlanRequest {
            agent_id: "ticket:1".to_owned(),
            start_node: 0,
            radius_m: 0.3,
            earliest_start_s: 0.0,
            speed_mps: 1.0,
            timestep_s: 0.5,
            maximum_expansions_per_stage: 100,
            clearance_epsilon_m: 0.0,
            targets: vec![
                TimedRoadmapTarget {
                    node: 1,
                    starts_at_s: 0.0,
                    ends_at_s: 1.0,
                },
                TimedRoadmapTarget {
                    node: 2,
                    starts_at_s: 1.0,
                    ends_at_s: 3.0,
                },
            ],
            occupied_trajectories: &[],
        };

        let plan = roadmap()
            .plan_multi_stage(&request)
            .expect("valid task windows")
            .expect("each target is reachable in its active window");

        assert_eq!(plan.trajectory.segments.len(), 3);
        assert!(plan.trajectory.segments[0].start.x_m.abs() < f64::EPSILON);
        assert!((plan.trajectory.segments[0].end.x_m - 1.0).abs() < f64::EPSILON);
        assert!((plan.trajectory.segments[0].ends_at_s - 1.0).abs() < f64::EPSILON);
        assert!((plan.trajectory.segments[1].start.x_m - 1.0).abs() < f64::EPSILON);
        assert!((plan.trajectory.segments[1].end.x_m - 2.0).abs() < f64::EPSILON);
        assert!((plan.trajectory.segments[1].starts_at_s - 1.0).abs() < f64::EPSILON);
        assert_eq!(
            plan.trajectory.segments[2].start,
            plan.trajectory.segments[2].end
        );
        assert!((plan.trajectory.segments[2].starts_at_s - 2.0).abs() < f64::EPSILON);
        assert!((plan.trajectory.segments[2].ends_at_s - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn multi_stage_plan_reports_an_unreachable_handoff_window_as_no_plan() {
        let request = MultiStagePlanRequest {
            agent_id: "ticket:1".to_owned(),
            start_node: 0,
            radius_m: 0.3,
            earliest_start_s: 0.0,
            speed_mps: 1.0,
            timestep_s: 0.5,
            maximum_expansions_per_stage: 100,
            clearance_epsilon_m: 0.0,
            targets: vec![TimedRoadmapTarget {
                node: 2,
                starts_at_s: 0.0,
                ends_at_s: 1.0,
            }],
            occupied_trajectories: &[],
        };

        assert_eq!(roadmap().plan_multi_stage(&request), Ok(None));
    }

    #[test]
    fn multi_stage_plan_rejects_non_contiguous_handoffs() {
        let request = MultiStagePlanRequest {
            agent_id: "ticket:1".to_owned(),
            start_node: 0,
            radius_m: 0.3,
            earliest_start_s: 0.0,
            speed_mps: 1.0,
            timestep_s: 0.5,
            maximum_expansions_per_stage: 100,
            clearance_epsilon_m: 0.0,
            targets: vec![
                TimedRoadmapTarget {
                    node: 1,
                    starts_at_s: 0.0,
                    ends_at_s: 1.0,
                },
                TimedRoadmapTarget {
                    node: 2,
                    starts_at_s: 1.5,
                    ends_at_s: 3.0,
                },
            ],
            occupied_trajectories: &[],
        };

        assert_eq!(
            roadmap().plan_multi_stage(&request),
            Err(CoordinationError::NonContiguousMultiStageTarget { target_index: 1 })
        );
    }

    #[test]
    fn bounded_conflict_repair_returns_only_an_exactly_clear_plan() {
        let roadmap = repair_roadmap();
        let request = ConflictRepairRequest {
            agents: vec![
                CoordinationAgentRequest {
                    agent_id: "a".to_owned(),
                    start_node: 0,
                    radius_m: 0.2,
                    earliest_start_s: 0.0,
                    speed_mps: 1.0,
                    task: CoordinationAgentTask::Goal {
                        goal_node: 2,
                        reserve_until_s: 4.0,
                    },
                },
                CoordinationAgentRequest {
                    agent_id: "b".to_owned(),
                    start_node: 2,
                    radius_m: 0.2,
                    earliest_start_s: 0.0,
                    speed_mps: 1.0,
                    task: CoordinationAgentTask::Goal {
                        goal_node: 0,
                        reserve_until_s: 4.0,
                    },
                },
            ],
            occupied_trajectories: &[],
            timestep_s: 0.5,
            maximum_low_level_expansions: 100,
            maximum_conflict_tree_nodes: 10,
            clearance_epsilon_m: 0.0,
            roadmap: &roadmap,
        };

        let plan = roadmap
            .repair_conflicts(&request)
            .expect("bounded search succeeds")
            .expect("detour resolves the crossing");

        assert!(plan.explored_conflict_tree_nodes > 0);
        assert!(
            timed_disc_conflicts(&plan.trajectories, 0.0)
                .expect("valid repaired trajectories")
                .is_empty()
        );
    }

    #[test]
    fn bounded_conflict_repair_resolves_timed_queue_target_conflicts() {
        let roadmap = repair_roadmap();
        let request = ConflictRepairRequest {
            agents: vec![
                CoordinationAgentRequest {
                    agent_id: "ticket:1".to_owned(),
                    start_node: 0,
                    radius_m: 0.2,
                    earliest_start_s: 0.0,
                    speed_mps: 1.0,
                    task: CoordinationAgentTask::TimedTargets {
                        targets: vec![TimedRoadmapTarget {
                            node: 2,
                            starts_at_s: 0.0,
                            ends_at_s: 4.0,
                        }],
                    },
                },
                CoordinationAgentRequest {
                    agent_id: "ticket:2".to_owned(),
                    start_node: 2,
                    radius_m: 0.2,
                    earliest_start_s: 0.0,
                    speed_mps: 1.0,
                    task: CoordinationAgentTask::TimedTargets {
                        targets: vec![TimedRoadmapTarget {
                            node: 0,
                            starts_at_s: 0.0,
                            ends_at_s: 4.0,
                        }],
                    },
                },
            ],
            occupied_trajectories: &[],
            timestep_s: 0.5,
            maximum_low_level_expansions: 100,
            maximum_conflict_tree_nodes: 10,
            clearance_epsilon_m: 0.0,
            roadmap: &roadmap,
        };

        let plan = roadmap
            .repair_conflicts(&request)
            .expect("bounded repair search succeeds")
            .expect("detour resolves the timed target conflict");

        assert!(
            timed_disc_conflicts(&plan.trajectories, 0.0)
                .expect("valid repaired trajectories")
                .is_empty()
        );
    }

    #[test]
    fn bounded_conflict_repair_reports_tree_exhaustion_as_unknown() {
        let roadmap = repair_roadmap();
        let request = ConflictRepairRequest {
            agents: vec![
                CoordinationAgentRequest {
                    agent_id: "a".to_owned(),
                    start_node: 0,
                    radius_m: 0.2,
                    earliest_start_s: 0.0,
                    speed_mps: 1.0,
                    task: CoordinationAgentTask::Goal {
                        goal_node: 2,
                        reserve_until_s: 4.0,
                    },
                },
                CoordinationAgentRequest {
                    agent_id: "b".to_owned(),
                    start_node: 2,
                    radius_m: 0.2,
                    earliest_start_s: 0.0,
                    speed_mps: 1.0,
                    task: CoordinationAgentTask::Goal {
                        goal_node: 0,
                        reserve_until_s: 4.0,
                    },
                },
            ],
            occupied_trajectories: &[],
            timestep_s: 0.5,
            maximum_low_level_expansions: 100,
            maximum_conflict_tree_nodes: 1,
            clearance_epsilon_m: 0.0,
            roadmap: &roadmap,
        };

        assert_eq!(
            roadmap.repair_conflicts(&request),
            Err(CoordinationError::ConflictRepairBoundExceeded {
                maximum_conflict_tree_nodes: 1,
            })
        );
    }

    #[test]
    fn rolling_conflict_repair_preserves_an_accepted_trajectory_handoff() {
        let roadmap = repair_roadmap();
        let first_request = ConflictRepairRequest {
            agents: vec![CoordinationAgentRequest {
                agent_id: "a".to_owned(),
                start_node: 0,
                radius_m: 0.2,
                earliest_start_s: 0.0,
                speed_mps: 1.0,
                task: CoordinationAgentTask::Goal {
                    goal_node: 2,
                    reserve_until_s: 4.0,
                },
            }],
            occupied_trajectories: &[],
            timestep_s: 0.5,
            maximum_low_level_expansions: 100,
            maximum_conflict_tree_nodes: 10,
            clearance_epsilon_m: 0.0,
            roadmap: &roadmap,
        };
        let first = roadmap
            .repair_conflicts(&first_request)
            .expect("first window succeeds")
            .expect("first window has a route");
        let second_request = ConflictRepairRequest {
            agents: vec![CoordinationAgentRequest {
                agent_id: "b".to_owned(),
                start_node: 2,
                radius_m: 0.2,
                earliest_start_s: 0.0,
                speed_mps: 1.0,
                task: CoordinationAgentTask::Goal {
                    goal_node: 0,
                    reserve_until_s: 4.0,
                },
            }],
            occupied_trajectories: &first.trajectories,
            timestep_s: 0.5,
            maximum_low_level_expansions: 100,
            maximum_conflict_tree_nodes: 10,
            clearance_epsilon_m: 0.0,
            roadmap: &roadmap,
        };
        let second = roadmap
            .repair_conflicts(&second_request)
            .expect("second window succeeds")
            .expect("second window has a route");
        let mut combined = first.trajectories;
        combined.extend(second.trajectories);

        assert!(
            timed_disc_conflicts(&combined, 0.0)
                .expect("valid handoff trajectories")
                .is_empty()
        );
    }

    #[test]
    fn queue_grid_slot_windows_follow_activation_and_fifo_service_handoffs() {
        let windows = queue_grid_slot_windows(
            3,
            &[
                QueueGridTicketActivation {
                    ticket: 1,
                    at_s: 0.0,
                },
                QueueGridTicketActivation {
                    ticket: 2,
                    at_s: 0.0,
                },
                QueueGridTicketActivation {
                    ticket: 3,
                    at_s: 4.0,
                },
            ],
            &[
                QueueGridServiceDeparture {
                    ticket: 1,
                    at_s: 5.0,
                },
                QueueGridServiceDeparture {
                    ticket: 2,
                    at_s: 7.0,
                },
            ],
            9.0,
        )
        .expect("valid FIFO handoffs");

        assert_eq!(windows.len(), 6);
        assert_eq!(windows[0].ticket, 1);
        assert_eq!(windows[0].slot_rank, 0);
        assert!((windows[0].starts_at_s - 0.0).abs() < f64::EPSILON);
        assert!((windows[0].ends_at_s - 5.0).abs() < f64::EPSILON);
        assert_eq!(windows[1].ticket, 2);
        assert_eq!(windows[1].slot_rank, 1);
        assert_eq!(windows[2].ticket, 2);
        assert_eq!(windows[2].slot_rank, 0);
        assert_eq!(windows[3].ticket, 3);
        assert_eq!(windows[3].slot_rank, 2);
        assert_eq!(windows[4].ticket, 3);
        assert_eq!(windows[4].slot_rank, 1);
        assert_eq!(windows[5].ticket, 3);
        assert_eq!(windows[5].slot_rank, 0);
        assert!((windows[5].ends_at_s - 9.0).abs() < f64::EPSILON);
    }

    #[test]
    fn queue_grid_slot_windows_reject_non_fifo_service() {
        assert_eq!(
            queue_grid_slot_windows(
                2,
                &[
                    QueueGridTicketActivation {
                        ticket: 1,
                        at_s: 0.0,
                    },
                    QueueGridTicketActivation {
                        ticket: 2,
                        at_s: 0.0,
                    },
                ],
                &[QueueGridServiceDeparture {
                    ticket: 2,
                    at_s: 1.0,
                }],
                2.0,
            ),
            Err(CoordinationError::NonFifoQueueGridDeparture { ticket: 2 })
        );
    }

    #[test]
    fn queue_grid_timed_targets_preserve_a_ticket_rank_handoff_order() {
        let windows = queue_grid_slot_windows(
            3,
            &[
                QueueGridTicketActivation {
                    ticket: 1,
                    at_s: 0.0,
                },
                QueueGridTicketActivation {
                    ticket: 2,
                    at_s: 0.0,
                },
                QueueGridTicketActivation {
                    ticket: 3,
                    at_s: 4.0,
                },
            ],
            &[
                QueueGridServiceDeparture {
                    ticket: 1,
                    at_s: 5.0,
                },
                QueueGridServiceDeparture {
                    ticket: 2,
                    at_s: 7.0,
                },
            ],
            9.0,
        )
        .expect("valid FIFO windows");

        let targets = queue_grid_timed_targets(3, &windows, &[10, 11, 12])
            .expect("every authored rank is bound to a roadmap node");

        assert_eq!(
            targets,
            vec![
                TimedRoadmapTarget {
                    node: 12,
                    starts_at_s: 4.0,
                    ends_at_s: 5.0,
                },
                TimedRoadmapTarget {
                    node: 11,
                    starts_at_s: 5.0,
                    ends_at_s: 7.0,
                },
                TimedRoadmapTarget {
                    node: 10,
                    starts_at_s: 7.0,
                    ends_at_s: 9.0,
                },
            ]
        );
    }

    #[test]
    fn uncalibrated_service_assumption_generates_active_fifo_departures() {
        let tickets = vec![
            QueueGridTicketRequest {
                ticket: 1,
                agent_id: "ticket:1".to_owned(),
                start_node: 0,
                radius_m: 0.3,
                activation_at_s: 0.0,
                speed_mps: 1.0,
            },
            QueueGridTicketRequest {
                ticket: 2,
                agent_id: "ticket:2".to_owned(),
                start_node: 0,
                radius_m: 0.3,
                activation_at_s: 0.0,
                speed_mps: 1.0,
            },
            QueueGridTicketRequest {
                ticket: 3,
                agent_id: "ticket:3".to_owned(),
                start_node: 0,
                radius_m: 0.3,
                activation_at_s: 4.0,
                speed_mps: 1.0,
            },
        ];

        let departures = estimate_queue_grid_departures(
            &tickets,
            QueueGridServiceAssumption {
                first_departure_at_s: 5.0,
                headway_s: 2.0,
            },
            8.0,
        )
        .expect("valid uncalibrated assumption");

        assert_eq!(
            departures,
            vec![
                QueueGridServiceDeparture {
                    ticket: 1,
                    at_s: 5.0,
                },
                QueueGridServiceDeparture {
                    ticket: 2,
                    at_s: 7.0,
                },
            ]
        );
    }

    #[test]
    fn uncalibrated_service_assumption_uses_the_active_head_not_future_ticket_order() {
        let tickets = vec![
            QueueGridTicketRequest {
                ticket: 1,
                agent_id: "ticket:1".to_owned(),
                start_node: 0,
                radius_m: 0.3,
                activation_at_s: 5.0,
                speed_mps: 1.0,
            },
            QueueGridTicketRequest {
                ticket: 2,
                agent_id: "ticket:2".to_owned(),
                start_node: 0,
                radius_m: 0.3,
                activation_at_s: 0.0,
                speed_mps: 1.0,
            },
        ];

        let departures = estimate_queue_grid_departures(
            &tickets,
            QueueGridServiceAssumption {
                first_departure_at_s: 1.0,
                headway_s: 2.0,
            },
            8.0,
        )
        .expect("valid uncalibrated assumption");

        assert_eq!(departures[0].ticket, 2);
        assert_eq!(departures[1].ticket, 1);
        assert!((departures[1].at_s - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn queue_grid_timed_targets_reject_an_unbound_slot_rank() {
        let windows = [QueueGridSlotWindow {
            ticket: 1,
            starts_at_s: 0.0,
            ends_at_s: 1.0,
            slot_rank: 1,
        }];

        assert_eq!(
            queue_grid_timed_targets(1, &windows, &[10]),
            Err(CoordinationError::UnboundQueueGridSlotRank {
                ticket: 1,
                slot_rank: 1,
            })
        );
    }

    #[test]
    fn queue_grid_pipeline_binds_fifo_windows_and_repairs_the_result() {
        let roadmap = roadmap();
        let request = QueueGridCoordinationRequest {
            slot_nodes: &[2],
            tickets: vec![QueueGridTicketRequest {
                ticket: 1,
                agent_id: "ticket:1".to_owned(),
                start_node: 0,
                radius_m: 0.3,
                activation_at_s: 0.0,
                speed_mps: 1.0,
            }],
            departures: Vec::new(),
            horizon_s: 4.0,
            occupied_trajectories: &[],
            timestep_s: 0.5,
            maximum_low_level_expansions: 100,
            maximum_conflict_tree_nodes: 10,
            clearance_epsilon_m: 0.0,
            roadmap: &roadmap,
        };

        let plan = plan_queue_grid(&request)
            .expect("valid queue-grid coordination request")
            .expect("single ticket reaches the authored head slot");

        assert_eq!(plan.slot_windows.len(), 1);
        assert_eq!(plan.slot_windows[0].ticket, 1);
        assert_eq!(plan.repair_plan.trajectories.len(), 1);
        assert!(
            timed_disc_conflicts(&plan.repair_plan.trajectories, 0.0)
                .expect("valid planned trajectory")
                .is_empty()
        );
    }

    #[test]
    fn queue_grid_pipeline_replans_a_ticket_after_the_head_departs() {
        let roadmap = repair_roadmap();
        let request = QueueGridCoordinationRequest {
            // Rank zero is node 2, rank one is node 0. The two tickets begin
            // at the opposing endpoint and must form, then advance after the
            // FIFO head's declared service departure.
            slot_nodes: &[2, 0],
            tickets: vec![
                QueueGridTicketRequest {
                    ticket: 1,
                    agent_id: "ticket:1".to_owned(),
                    start_node: 0,
                    radius_m: 0.2,
                    activation_at_s: 0.0,
                    speed_mps: 1.0,
                },
                QueueGridTicketRequest {
                    ticket: 2,
                    agent_id: "ticket:2".to_owned(),
                    start_node: 2,
                    radius_m: 0.2,
                    activation_at_s: 0.0,
                    speed_mps: 1.0,
                },
            ],
            departures: vec![QueueGridServiceDeparture {
                ticket: 1,
                at_s: 4.0,
            }],
            horizon_s: 6.0,
            occupied_trajectories: &[],
            timestep_s: 0.5,
            maximum_low_level_expansions: 100,
            maximum_conflict_tree_nodes: 10,
            clearance_epsilon_m: 0.0,
            roadmap: &roadmap,
        };

        let plan = plan_queue_grid(&request)
            .expect("valid queue-grid coordination request")
            .expect("bounded repair finds a dynamic FIFO plan");
        let rolling_plan = plan_queue_grid_rolling(&QueueGridRollingCoordinationRequest {
            queue: request.clone(),
            maximum_tickets_per_cohort: 1,
        })
        .expect("valid rolling queue-grid coordination request")
        .expect("bounded rolling repair finds a dynamic FIFO plan");

        let second_ticket_windows = plan
            .slot_windows
            .iter()
            .filter(|window| window.ticket == 2)
            .collect::<Vec<_>>();
        assert_eq!(second_ticket_windows.len(), 2);
        assert_eq!(second_ticket_windows[0].slot_rank, 1);
        assert_eq!(second_ticket_windows[1].slot_rank, 0);
        assert!(
            timed_disc_conflicts(&plan.repair_plan.trajectories, 0.0)
                .expect("valid planned trajectories")
                .is_empty()
        );
        assert!(
            timed_disc_conflicts(&rolling_plan.repair_plan.trajectories, 0.0)
                .expect("valid rolling planned trajectories")
                .is_empty()
        );
    }

    #[test]
    fn lattice_roadmap_retains_exact_anchors_and_static_clearance() {
        let surface = lattice_surface();
        let obstacles = vec![Obstacle {
            id: "column".to_owned(),
            surface: "concourse".to_owned(),
            at: Point3 {
                x_m: 1.5,
                y_m: 1.5,
                z_m: 0.0,
            },
            width_m: 1.0,
            depth_m: 1.0,
        }];
        let anchors = [
            Point3 {
                x_m: 0.5,
                y_m: 0.5,
                z_m: 0.0,
            },
            Point3 {
                x_m: 3.5,
                y_m: 3.5,
                z_m: 0.0,
            },
        ];
        let lattice = CoordinationRoadmap::lattice(&surface, &obstacles, 0.2, 1.0, 100, &anchors)
            .expect("bounded clear lattice");

        assert_eq!(lattice.anchor_nodes.len(), anchors.len());
        for (anchor, &node_index) in anchors.iter().zip(&lattice.anchor_nodes) {
            assert_eq!(lattice.roadmap.nodes[node_index].position, *anchor);
        }
        for node in &lattice.roadmap.nodes {
            assert!(point_has_static_clearance(
                node.position,
                &surface,
                &obstacles,
                0.2
            ));
        }
        for (first_node, neighbours) in lattice.roadmap.adjacency.iter().enumerate() {
            for &second_node in neighbours {
                assert!(segment_has_static_clearance(
                    lattice.roadmap.nodes[first_node].position,
                    lattice.roadmap.nodes[second_node].position,
                    &surface,
                    &obstacles,
                    0.2,
                ));
            }
        }
    }

    #[test]
    fn lattice_roadmap_reports_its_node_bound() {
        let surface = lattice_surface();

        assert_eq!(
            CoordinationRoadmap::lattice(&surface, &[], 0.2, 1.0, 1, &[]),
            Err(CoordinationError::RoadmapNodeBoundExceeded { maximum_nodes: 1 })
        );
    }

    #[test]
    fn dense_queue_grid_batch_finds_a_reference_clear_plan() {
        let surface = Surface {
            id: "concourse".to_owned(),
            origin: Point3 {
                x_m: 0.0,
                y_m: 0.0,
                z_m: 0.0,
            },
            width_m: 40.0,
            depth_m: 16.0,
        };
        let starts = (0..8)
            .map(|index| Point3 {
                x_m: 1.0 + f64::from(index) * 0.63,
                y_m: 1.0,
                z_m: 0.0,
            })
            .collect::<Vec<_>>();
        let goals = (0..8)
            .map(|rank| Point3 {
                x_m: 29.0 - 27.0 * f64::from(rank) / 37.0,
                y_m: 11.0,
                z_m: 0.0,
            })
            .collect::<Vec<_>>();
        let anchors = starts.iter().chain(&goals).copied().collect::<Vec<_>>();
        let lattice = CoordinationRoadmap::lattice(&surface, &[], 0.3, 1.2, 2_000, &anchors)
            .expect("clear queue-grid formation roadmap");
        let request = ConflictRepairRequest {
            agents: (0..8)
                .map(|index| CoordinationAgentRequest {
                    agent_id: format!("passengers:{index}"),
                    start_node: lattice.anchor_nodes[index],
                    radius_m: 0.3,
                    earliest_start_s: 0.0,
                    speed_mps: 1.2,
                    task: CoordinationAgentTask::Goal {
                        goal_node: lattice.anchor_nodes[index + 8],
                        reserve_until_s: 40.0,
                    },
                })
                .collect(),
            occupied_trajectories: &[],
            timestep_s: 0.1,
            maximum_low_level_expansions: 100_000,
            maximum_conflict_tree_nodes: 500,
            clearance_epsilon_m: 0.0,
            roadmap: &lattice.roadmap,
        };

        let plan = lattice
            .roadmap
            .repair_conflicts(&request)
            .expect("bounded search does not exhaust")
            .expect("queue-grid batch has a lattice plan");
        assert!(
            timed_disc_conflicts(&plan.trajectories, 0.0)
                .expect("valid trajectories")
                .is_empty()
        );
    }

    #[test]
    fn dense_queue_grid_pipeline_handles_multiple_fifo_rank_handoffs() {
        let surface = Surface {
            id: "concourse".to_owned(),
            origin: Point3 {
                x_m: 0.0,
                y_m: 0.0,
                z_m: 0.0,
            },
            width_m: 40.0,
            depth_m: 16.0,
        };
        let starts = (0..8)
            .map(|index| Point3 {
                x_m: 1.0 + f64::from(index) * 0.63,
                y_m: 1.0,
                z_m: 0.0,
            })
            .collect::<Vec<_>>();
        let slots = (0..8)
            .map(|rank| Point3 {
                x_m: 29.0 - 27.0 * f64::from(rank) / 37.0,
                y_m: 11.0,
                z_m: 0.0,
            })
            .collect::<Vec<_>>();
        let anchors = starts.iter().chain(&slots).copied().collect::<Vec<_>>();
        let lattice = CoordinationRoadmap::lattice(&surface, &[], 0.3, 1.2, 2_000, &anchors)
            .expect("clear queue-grid formation roadmap");
        let tickets = (0..8)
            .map(|index| QueueGridTicketRequest {
                ticket: (index + 1).try_into().expect("small test ticket fits u64"),
                agent_id: format!("passengers:{index}"),
                start_node: lattice.anchor_nodes[index],
                radius_m: 0.3,
                activation_at_s: 0.0,
                speed_mps: 1.2,
            })
            .collect::<Vec<_>>();
        let departures = estimate_queue_grid_departures(
            &tickets,
            QueueGridServiceAssumption {
                first_departure_at_s: 35.0,
                headway_s: 2.0,
            },
            51.0,
        )
        .expect("valid exploratory service assumption");
        let request = QueueGridCoordinationRequest {
            slot_nodes: &lattice.anchor_nodes[8..],
            tickets,
            departures,
            horizon_s: 51.0,
            occupied_trajectories: &[],
            timestep_s: 0.1,
            maximum_low_level_expansions: 100_000,
            maximum_conflict_tree_nodes: 1_000,
            clearance_epsilon_m: 0.0,
            roadmap: &lattice.roadmap,
        };

        let plan = plan_queue_grid(&request)
            .expect("bounded search does not exhaust")
            .expect("eight-ticket queue-grid has a dynamic FIFO plan");
        let rolling_plan = plan_queue_grid_rolling(&QueueGridRollingCoordinationRequest {
            queue: request.clone(),
            maximum_tickets_per_cohort: 2,
        })
        .expect("bounded rolling search does not exhaust")
        .expect("eight-ticket queue-grid has a rolling dynamic FIFO plan");

        assert_eq!(plan.slot_windows.len(), 36);
        assert!(
            timed_disc_conflicts(&plan.repair_plan.trajectories, 0.0)
                .expect("valid planned trajectories")
                .is_empty()
        );
        assert!(
            timed_disc_conflicts(&rolling_plan.repair_plan.trajectories, 0.0)
                .expect("valid rolling planned trajectories")
                .is_empty()
        );
    }

    #[test]
    #[ignore = "runs the 152-agent queue-grid planning stress case explicitly"]
    fn queue_grid_stress_source_reports_its_first_unformable_rolling_cohort() {
        let scenario = crate::parse(include_str!(
            "../../../examples/experiments/queue-grid-stress.chy"
        ))
        .expect("stress source parses");
        let group = scenario
            .agents
            .first()
            .expect("stress source has one group");
        let footprint = scenario
            .queue_footprints
            .first()
            .expect("stress source has one queue grid");
        let surface = scenario
            .surfaces
            .iter()
            .find(|surface| surface.id == footprint.surface)
            .expect("queue grid surface exists");
        let starts = group.spawn_positions().collect::<Vec<_>>();
        let slots = (0..footprint.slots)
            .map(|rank| footprint.position(rank))
            .collect::<Vec<_>>();
        let anchors = starts.iter().chain(&slots).copied().collect::<Vec<_>>();
        let lattice = CoordinationRoadmap::lattice(
            surface,
            &scenario.obstacles,
            group.radius_m,
            0.6,
            3_000,
            &anchors,
        )
        .expect("bounded static roadmap exists for stress source");
        let tickets = starts
            .iter()
            .enumerate()
            .map(|(index, _)| QueueGridTicketRequest {
                ticket: (index + 1)
                    .try_into()
                    .expect("stress ticket ordinal fits u64"),
                agent_id: format!("{}:{index}", group.id),
                start_node: lattice.anchor_nodes[index],
                radius_m: group.radius_m,
                activation_at_s: group
                    .release_time_for(index.try_into().expect("stress ordinal fits u32")),
                speed_mps: group.speed_mps,
            })
            .collect::<Vec<_>>();
        // This is an explicit exploratory assumption, deliberately slower
        // than the source's abstract 25/s token rate so a 0.3m reference disc
        // can advance through adjacent authored slots on a 1.2m/s roadmap.
        let departures = estimate_queue_grid_departures(
            &tickets,
            QueueGridServiceAssumption {
                first_departure_at_s: 135.0,
                headway_s: 4.0,
            },
            scenario.duration_s,
        )
        .expect("valid exploratory service schedule");
        let outcome = assess_queue_grid_rolling(&QueueGridRollingCoordinationRequest {
            queue: QueueGridCoordinationRequest {
                slot_nodes: &lattice.anchor_nodes[starts.len()..],
                tickets,
                departures,
                horizon_s: scenario.duration_s,
                occupied_trajectories: &[],
                // The planner grid is a bounded search policy, separate from
                // the source runtime's 100ms integration cadence. Every
                // emitted move remains an exact continuous-time disc check.
                timestep_s: 0.5,
                maximum_low_level_expansions: 100_000,
                maximum_conflict_tree_nodes: 1_000,
                clearance_epsilon_m: reference_clearance_epsilon_m(),
                roadmap: &lattice.roadmap,
            },
            maximum_tickets_per_cohort: 8,
        })
        .expect("bounded rolling stress search does not exhaust");

        assert_eq!(
            outcome,
            QueueGridRollingOutcome::NoPlan {
                cohort_tickets: vec![144, 143, 142, 141, 140, 139, 138, 137],
            }
        );
    }
}
