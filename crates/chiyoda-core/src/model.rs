use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exit {
    pub id: String,
    pub surface: String,
    pub at: Point3,
    pub width_m: f64,
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
    },
    Ramp {
        id: String,
        from_surface: String,
        from: Point3,
        to_surface: String,
        to: Point3,
        width_m: f64,
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
    },
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
    pub fn capacity(&self) -> Option<u32> {
        match self {
            Self::Lift { capacity, .. } => Some(*capacity),
            Self::Stair { .. } | Self::Ramp { .. } | Self::Escalator { .. } => None,
        }
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
    pub destination: String,
    pub speed_mps: f64,
    pub radius_m: f64,
    pub height_m: f64,
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
}

impl Proposition {
    #[must_use]
    pub fn connector(&self) -> &str {
        match self {
            Self::ConnectorAvailability { connector, .. } => connector,
        }
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        match self {
            Self::ConnectorAvailability { open, .. } => *open,
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
    pub exits: Vec<Exit>,
    pub connectors: Vec<Connector>,
    pub gates: Vec<Gate>,
    pub agents: Vec<AgentGroup>,
    pub messages: Vec<Message>,
    pub countermeasures: Vec<Countermeasure>,
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
