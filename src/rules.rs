use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::GenerationProfile;
use crate::structure::StructureResult;

const VALID_DISTRICTS: &[&str] = &["INDUSTRIAL", "RESIDENTIAL", "COMMERCIAL", "SLUM", "ELITE"];
const VALID_STRATA: &[&str] = &["UNDERGROUND", "SURFACE", "MIDRISE", "SKYLINE"];

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RulePackDocument {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub packs: Vec<EditableRulePack>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EditableRulePack {
    pub name: String,
    pub profile: GenerationProfile,
    pub district: String,
    pub stratum: String,
    pub density_weight: f32,
    pub route_weight: f32,
    pub decay_weight: f32,
    pub detail_weight: f32,
    #[serde(default)]
    pub grammar: Vec<String>,
}

impl RulePackDocument {
    pub fn from_json_file(path: impl AsRef<Path>) -> StructureResult<Self> {
        let json = fs::read_to_string(path)?;
        let document: Self = serde_json::from_str(&json)?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> StructureResult<()> {
        ensure(!self.name.trim().is_empty(), "rule document name is empty")?;
        ensure(!self.packs.is_empty(), "rule document contains no packs")?;
        for pack in &self.packs {
            pack.validate()?;
        }
        Ok(())
    }
}

impl EditableRulePack {
    pub fn validate(&self) -> StructureResult<()> {
        ensure(!self.name.trim().is_empty(), "rule pack name is empty")?;
        ensure(
            VALID_DISTRICTS.contains(&self.district.as_str()),
            format!("unknown rule pack district '{}'", self.district),
        )?;
        ensure(
            VALID_STRATA.contains(&self.stratum.as_str()),
            format!("unknown rule pack stratum '{}'", self.stratum),
        )?;
        validate_weight("density_weight", self.density_weight, 0.05, 4.0)?;
        validate_weight("route_weight", self.route_weight, 0.05, 4.0)?;
        validate_weight("decay_weight", self.decay_weight, 0.05, 4.0)?;
        validate_weight("detail_weight", self.detail_weight, 0.05, 1.5)
    }
}

pub fn load_optional_rule_document(
    path: Option<impl AsRef<Path>>,
) -> StructureResult<Option<RulePackDocument>> {
    match path {
        Some(path) => RulePackDocument::from_json_file(path).map(Some),
        None => Ok(None),
    }
}

fn validate_weight(name: &str, value: f32, min: f32, max: f32) -> StructureResult<()> {
    ensure(
        value.is_finite() && (min..=max).contains(&value),
        format!("{name} must be between {min} and {max}, got {value}"),
    )
}

fn ensure(condition: bool, message: impl Into<String>) -> StructureResult<()> {
    if condition {
        Ok(())
    } else {
        Err(Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            message.into(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editable_rule_document_loads_from_json() {
        let document = RulePackDocument::from_json_file("rules/balanced-base.json").unwrap();
        assert_eq!(document.name, "balanced-base");
        assert!(!document.packs.is_empty());
    }

    #[test]
    fn missing_external_rules_fall_back_to_built_ins() {
        let document = load_optional_rule_document(Option::<&str>::None).unwrap();
        assert!(document.is_none());
    }

    #[test]
    fn rejects_invalid_rule_bounds() {
        let document = RulePackDocument {
            name: "bad".to_owned(),
            description: String::new(),
            packs: vec![EditableRulePack {
                name: "bad".to_owned(),
                profile: GenerationProfile::Balanced,
                district: "SLUM".to_owned(),
                stratum: "SURFACE".to_owned(),
                density_weight: 9.0,
                route_weight: 1.0,
                decay_weight: 1.0,
                detail_weight: 0.5,
                grammar: Vec::new(),
            }],
        };
        assert!(document.validate().is_err());
    }
}
