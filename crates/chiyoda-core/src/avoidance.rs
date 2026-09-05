//! Deterministic, two-dimensional ORCA velocity selection.
//!
//! This is the agent-agent portion of Optimal Reciprocal Collision Avoidance
//! (ORCA). The runtime supplies one preferred velocity from its global
//! visibility-graph planner, snapshots all on-surface agents, and asks this
//! module for a velocity that is as close as possible to that preference while
//! satisfying the reciprocal velocity constraints from that snapshot. An
//! explicit queue-grid ticket is the narrow exception: an earlier FIFO member
//! has deterministic right-of-way over a later member of the same grid.
//!
//! The implementation intentionally uses `f64`, a stable identifier order,
//! and a deterministic direction for exact co-location. Those choices matter
//! because Chiyoda persists replayable run bundles. ORCA's formal assumptions
//! do not turn the resulting reference model into a calibrated crowd model.

use std::cmp::Ordering;

const EPSILON: f64 = 1e-9;

/// A two-dimensional vector in the scenario's metre coordinate system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Vec2 {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl Vec2 {
    pub(crate) const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub(crate) fn dot(self, other: Self) -> f64 {
        self.x.mul_add(other.x, self.y * other.y)
    }

    fn cross(self, other: Self) -> f64 {
        self.x.mul_add(other.y, -(self.y * other.x))
    }

    pub(crate) fn length_squared(self) -> f64 {
        self.dot(self)
    }

    fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    fn normalized(self) -> Option<Self> {
        let length = self.length();
        (length.is_finite() && length > EPSILON).then_some(self / length)
    }

    fn perpendicular(self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }

    fn project_onto_unit(self, direction: Self) -> Self {
        direction * self.dot(direction)
    }
}

impl std::ops::Add for Vec2 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl std::ops::AddAssign for Vec2 {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl std::ops::Mul<f64> for Vec2 {
    type Output = Self;

    fn mul(self, scalar: f64) -> Self {
        Self {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }
}

impl std::ops::Div<f64> for Vec2 {
    type Output = Self;

    fn div(self, scalar: f64) -> Self {
        Self {
            x: self.x / scalar,
            y: self.y / scalar,
        }
    }
}

impl std::ops::Neg for Vec2 {
    type Output = Self;

    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}

/// The state of one agent at the beginning of a local-motion step.
#[derive(Debug, Clone)]
pub(crate) struct AvoidanceAgent {
    /// A stable, unique runtime identifier used only for deterministic ties.
    pub(crate) id: String,
    pub(crate) position: Vec2,
    pub(crate) velocity: Vec2,
    pub(crate) radius_m: f64,
    pub(crate) max_speed_mps: f64,
    /// An explicit FIFO grid assignment. Only agents in the same authored
    /// queue grid use this to choose an asymmetric responsibility split.
    pub(crate) queue_priority: Option<QueueAvoidancePriority>,
}

/// Stable queue information supplied by the runtime's immutable local-motion
/// snapshot. It is absent for ordinary motion and legacy line footprints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QueueAvoidancePriority {
    pub(crate) resource: String,
    pub(crate) ticket: u64,
}

/// The result of one ORCA velocity selection.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AvoidanceDecision {
    pub(crate) velocity: Vec2,
    /// `true` means the snapshot's constraints could not all be met under the
    /// supplied speed bound, which can occur in dense, competing motion as
    /// well as when agents begin already interpenetrating.
    pub(crate) constraint_fallback: bool,
}

#[derive(Debug, Clone, Copy)]
struct Line {
    point: Vec2,
    /// Unit direction; the counter-clockwise side is feasible.
    direction: Vec2,
}

/// Select the closest speed-bounded velocity satisfying the applicable ORCA
/// half-planes induced by `agents`. The caller must supply a same-surface
/// snapshot and a valid `own_index` into that snapshot. A later grid ticket's
/// constraint is deliberately absent from an earlier ticket's program.
///
/// The selector solves the small two-dimensional convex program by evaluating
/// the preference's projection on every feasible boundary feature (interior,
/// line, circle, and line intersections). This makes the tie rule explicit
/// and avoids dependence on an unordered or randomized numerical solver.
#[must_use]
pub(crate) fn choose_velocity(
    agents: &[AvoidanceAgent],
    own_index: usize,
    preferred_velocity: Vec2,
    max_speed_mps: f64,
    time_horizon_s: f64,
    timestep_s: f64,
) -> AvoidanceDecision {
    debug_assert!(own_index < agents.len());
    debug_assert!(max_speed_mps.is_finite() && max_speed_mps >= 0.0);
    debug_assert!(time_horizon_s.is_finite() && time_horizon_s > 0.0);
    debug_assert!(timestep_s.is_finite() && timestep_s > 0.0);

    let own = &agents[own_index];
    let mut neighbours: Vec<_> = agents
        .iter()
        .enumerate()
        .filter(|(index, neighbour)| {
            *index != own_index && !queue_grid_right_of_way(own, neighbour)
        })
        .collect();
    neighbours.sort_unstable_by(|(_, left), (_, right)| left.id.cmp(&right.id));
    let lines: Vec<_> = neighbours
        .into_iter()
        .map(|(_, neighbour)| orca_line(own, neighbour, time_horizon_s, timestep_s))
        .collect();

    let preferred_velocity = clamp_to_circle(preferred_velocity, max_speed_mps);
    match solve_linear_program(&lines, max_speed_mps, preferred_velocity) {
        Ok(velocity) => AvoidanceDecision {
            velocity,
            constraint_fallback: false,
        },
        Err((failed_line, partial_velocity)) => AvoidanceDecision {
            velocity: solve_relaxed_linear_program(
                &lines,
                max_speed_mps,
                failed_line,
                partial_velocity,
            ),
            constraint_fallback: true,
        },
    }
}

fn orca_line(
    own: &AvoidanceAgent,
    neighbour: &AvoidanceAgent,
    time_horizon_s: f64,
    timestep_s: f64,
) -> Line {
    let relative_position = neighbour.position - own.position;
    let relative_velocity = own.velocity - neighbour.velocity;
    let distance_squared = relative_position.length_squared();
    let combined_radius = own.radius_m + neighbour.radius_m;
    let combined_radius_squared = combined_radius * combined_radius;

    let (normal, projected_relative_velocity, inside_velocity_obstacle) =
        if distance_squared > combined_radius_squared {
            let cutoff_center = relative_position / time_horizon_s;
            let from_cutoff_center = relative_velocity - cutoff_center;
            let from_cutoff_center_squared = from_cutoff_center.length_squared();
            let dot = from_cutoff_center.dot(relative_position);
            if dot < 0.0 && dot * dot > combined_radius_squared * from_cutoff_center_squared {
                let normal = from_cutoff_center
                    .normalized()
                    .unwrap_or_else(|| pair_direction(&own.id, &neighbour.id));
                let cutoff_radius = combined_radius / time_horizon_s;
                (
                    normal,
                    cutoff_center + normal * cutoff_radius,
                    from_cutoff_center_squared < cutoff_radius * cutoff_radius,
                )
            } else {
                let tangent_leg = (distance_squared - combined_radius_squared).sqrt();
                let side = relative_position.cross(from_cutoff_center).signum();
                let shadow_direction = (relative_position * (tangent_leg * side)
                    + relative_position.perpendicular() * combined_radius)
                    / distance_squared;
                let normal = shadow_direction.perpendicular();
                (
                    normal,
                    relative_velocity.project_onto_unit(shadow_direction),
                    relative_velocity.cross(shadow_direction) >= 0.0,
                )
            }
        } else {
            let cutoff_center = relative_position / timestep_s;
            let cutoff_radius = combined_radius / timestep_s;
            let from_cutoff_center = relative_velocity - cutoff_center;
            let normal = from_cutoff_center
                .normalized()
                .unwrap_or_else(|| pair_direction(&own.id, &neighbour.id));
            (normal, cutoff_center + normal * cutoff_radius, true)
        };

    let correction = projected_relative_velocity - relative_velocity;
    let responsibility = if inside_velocity_obstacle {
        queue_grid_responsibility(own, neighbour)
    } else {
        1.0
    };
    Line {
        point: own.velocity + correction * responsibility,
        direction: -normal.perpendicular(),
    }
}

fn queue_grid_responsibility(own: &AvoidanceAgent, neighbour: &AvoidanceAgent) -> f64 {
    match (&own.queue_priority, &neighbour.queue_priority) {
        (Some(own_priority), Some(neighbour_priority))
            if own_priority.resource == neighbour_priority.resource
                && own_priority.ticket > neighbour_priority.ticket =>
        {
            1.0
        }
        _ => 0.5,
    }
}

fn queue_grid_right_of_way(own: &AvoidanceAgent, neighbour: &AvoidanceAgent) -> bool {
    // An earlier grid ticket chooses against all unrelated neighbours as
    // usual, but does not yield to a later ticket for the same authored grid.
    // The later ticket receives the full inside-obstacle correction above.
    matches!(
        (&own.queue_priority, &neighbour.queue_priority),
        (Some(own_priority), Some(neighbour_priority))
            if own_priority.resource == neighbour_priority.resource
                && own_priority.ticket < neighbour_priority.ticket
    )
}

fn clamp_to_circle(vector: Vec2, radius: f64) -> Vec2 {
    let length_squared = vector.length_squared();
    let radius_squared = radius * radius;
    if length_squared <= radius_squared {
        vector
    } else {
        vector * (radius / length_squared.sqrt())
    }
}

#[derive(Debug, Clone, Copy)]
enum Optimization {
    Point(Vec2),
    Direction(Vec2),
}

enum LinearProgramResult {
    Feasible(Vec2),
    Infeasible {
        failed_line: usize,
        partial_velocity: Vec2,
    },
}

fn solve_linear_program(
    lines: &[Line],
    radius: f64,
    preferred: Vec2,
) -> Result<Vec2, (usize, Vec2)> {
    match solve_2d(lines, radius, Optimization::Point(preferred)) {
        LinearProgramResult::Feasible(velocity) => Ok(velocity),
        LinearProgramResult::Infeasible {
            failed_line,
            partial_velocity,
        } => Err((failed_line, partial_velocity)),
    }
}

fn solve_2d(lines: &[Line], radius: f64, optimization: Optimization) -> LinearProgramResult {
    let mut best = match optimization {
        Optimization::Point(point) => clamp_to_circle(point, radius),
        Optimization::Direction(direction) => direction * radius,
    };
    for (index, line) in lines.iter().enumerate() {
        if line.direction.cross(best - line.point) >= -EPSILON {
            continue;
        }
        match solve_along_line(*line, radius, &lines[..index], optimization) {
            Some(velocity) => best = velocity,
            None => {
                return LinearProgramResult::Infeasible {
                    failed_line: index,
                    partial_velocity: best,
                };
            }
        }
    }
    LinearProgramResult::Feasible(best)
}

fn solve_along_line(
    line: Line,
    radius: f64,
    constraints: &[Line],
    optimization: Optimization,
) -> Option<Vec2> {
    let point_projection = line.point.dot(line.direction);
    let discriminant = point_projection.mul_add(
        point_projection,
        radius.mul_add(radius, -line.point.length_squared()),
    );
    if discriminant < -EPSILON {
        return None;
    }
    let offset = discriminant.max(0.0).sqrt();
    let mut left = -point_projection - offset;
    let mut right = -point_projection + offset;
    for constraint in constraints {
        let determinant = line.direction.cross(constraint.direction);
        let numerator = constraint.direction.cross(line.point - constraint.point);
        if determinant.abs() <= EPSILON {
            if numerator < -EPSILON {
                return None;
            }
            continue;
        }
        let point = numerator / determinant;
        if determinant >= 0.0 {
            right = right.min(point);
        } else {
            left = left.max(point);
        }
        if left > right + EPSILON {
            return None;
        }
    }
    if left > right {
        let midpoint = left.midpoint(right);
        left = midpoint;
        right = midpoint;
    }
    let location = match optimization {
        Optimization::Point(point) => (point - line.point).dot(line.direction).clamp(left, right),
        Optimization::Direction(direction) if direction.dot(line.direction) > 0.0 => right,
        Optimization::Direction(_) => left,
    };
    Some(line.point + line.direction * location)
}

fn solve_relaxed_linear_program(
    lines: &[Line],
    radius: f64,
    failed_line: usize,
    partial_velocity: Vec2,
) -> Vec2 {
    let mut penetration = 0.0;
    let mut best = partial_velocity;
    for (offset, line) in lines[failed_line..].iter().enumerate() {
        if line.direction.cross(line.point - best) <= penetration + EPSILON {
            continue;
        }
        let index = failed_line + offset;
        let mut derived = Vec::with_capacity(index);
        for previous in &lines[..index] {
            let determinant = line.direction.cross(previous.direction);
            let point = if determinant.abs() <= EPSILON {
                if line.direction.dot(previous.direction) > 0.0 {
                    continue;
                }
                (line.point + previous.point) * 0.5
            } else {
                let distance = previous.direction.cross(line.point - previous.point) / determinant;
                line.point + line.direction * distance
            };
            let Some(direction) = (previous.direction - line.direction).normalized() else {
                continue;
            };
            derived.push(Line { point, direction });
        }
        if let LinearProgramResult::Feasible(velocity) = solve_2d(
            &derived,
            radius,
            Optimization::Direction(line.direction.perpendicular()),
        ) {
            best = velocity;
            penetration = line.direction.cross(line.point - best);
        }
    }
    best
}

fn pair_direction(first_id: &str, second_id: &str) -> Vec2 {
    let (first, second, sign) = match first_id.cmp(second_id) {
        Ordering::Less => (first_id, second_id, 1.0),
        Ordering::Greater => (second_id, first_id, -1.0),
        Ordering::Equal => return Vec2 { x: 1.0, y: 0.0 },
    };
    let hash = first
        .bytes()
        .chain(std::iter::once(0))
        .chain(second.bytes())
        .fold(0xcbf2_9ce4_8422_2325_u64, |state, byte| {
            (state ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    let direction = match hash % 8 {
        0 => Vec2 { x: 1.0, y: 0.0 },
        1 => Vec2 {
            x: std::f64::consts::FRAC_1_SQRT_2,
            y: std::f64::consts::FRAC_1_SQRT_2,
        },
        2 => Vec2 { x: 0.0, y: 1.0 },
        3 => Vec2 {
            x: -std::f64::consts::FRAC_1_SQRT_2,
            y: std::f64::consts::FRAC_1_SQRT_2,
        },
        4 => Vec2 { x: -1.0, y: 0.0 },
        5 => Vec2 {
            x: -std::f64::consts::FRAC_1_SQRT_2,
            y: -std::f64::consts::FRAC_1_SQRT_2,
        },
        6 => Vec2 { x: 0.0, y: -1.0 },
        _ => Vec2 {
            x: std::f64::consts::FRAC_1_SQRT_2,
            y: -std::f64::consts::FRAC_1_SQRT_2,
        },
    };
    direction * sign
}

#[cfg(test)]
mod tests {
    use super::{AvoidanceAgent, QueueAvoidancePriority, Vec2, choose_velocity};

    fn agent(id: &str, x: f64, y: f64, vx: f64, vy: f64) -> AvoidanceAgent {
        AvoidanceAgent {
            id: id.to_owned(),
            position: Vec2 { x, y },
            velocity: Vec2 { x: vx, y: vy },
            radius_m: 0.3,
            max_speed_mps: 1.0,
            queue_priority: None,
        }
    }

    #[test]
    fn head_on_agents_choose_safe_reciprocal_velocities() {
        let agents = [
            agent("left", -1.0, 0.0, 1.0, 0.0),
            agent("right", 1.0, 0.0, -1.0, 0.0),
        ];
        let left = choose_velocity(&agents, 0, Vec2 { x: 1.0, y: 0.0 }, 1.0, 2.5, 0.5);
        let right = choose_velocity(&agents, 1, Vec2 { x: -1.0, y: 0.0 }, 1.0, 2.5, 0.5);
        assert!(!left.constraint_fallback);
        assert!(!right.constraint_fallback);
        let left_next = agents[0].position + left.velocity * 0.5;
        let right_next = agents[1].position + right.velocity * 0.5;
        assert!((left_next - right_next).length() >= 0.6 - 1e-9);
    }

    #[test]
    fn coincident_agents_use_a_reproducible_non_random_tie() {
        let agents = [
            agent("alpha", 0.0, 0.0, 0.0, 0.0),
            agent("beta", 0.0, 0.0, 0.0, 0.0),
        ];
        let first = choose_velocity(&agents, 0, Vec2 { x: 1.0, y: 0.0 }, 1.0, 2.5, 1.0);
        let repeated = choose_velocity(&agents, 0, Vec2 { x: 1.0, y: 0.0 }, 1.0, 2.5, 1.0);
        let other = choose_velocity(&agents, 1, Vec2 { x: -1.0, y: 0.0 }, 1.0, 2.5, 1.0);
        assert_eq!(first.velocity, repeated.velocity);
        assert!(first.velocity.x.is_finite() && first.velocity.y.is_finite());
        assert!(other.velocity.x.is_finite() && other.velocity.y.is_finite());
        let first_next = agents[0].position + first.velocity;
        let other_next = agents[1].position + other.velocity;
        assert!((first_next - other_next).length() >= 0.6 - 1e-9);
    }

    #[test]
    fn incompatible_overlapping_constraints_use_the_documented_fallback() {
        let agents = [
            agent("alpha", 0.0, 0.0, 0.0, 0.0),
            agent("beta", 0.0, 0.0, 0.0, 0.0),
            agent("gamma", 0.0, 0.0, 0.0, 0.0),
            agent("delta", 0.0, 0.0, 0.0, 0.0),
            agent("epsilon", 0.0, 0.0, 0.0, 0.0),
            agent("zeta", 0.0, 0.0, 0.0, 0.0),
        ];
        let first = choose_velocity(&agents, 0, Vec2::ZERO, 1.0, 2.5, 1.0);
        let repeated = choose_velocity(&agents, 0, Vec2::ZERO, 1.0, 2.5, 1.0);
        assert!(first.constraint_fallback);
        assert_eq!(first.velocity, repeated.velocity);
        assert!(first.velocity.x.is_finite() && first.velocity.y.is_finite());
        assert!(first.velocity.length() <= 1.0 + 1e-9);
    }

    #[test]
    fn earlier_grid_ticket_keeps_right_of_way_over_later_ticket() {
        let mut leader = agent("leader", 0.0, 0.0, 0.0, 0.0);
        leader.queue_priority = Some(QueueAvoidancePriority {
            resource: "gate:fare_gate".to_owned(),
            ticket: 4,
        });
        let mut follower = agent("follower", 0.4, 0.0, 0.0, 0.0);
        follower.queue_priority = Some(QueueAvoidancePriority {
            resource: "gate:fare_gate".to_owned(),
            ticket: 5,
        });
        let agents = [leader, follower];

        let leader_velocity = choose_velocity(&agents, 0, Vec2 { x: 1.0, y: 0.0 }, 1.0, 2.5, 0.5);
        let follower_velocity = choose_velocity(&agents, 1, Vec2::ZERO, 1.0, 2.5, 0.5);

        assert!(!leader_velocity.constraint_fallback);
        assert!(!follower_velocity.constraint_fallback);
        assert!((leader_velocity.velocity.x - 1.0).abs() < 1e-9);
        assert!(follower_velocity.velocity.x > 0.0);
    }

    #[test]
    fn later_grid_ticket_can_avoid_a_predecessors_selected_velocity() {
        let mut leader = agent("leader", 0.0, 0.0, 0.0, 0.0);
        leader.queue_priority = Some(QueueAvoidancePriority {
            resource: "gate:fare_gate".to_owned(),
            ticket: 4,
        });
        let mut follower = agent("follower", 0.8, 0.0, 0.0, 0.0);
        follower.queue_priority = Some(QueueAvoidancePriority {
            resource: "gate:fare_gate".to_owned(),
            ticket: 5,
        });
        let mut agents = [leader, follower];

        let leader_velocity = choose_velocity(&agents, 0, Vec2 { x: 1.0, y: 0.0 }, 1.0, 2.5, 0.5);
        agents[0].velocity = leader_velocity.velocity;
        let follower_velocity = choose_velocity(&agents, 1, Vec2::ZERO, 1.0, 2.5, 0.5);

        let leader_next = agents[0].position + leader_velocity.velocity * 0.5;
        let follower_next = agents[1].position + follower_velocity.velocity * 0.5;
        assert!((leader_next - follower_next).length() >= 0.6 - 1e-9);
    }

    #[test]
    fn queue_priority_does_not_cross_grid_resource_boundaries() {
        let mut first = agent("first", 0.0, 0.0, 0.0, 0.0);
        first.queue_priority = Some(QueueAvoidancePriority {
            resource: "gate:first".to_owned(),
            ticket: 4,
        });
        let mut second = agent("second", 0.4, 0.0, 0.0, 0.0);
        second.queue_priority = Some(QueueAvoidancePriority {
            resource: "gate:second".to_owned(),
            ticket: 5,
        });
        let agents = [first, second];

        let first_velocity = choose_velocity(&agents, 0, Vec2 { x: 1.0, y: 0.0 }, 1.0, 2.5, 0.5);
        let second_velocity = choose_velocity(&agents, 1, Vec2::ZERO, 1.0, 2.5, 0.5);

        assert!(!first_velocity.constraint_fallback);
        assert!(!second_velocity.constraint_fallback);
        assert!((first_velocity.velocity.x + 0.2).abs() < 1e-9);
        assert!((second_velocity.velocity.x - 0.2).abs() < 1e-9);
    }

    #[test]
    fn unconstrained_motion_returns_the_preferred_velocity() {
        let agents = [agent("solo", 0.0, 0.0, 0.0, 0.0)];
        let result = choose_velocity(&agents, 0, Vec2 { x: 0.6, y: -0.4 }, 1.0, 2.5, 0.1);
        assert_eq!(result.velocity, Vec2 { x: 0.6, y: -0.4 });
        assert!(!result.constraint_fallback);
    }
}
