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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationProfile {
    Balanced,
    Dense,
    Vertical,
    Decayed,
    Neon,
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
            },
            GenerationProfile::Dense => Self {
                profile,
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
            },
            GenerationProfile::Vertical => Self {
                profile,
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
            },
            GenerationProfile::Decayed => Self {
                profile,
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
            },
            GenerationProfile::Neon => Self {
                profile,
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
        validate_f32("strata_separation", self.strata_separation, 0.5, 2.0)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GenerationConfigPatch {
    profile: Option<GenerationProfile>,
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
}

impl GenerationConfigPatch {
    fn apply_to(self, mut config: GenerationConfig) -> StructureResult<GenerationConfig> {
        if let Some(profile) = self.profile {
            config = GenerationConfig::profile(profile);
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
            route_density: Some(1.7),
            landmark_frequency: Some(1.4),
            decay_story_density: Some(0.6),
            district_contrast: Some(1.2),
            strata_separation: Some(1.3),
            ..Default::default()
        };
        let config = patch.apply_to(GenerationConfig::default()).unwrap();
        assert_eq!(config.route_density, 1.7);
        assert_eq!(config.landmark_frequency, 1.4);
        assert_eq!(config.decay_story_density, 0.6);
        assert_eq!(config.district_contrast, 1.2);
        assert_eq!(config.strata_separation, 1.3);
    }
}
