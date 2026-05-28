use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::str::FromStr;

use crate::structure::StructureResult;

pub const MIN_GRID_SIZE: usize = 12;
pub const MAX_GRID_SIZE: usize = 80;
pub const MIN_GRID_LAYERS: usize = 6;
pub const MAX_GRID_LAYERS: usize = 48;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationProfile {
    Balanced,
    Dense,
    Vertical,
    Decayed,
    Neon,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MegastructureTypology {
    DenseEnclave,
    ArcologySpire,
    LinearCity,
    BridgeVoid,
    MarinePlatform,
    OrbitalRing,
    UndergroundHive,
    MountainBurrow,
    DesertArcology,
    AirportCity,
    DamCity,
    ShipyardStack,
    VolcanicCaldera,
    IceShelfCity,
    CanopyBabel,
    SpaceElevatorAnchor,
    CrawlerCity,
    ReefAtollArcology,
    StratospherePlatform,
    SinkholeCitadel,
}

impl MegastructureTypology {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DenseEnclave => "dense_enclave",
            Self::ArcologySpire => "arcology_spire",
            Self::LinearCity => "linear_city",
            Self::BridgeVoid => "bridge_void",
            Self::MarinePlatform => "marine_platform",
            Self::OrbitalRing => "orbital_ring",
            Self::UndergroundHive => "underground_hive",
            Self::MountainBurrow => "mountain_burrow",
            Self::DesertArcology => "desert_arcology",
            Self::AirportCity => "airport_city",
            Self::DamCity => "dam_city",
            Self::ShipyardStack => "shipyard_stack",
            Self::VolcanicCaldera => "volcanic_caldera",
            Self::IceShelfCity => "ice_shelf_city",
            Self::CanopyBabel => "canopy_babel",
            Self::SpaceElevatorAnchor => "space_elevator_anchor",
            Self::CrawlerCity => "crawler_city",
            Self::ReefAtollArcology => "reef_atoll_arcology",
            Self::StratospherePlatform => "stratosphere_platform",
            Self::SinkholeCitadel => "sinkhole_citadel",
        }
    }

    pub fn cli_values() -> &'static str {
        "dense-enclave|arcology-spire|linear-city|bridge-void|marine-platform|orbital-ring|underground-hive|mountain-burrow|desert-arcology|airport-city|dam-city|shipyard-stack|volcanic-caldera|ice-shelf-city|canopy-babel|space-elevator-anchor|crawler-city|reef-atoll-arcology|stratosphere-platform|sinkhole-citadel"
    }

    pub fn all() -> [Self; 20] {
        [
            Self::DenseEnclave,
            Self::ArcologySpire,
            Self::LinearCity,
            Self::BridgeVoid,
            Self::MarinePlatform,
            Self::OrbitalRing,
            Self::UndergroundHive,
            Self::MountainBurrow,
            Self::DesertArcology,
            Self::AirportCity,
            Self::DamCity,
            Self::ShipyardStack,
            Self::VolcanicCaldera,
            Self::IceShelfCity,
            Self::CanopyBabel,
            Self::SpaceElevatorAnchor,
            Self::CrawlerCity,
            Self::ReefAtollArcology,
            Self::StratospherePlatform,
            Self::SinkholeCitadel,
        ]
    }
}

impl Default for MegastructureTypology {
    fn default() -> Self {
        Self::DenseEnclave
    }
}

impl fmt::Display for MegastructureTypology {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for MegastructureTypology {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.replace('-', "_").as_str() {
            "dense_enclave" => Ok(Self::DenseEnclave),
            "arcology_spire" => Ok(Self::ArcologySpire),
            "linear_city" => Ok(Self::LinearCity),
            "bridge_void" => Ok(Self::BridgeVoid),
            "marine_platform" => Ok(Self::MarinePlatform),
            "orbital_ring" => Ok(Self::OrbitalRing),
            "underground_hive" => Ok(Self::UndergroundHive),
            "mountain_burrow" => Ok(Self::MountainBurrow),
            "desert_arcology" => Ok(Self::DesertArcology),
            "airport_city" => Ok(Self::AirportCity),
            "dam_city" => Ok(Self::DamCity),
            "shipyard_stack" => Ok(Self::ShipyardStack),
            "volcanic_caldera" => Ok(Self::VolcanicCaldera),
            "ice_shelf_city" => Ok(Self::IceShelfCity),
            "canopy_babel" => Ok(Self::CanopyBabel),
            "space_elevator_anchor" => Ok(Self::SpaceElevatorAnchor),
            "crawler_city" => Ok(Self::CrawlerCity),
            "reef_atoll_arcology" => Ok(Self::ReefAtollArcology),
            "stratosphere_platform" => Ok(Self::StratospherePlatform),
            "sinkhole_citadel" => Ok(Self::SinkholeCitadel),
            _ => Err(format!("unknown typology '{value}'")),
        }
    }
}

impl GenerationProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::Dense => "dense",
            Self::Vertical => "vertical",
            Self::Decayed => "decayed",
            Self::Neon => "neon",
        }
    }

    pub fn all() -> [Self; 5] {
        [
            Self::Balanced,
            Self::Dense,
            Self::Vertical,
            Self::Decayed,
            Self::Neon,
        ]
    }
}

impl fmt::Display for GenerationProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for GenerationProfile {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "balanced" => Ok(Self::Balanced),
            "dense" => Ok(Self::Dense),
            "vertical" => Ok(Self::Vertical),
            "decayed" => Ok(Self::Decayed),
            "neon" => Ok(Self::Neon),
            _ => Err(format!("unknown profile '{value}'")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GenerationConfig {
    pub profile: GenerationProfile,
    #[serde(default)]
    pub typology: MegastructureTypology,
    pub grid_size: usize,
    pub grid_layers: usize,
    pub district_density_scale: f32,
    pub verticality_scale: f32,
    pub bridge_frequency: f32,
    pub cable_frequency: f32,
    pub pipe_frequency: f32,
    pub erosion_strength: f32,
    pub neon_intensity: f32,
    pub wfc_room_density: f32,
    pub route_density: f32,
    pub landmark_frequency: f32,
    pub decay_story_density: f32,
    pub district_contrast: f32,
    pub strata_separation: f32,
    pub entity_density: f32,
    pub entity_layout_pressure: f32,
    pub advanced_pattern_complexity: f32,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self::profile(GenerationProfile::Balanced)
    }
}

impl GenerationConfig {
    pub fn profile(profile: GenerationProfile) -> Self {
        match profile {
            GenerationProfile::Balanced => Self {
                profile,
                typology: MegastructureTypology::DenseEnclave,
                grid_size: 30,
                grid_layers: 15,
                district_density_scale: 1.0,
                verticality_scale: 1.0,
                bridge_frequency: 1.0,
                cable_frequency: 1.0,
                pipe_frequency: 1.0,
                erosion_strength: 1.0,
                neon_intensity: 1.0,
                wfc_room_density: 1.0,
                route_density: 1.0,
                landmark_frequency: 1.0,
                decay_story_density: 1.0,
                district_contrast: 1.0,
                strata_separation: 1.0,
                entity_density: 1.0,
                entity_layout_pressure: 1.0,
                advanced_pattern_complexity: 1.0,
            },
            GenerationProfile::Dense => Self {
                profile,
                typology: MegastructureTypology::DenseEnclave,
                grid_size: 36,
                grid_layers: 18,
                district_density_scale: 1.35,
                verticality_scale: 1.05,
                bridge_frequency: 1.25,
                cable_frequency: 1.25,
                pipe_frequency: 1.30,
                erosion_strength: 0.85,
                neon_intensity: 0.9,
                wfc_room_density: 1.25,
                route_density: 1.35,
                landmark_frequency: 1.05,
                decay_story_density: 0.8,
                district_contrast: 1.15,
                strata_separation: 0.9,
                entity_density: 1.25,
                entity_layout_pressure: 1.15,
                advanced_pattern_complexity: 1.25,
            },
            GenerationProfile::Vertical => Self {
                profile,
                typology: MegastructureTypology::DenseEnclave,
                grid_size: 30,
                grid_layers: 24,
                district_density_scale: 0.95,
                verticality_scale: 1.55,
                bridge_frequency: 1.35,
                cable_frequency: 1.10,
                pipe_frequency: 1.0,
                erosion_strength: 0.75,
                neon_intensity: 1.0,
                wfc_room_density: 1.0,
                route_density: 1.2,
                landmark_frequency: 1.1,
                decay_story_density: 0.7,
                district_contrast: 1.0,
                strata_separation: 1.35,
                entity_density: 0.95,
                entity_layout_pressure: 0.85,
                advanced_pattern_complexity: 1.1,
            },
            GenerationProfile::Decayed => Self {
                profile,
                typology: MegastructureTypology::DenseEnclave,
                grid_size: 30,
                grid_layers: 15,
                district_density_scale: 1.05,
                verticality_scale: 0.95,
                bridge_frequency: 0.8,
                cable_frequency: 1.45,
                pipe_frequency: 1.55,
                erosion_strength: 1.75,
                neon_intensity: 0.55,
                wfc_room_density: 0.85,
                route_density: 0.9,
                landmark_frequency: 1.25,
                decay_story_density: 1.7,
                district_contrast: 1.25,
                strata_separation: 1.0,
                entity_density: 0.9,
                entity_layout_pressure: 1.35,
                advanced_pattern_complexity: 1.2,
            },
            GenerationProfile::Neon => Self {
                profile,
                typology: MegastructureTypology::DenseEnclave,
                grid_size: 32,
                grid_layers: 18,
                district_density_scale: 1.05,
                verticality_scale: 1.20,
                bridge_frequency: 1.15,
                cable_frequency: 1.20,
                pipe_frequency: 0.9,
                erosion_strength: 0.7,
                neon_intensity: 2.0,
                wfc_room_density: 1.15,
                route_density: 1.15,
                landmark_frequency: 1.3,
                decay_story_density: 0.65,
                district_contrast: 1.1,
                strata_separation: 1.0,
                entity_density: 1.2,
                entity_layout_pressure: 0.9,
                advanced_pattern_complexity: 1.15,
            },
        }
    }

    pub fn from_json_file(
        path: impl AsRef<Path>,
        profile: GenerationProfile,
    ) -> StructureResult<Self> {
        let json = fs::read_to_string(path)?;
        let patch: GenerationConfigPatch = serde_json::from_str(&json)?;
        patch.apply_to(Self::profile(profile))
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_usize("grid_size", self.grid_size, MIN_GRID_SIZE, MAX_GRID_SIZE)?;
        validate_usize(
            "grid_layers",
            self.grid_layers,
            MIN_GRID_LAYERS,
            MAX_GRID_LAYERS,
        )?;
        validate_f32(
            "district_density_scale",
            self.district_density_scale,
            0.2,
            3.0,
        )?;
        validate_f32("verticality_scale", self.verticality_scale, 0.2, 3.0)?;
        validate_f32("bridge_frequency", self.bridge_frequency, 0.0, 4.0)?;
        validate_f32("cable_frequency", self.cable_frequency, 0.0, 4.0)?;
        validate_f32("pipe_frequency", self.pipe_frequency, 0.0, 4.0)?;
        validate_f32("erosion_strength", self.erosion_strength, 0.0, 3.0)?;
        validate_f32("neon_intensity", self.neon_intensity, 0.0, 4.0)?;
        validate_f32("wfc_room_density", self.wfc_room_density, 0.2, 3.0)?;
        validate_f32("route_density", self.route_density, 0.0, 4.0)?;
        validate_f32("landmark_frequency", self.landmark_frequency, 0.0, 4.0)?;
        validate_f32("decay_story_density", self.decay_story_density, 0.0, 4.0)?;
        validate_f32("district_contrast", self.district_contrast, 0.2, 3.0)?;
        validate_f32("strata_separation", self.strata_separation, 0.5, 2.0)?;
        validate_f32("entity_density", self.entity_density, 0.0, 4.0)?;
        validate_f32(
            "entity_layout_pressure",
            self.entity_layout_pressure,
            0.0,
            3.0,
        )?;
        validate_f32(
            "advanced_pattern_complexity",
            self.advanced_pattern_complexity,
            0.0,
            3.0,
        )
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GenerationConfigPatch {
    profile: Option<GenerationProfile>,
    typology: Option<MegastructureTypology>,
    grid_size: Option<usize>,
    grid_layers: Option<usize>,
    district_density_scale: Option<f32>,
    verticality_scale: Option<f32>,
    bridge_frequency: Option<f32>,
    cable_frequency: Option<f32>,
    pipe_frequency: Option<f32>,
    erosion_strength: Option<f32>,
    neon_intensity: Option<f32>,
    wfc_room_density: Option<f32>,
    route_density: Option<f32>,
    landmark_frequency: Option<f32>,
    decay_story_density: Option<f32>,
    district_contrast: Option<f32>,
    strata_separation: Option<f32>,
    entity_density: Option<f32>,
    entity_layout_pressure: Option<f32>,
    advanced_pattern_complexity: Option<f32>,
}

impl GenerationConfigPatch {
    fn apply_to(self, mut config: GenerationConfig) -> StructureResult<GenerationConfig> {
        if let Some(profile) = self.profile {
            config = GenerationConfig::profile(profile);
        }
        if let Some(typology) = self.typology {
            config.typology = typology;
        }
        if let Some(value) = self.grid_size {
            config.grid_size = value;
        }
        if let Some(value) = self.grid_layers {
            config.grid_layers = value;
        }
        if let Some(value) = self.district_density_scale {
            config.district_density_scale = value;
        }
        if let Some(value) = self.verticality_scale {
            config.verticality_scale = value;
        }
        if let Some(value) = self.bridge_frequency {
            config.bridge_frequency = value;
        }
        if let Some(value) = self.cable_frequency {
            config.cable_frequency = value;
        }
        if let Some(value) = self.pipe_frequency {
            config.pipe_frequency = value;
        }
        if let Some(value) = self.erosion_strength {
            config.erosion_strength = value;
        }
        if let Some(value) = self.neon_intensity {
            config.neon_intensity = value;
        }
        if let Some(value) = self.wfc_room_density {
            config.wfc_room_density = value;
        }
        if let Some(value) = self.route_density {
            config.route_density = value;
        }
        if let Some(value) = self.landmark_frequency {
            config.landmark_frequency = value;
        }
        if let Some(value) = self.decay_story_density {
            config.decay_story_density = value;
        }
        if let Some(value) = self.district_contrast {
            config.district_contrast = value;
        }
        if let Some(value) = self.strata_separation {
            config.strata_separation = value;
        }
        if let Some(value) = self.entity_density {
            config.entity_density = value;
        }
        if let Some(value) = self.entity_layout_pressure {
            config.entity_layout_pressure = value;
        }
        if let Some(value) = self.advanced_pattern_complexity {
            config.advanced_pattern_complexity = value;
        }
        config
            .validate()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        Ok(config)
    }
}

fn validate_usize(name: &str, value: usize, min: usize, max: usize) -> Result<(), String> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "{name} must be between {min} and {max}, got {value}"
        ))
    }
}

fn validate_f32(name: &str, value: f32, min: f32, max: f32) -> Result<(), String> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "{name} must be between {min} and {max}, got {value}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_balanced_profile() {
        assert_eq!(
            GenerationConfig::default(),
            GenerationConfig::profile(GenerationProfile::Balanced)
        );
    }

    #[test]
    fn rejects_invalid_bounds() {
        let config = GenerationConfig {
            grid_size: 4,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = GenerationConfig {
            route_density: 5.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = GenerationConfig {
            strata_separation: 0.1,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = GenerationConfig {
            entity_density: 4.5,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = GenerationConfig {
            entity_layout_pressure: 3.5,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = GenerationConfig {
            advanced_pattern_complexity: 3.5,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn config_patch_defaults_from_profile() {
        let patch = GenerationConfigPatch {
            grid_layers: Some(20),
            ..Default::default()
        };
        let config = patch
            .apply_to(GenerationConfig::profile(GenerationProfile::Dense))
            .unwrap();
        assert_eq!(config.profile, GenerationProfile::Dense);
        assert_eq!(config.grid_layers, 20);
        assert_eq!(config.grid_size, 36);
    }

    #[test]
    fn config_patch_accepts_generation_story_controls() {
        let patch = GenerationConfigPatch {
            typology: Some(MegastructureTypology::LinearCity),
            route_density: Some(1.7),
            landmark_frequency: Some(1.4),
            decay_story_density: Some(0.6),
            district_contrast: Some(1.2),
            strata_separation: Some(1.3),
            entity_density: Some(1.6),
            entity_layout_pressure: Some(0.8),
            advanced_pattern_complexity: Some(1.9),
            ..Default::default()
        };
        let config = patch.apply_to(GenerationConfig::default()).unwrap();
        assert_eq!(config.typology, MegastructureTypology::LinearCity);
        assert_eq!(config.route_density, 1.7);
        assert_eq!(config.landmark_frequency, 1.4);
        assert_eq!(config.decay_story_density, 0.6);
        assert_eq!(config.district_contrast, 1.2);
        assert_eq!(config.strata_separation, 1.3);
        assert_eq!(config.entity_density, 1.6);
        assert_eq!(config.entity_layout_pressure, 0.8);
        assert_eq!(config.advanced_pattern_complexity, 1.9);
    }

    #[test]
    fn checked_in_presets_load_as_valid_configs() {
        for preset in [
            "presets/flooded-slum.json",
            "presets/corp-skyline.json",
            "presets/market-collapse.json",
            "presets/blackout-core.json",
        ] {
            let config =
                GenerationConfig::from_json_file(preset, GenerationProfile::Balanced).unwrap();
            config.validate().unwrap();
        }
    }
}
