use serde::{Deserialize, Serialize};

/// Extra clearance used to keep visibility-graph paths off obstacle boundaries.
pub(crate) const NAVIGATION_CLEARANCE_EPSILON_M: f64 = 1e-6;

/// A point in metres in the scenario's declared Cartesian coordinate system.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point3 {
    pub x_m: f64,
    pub y_m: f64,
    pub z_m: f64,
}

impl Point3 {
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        let dx = self.x_m - other.x_m;
        let dy = self.y_m - other.y_m;
        let dz = self.z_m - other.z_m;
        (dx.mul_add(dx, dy.mul_add(dy, dz * dz))).sqrt()
    }

    #[must_use]
    pub fn lerp(self, other: Self, ratio: f64) -> Self {
        Self {
            x_m: self.x_m + ((other.x_m - self.x_m) * ratio),
            y_m: self.y_m + ((other.y_m - self.y_m) * ratio),
            z_m: self.z_m + ((other.z_m - self.z_m) * ratio),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    pub id: String,
    pub origin: Point3,
    pub width_m: f64,
    pub depth_m: f64,
}

impl Surface {
    #[must_use]
    pub fn contains(&self, point: Point3) -> bool {
        const EPSILON: f64 = 1e-9;
        (point.z_m - self.origin.z_m).abs() < EPSILON
            && point.x_m >= self.origin.x_m - EPSILON
            && point.x_m <= self.origin.x_m + self.width_m + EPSILON
            && point.y_m >= self.origin.y_m - EPSILON
            && point.y_m <= self.origin.y_m + self.depth_m + EPSILON
    }
}

/// An axis-aligned rectangular no-go zone on a declared walkable surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Obstacle {
    pub id: String,
    pub surface: String,
    pub at: Point3,
    pub width_m: f64,
    pub depth_m: f64,
}

/// A required intermediate point in an agent group's authored journey.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Waypoint {
    pub id: String,
    pub surface: String,
    pub at: Point3,
    /// Authored hold time after arrival before the next journey stage begins.
    pub dwell_s: f64,
}

impl Obstacle {
    #[must_use]
    pub fn contains(&self, point: Point3) -> bool {
        const EPSILON: f64 = 1e-9;
        (point.z_m - self.at.z_m).abs() < EPSILON
            && point.x_m >= self.at.x_m - EPSILON
            && point.x_m <= self.at.x_m + self.width_m + EPSILON
            && point.y_m >= self.at.y_m - EPSILON
            && point.y_m <= self.at.y_m + self.depth_m + EPSILON
    }

    /// Match the conservative axis-aligned obstacle expansion used by navigation.
    pub(crate) fn contains_with_clearance(&self, point: Point3, clearance_m: f64) -> bool {
        point.x_m >= self.at.x_m - clearance_m
            && point.x_m <= self.at.x_m + self.width_m + clearance_m
            && point.y_m >= self.at.y_m - clearance_m
            && point.y_m <= self.at.y_m + self.depth_m + clearance_m
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exit {
    pub id: String,
    pub surface: String,
    pub at: Point3,
    pub width_m: f64,
    /// Optional authored discharge limit. Width alone does not imply flow.
    pub capacity_per_s: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gate {
    pub id: String,
    pub surface: String,
    pub at: Point3,
    pub width_m: f64,
    /// Maximum people processed per second, declared as `N/s` in source.
    pub service_rate_per_s: f64,
    /// The exit this gate controls. This makes service semantics unambiguous.
    pub destination: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Connector {
    Stair {
        id: String,
        from_surface: String,
        from: Point3,
        to_surface: String,
        to: Point3,
        width_m: f64,
        /// Optional authored throughput limit. Width alone is not treated as
        /// empirical evidence for a particular service rate.
        capacity_per_s: Option<f64>,
        /// Optional authored maximum traversable agent height.
        clearance_height_m: Option<f64>,
    },
    Ramp {
        id: String,
        from_surface: String,
        from: Point3,
        to_surface: String,
        to: Point3,
        width_m: f64,
        capacity_per_s: Option<f64>,
        clearance_height_m: Option<f64>,
    },
    Escalator {
        id: String,
        from_surface: String,
        from: Point3,
        to_surface: String,
        to: Point3,
        width_m: f64,
        /// Declared directed belt speed. The reference model assumes walking
        /// agents add their own speed to this value while in transit.
        belt_speed_mps: f64,
        capacity_per_s: Option<f64>,
        clearance_height_m: Option<f64>,
    },
    Lift {
        id: String,
        from_surface: String,
        from: Point3,
        to_surface: String,
        to: Point3,
        cabin_width_m: f64,
        cabin_depth_m: f64,
        capacity: u32,
        cycle_s: f64,
        clearance_height_m: Option<f64>,
    },
}

/// A connector class used for authored route-eligibility constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorKind {
    Stair,
    Ramp,
    Escalator,
    Lift,
}

impl ConnectorKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stair => "stair",
            Self::Ramp => "ramp",
            Self::Escalator => "escalator",
            Self::Lift => "lift",
        }
    }
}

impl Connector {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Stair { id, .. }
            | Self::Ramp { id, .. }
            | Self::Escalator { id, .. }
            | Self::Lift { id, .. } => id,
        }
    }

    #[must_use]
    pub fn from_surface(&self) -> &str {
        match self {
            Self::Stair { from_surface, .. }
            | Self::Ramp { from_surface, .. }
            | Self::Escalator { from_surface, .. }
            | Self::Lift { from_surface, .. } => from_surface,
        }
    }

    #[must_use]
    pub fn to_surface(&self) -> &str {
        match self {
            Self::Stair { to_surface, .. }
            | Self::Ramp { to_surface, .. }
            | Self::Escalator { to_surface, .. }
            | Self::Lift { to_surface, .. } => to_surface,
        }
    }

    #[must_use]
    pub fn from(&self) -> Point3 {
        match self {
            Self::Stair { from, .. }
            | Self::Ramp { from, .. }
            | Self::Escalator { from, .. }
            | Self::Lift { from, .. } => *from,
        }
    }

    #[must_use]
    pub fn to(&self) -> Point3 {
        match self {
            Self::Stair { to, .. }
            | Self::Ramp { to, .. }
            | Self::Escalator { to, .. }
            | Self::Lift { to, .. } => *to,
        }
    }

    #[must_use]
    pub fn is_lift(&self) -> bool {
        matches!(self, Self::Lift { .. })
    }

    #[must_use]
    pub fn kind(&self) -> ConnectorKind {
        match self {
            Self::Stair { .. } => ConnectorKind::Stair,
            Self::Ramp { .. } => ConnectorKind::Ramp,
            Self::Escalator { .. } => ConnectorKind::Escalator,
            Self::Lift { .. } => ConnectorKind::Lift,
        }
    }

    #[must_use]
    pub fn capacity(&self) -> Option<u32> {
        match self {
            Self::Lift { capacity, .. } => Some(*capacity),
            Self::Stair { .. } | Self::Ramp { .. } | Self::Escalator { .. } => None,
        }
    }

    #[must_use]
    pub fn service_rate_per_s(&self) -> Option<f64> {
        match self {
            Self::Stair { capacity_per_s, .. }
            | Self::Ramp { capacity_per_s, .. }
            | Self::Escalator { capacity_per_s, .. } => *capacity_per_s,
            Self::Lift { .. } => None,
        }
    }

    #[must_use]
    pub fn clearance_height_m(&self) -> Option<f64> {
        match self {
            Self::Stair {
                clearance_height_m, ..
            }
            | Self::Ramp {
                clearance_height_m, ..
            }
            | Self::Escalator {
                clearance_height_m, ..
            }
            | Self::Lift {
                clearance_height_m, ..
            } => *clearance_height_m,
        }
    }

    #[must_use]
    pub fn supports_height(&self, height_m: f64) -> bool {
        self.clearance_height_m()
            .is_none_or(|clearance_height_m| height_m <= clearance_height_m)
    }

    #[must_use]
    pub fn traversal_duration_s(&self, walking_speed_mps: f64) -> f64 {
        match self {
            Self::Stair { from, to, .. } | Self::Ramp { from, to, .. } => {
                from.distance(*to) / walking_speed_mps
            }
            Self::Escalator {
                from,
                to,
                belt_speed_mps,
                ..
            } => from.distance(*to) / (walking_speed_mps + belt_speed_mps),
            Self::Lift { cycle_s, .. } => *cycle_s,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentGroup {
    pub id: String,
    pub count: u32,
    pub surface: String,
    pub at: Point3,
    /// The first declared final exit, used to break equal nominal-route costs.
    pub destination: String,
    /// Additional final exits considered alongside `destination`; declaration
    /// order breaks equal nominal-route costs.
    pub alternative_destinations: Vec<String>,
    pub speed_mps: f64,
    pub radius_m: f64,
    pub height_m: f64,
    /// The authored time at which this group becomes active in the simulation.
    /// This is a scenario input, not an inferred arrival distribution.
    pub release_at_s: f64,
    /// Ordered required stages before the group's final exit.
    pub via: Vec<String>,
    /// Connector classes this group must not traverse. This is an authored
    /// route constraint, not an inferred mobility or accessibility profile.
    pub excluded_connector_kinds: Vec<ConnectorKind>,
}

impl AgentGroup {
    pub fn exit_candidates(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.destination.as_str())
            .chain(self.alternative_destinations.iter().map(String::as_str))
    }

    #[must_use]
    pub fn allows_connector(&self, connector: &Connector) -> bool {
        !self.excluded_connector_kinds.contains(&connector.kind())
    }

    /// Return the deterministic row-major positions used to instantiate this group.
    pub(crate) fn spawn_positions(&self) -> impl Iterator<Item = Point3> + '_ {
        let columns = spawn_grid_columns(self.count);
        let spacing = self.radius_m * 2.1;
        (0..self.count).map(move |ordinal| {
            let column = ordinal % columns;
            let row = ordinal / columns;
            Point3 {
                x_m: self.at.x_m + f64::from(column) * spacing,
                y_m: self.at.y_m + f64::from(row) * spacing,
                z_m: self.at.z_m,
            }
        })
    }
}

fn spawn_grid_columns(count: u32) -> u32 {
    let mut columns = 1_u32;
    while columns.saturating_mul(columns) < count {
        columns += 1;
    }
    columns
}

/// An authored change to a connector's physical availability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorStateChange {
    pub id: String,
    pub connector: String,
    pub open: bool,
    pub at_s: f64,
}

/// An authored change to an exit's physical availability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExitStateChange {
    pub id: String,
    pub exit: String,
    pub open: bool,
    pub at_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InformationSource {
    Peer,
    Official,
    Signage,
    Staff,
}

impl InformationSource {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Peer => "peer",
            Self::Official => "official",
            Self::Signage => "signage",
            Self::Staff => "staff",
        }
    }
}

/// A route-relevant, machine-checkable proposition. Free-form narrative is
/// intentionally excluded from the reference semantics: it belongs in an
/// attached research note, not in an executable causal claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Proposition {
    ConnectorAvailability { connector: String, open: bool },
    ExitAvailability { exit: String, open: bool },
}

impl Proposition {
    #[must_use]
    pub fn subject_kind(&self) -> &'static str {
        match self {
            Self::ConnectorAvailability { .. } => "connector",
            Self::ExitAvailability { .. } => "exit",
        }
    }

    #[must_use]
    pub fn subject(&self) -> &str {
        match self {
            Self::ConnectorAvailability { connector, .. } => connector,
            Self::ExitAvailability { exit, .. } => exit,
        }
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        match self {
            Self::ConnectorAvailability { open, .. } => *open,
            Self::ExitAvailability { open, .. } => *open,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub source: InformationSource,
    pub surface: String,
    pub origin: Point3,
    pub claim: Proposition,
    pub truthful: bool,
    pub at_s: f64,
    pub reach_m: f64,
    pub trust: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Countermeasure {
    pub id: String,
    pub corrects: String,
    pub source: InformationSource,
    pub surface: String,
    pub origin: Point3,
    pub at_s: f64,
    pub reach_m: f64,
    pub trust: f64,
}

/// The parsed, typed source program. Its vectors retain declaration order,
/// which is part of the deterministic execution contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub name: String,
    pub seed: u64,
    pub duration_s: f64,
    pub timestep_s: f64,
    pub surfaces: Vec<Surface>,
    pub obstacles: Vec<Obstacle>,
    pub waypoints: Vec<Waypoint>,
    pub exits: Vec<Exit>,
    pub connectors: Vec<Connector>,
    pub connector_states: Vec<ConnectorStateChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exit_states: Vec<ExitStateChange>,
    pub gates: Vec<Gate>,
    pub agents: Vec<AgentGroup>,
    pub messages: Vec<Message>,
    pub countermeasures: Vec<Countermeasure>,
}

impl Scenario {
    /// Return a connector's authored physical state at an exact scenario time.
    /// The default is open; same-time declarations apply in source order.
    #[must_use]
    pub fn connector_open_at(&self, connector_id: &str, time_s: f64) -> bool {
        let mut changes: Vec<_> = self
            .connector_states
            .iter()
            .enumerate()
            .filter(|(_, change)| change.connector == connector_id && change.at_s <= time_s)
            .collect();
        changes.sort_by(|(left_index, left), (right_index, right)| {
            left.at_s
                .total_cmp(&right.at_s)
                .then(left_index.cmp(right_index))
        });
        changes.into_iter().fold(true, |_, (_, change)| change.open)
    }

    /// Return an exit's authored physical state at an exact scenario time.
    /// The default is open; same-time declarations apply in source order.
    #[must_use]
    pub fn exit_open_at(&self, exit_id: &str, time_s: f64) -> bool {
        let mut changes: Vec<_> = self
            .exit_states
            .iter()
            .enumerate()
            .filter(|(_, change)| change.exit == exit_id && change.at_s <= time_s)
            .collect();
        changes.sort_by(|(left_index, left), (right_index, right)| {
            left.at_s
                .total_cmp(&right.at_s)
                .then(left_index.cmp(right_index))
        });
        changes.into_iter().fold(true, |_, (_, change)| change.open)
    }
}

/// The compiler's stable interchange representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalScenario {
    pub language_version: String,
    pub scenario: Scenario,
}

impl From<Scenario> for CanonicalScenario {
    fn from(scenario: Scenario) -> Self {
        Self {
            language_version: crate::LANGUAGE_VERSION.to_owned(),
            scenario,
        }
    }
}
