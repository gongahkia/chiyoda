use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::{GenerationProfile, MegastructureTypology};
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
    #[serde(default)]
    pub typology: Option<MegastructureTypology>,
    pub district: String,
    pub stratum: String,
    pub density_weight: f32,
    pub route_weight: f32,
    pub decay_weight: f32,
    pub detail_weight: f32,
    #[serde(default)]
    pub entity_density_weight: Option<f32>,
    #[serde(default)]
    pub entity_layout_weight: Option<f32>,
    #[serde(default)]
    pub patrol_weight: Option<f32>,
    #[serde(default)]
    pub crowd_weight: Option<f32>,
    #[serde(default)]
    pub builder_weight: Option<f32>,
    #[serde(default)]
    pub grammar: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompiledRulePackSet {
    pub source_name: String,
    packs: Vec<CompiledRulePack>,
    by_key: BTreeMap<RulePackKey, usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledRulePack {
    pub name: String,
    pub profile: GenerationProfile,
    pub typology: Option<MegastructureTypology>,
    pub district: String,
    pub stratum: String,
    pub density_weight: f32,
    pub route_weight: f32,
    pub decay_weight: f32,
    pub detail_weight: f32,
    pub entity_density_weight: f32,
    pub entity_layout_weight: f32,
    pub patrol_weight: f32,
    pub crowd_weight: f32,
    pub builder_weight: f32,
    pub grammar: Vec<String>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct RulePackKey {
    profile: GenerationProfile,
    typology: Option<MegastructureTypology>,
    district: String,
    stratum: String,
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

    pub fn compile(&self) -> StructureResult<CompiledRulePackSet> {
        self.validate()?;
        let mut packs = Vec::with_capacity(self.packs.len());
        let mut by_key = BTreeMap::new();
        for pack in &self.packs {
            let compiled = pack.compile();
            let key = compiled.key();
            ensure(
                !by_key.contains_key(&key),
                format!(
                    "duplicate rule pack for profile={} typology={} district={} stratum={}",
                    key.profile,
                    key.typology
                        .map(|typology| typology.to_string())
                        .unwrap_or_else(|| "all".to_owned()),
                    key.district,
                    key.stratum
                ),
            )?;
            by_key.insert(key, packs.len());
            packs.push(compiled);
        }
        Ok(CompiledRulePackSet {
            source_name: self.name.clone(),
            packs,
            by_key,
        })
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
        validate_weight("detail_weight", self.detail_weight, 0.05, 1.5)?;
        validate_optional_weight(
            "entity_density_weight",
            self.entity_density_weight,
            0.0,
            4.0,
        )?;
        validate_optional_weight("entity_layout_weight", self.entity_layout_weight, 0.0, 4.0)?;
        validate_optional_weight("patrol_weight", self.patrol_weight, 0.0, 3.0)?;
        validate_optional_weight("crowd_weight", self.crowd_weight, 0.0, 3.0)?;
        validate_optional_weight("builder_weight", self.builder_weight, 0.0, 3.0)
    }

    fn compile(&self) -> CompiledRulePack {
        CompiledRulePack {
            name: self.name.clone(),
            profile: self.profile,
            typology: self.typology,
            district: self.district.clone(),
            stratum: self.stratum.clone(),
            density_weight: self.density_weight,
            route_weight: self.route_weight,
            decay_weight: self.decay_weight,
            detail_weight: self.detail_weight,
            entity_density_weight: self.entity_density_weight.unwrap_or(1.0),
            entity_layout_weight: self.entity_layout_weight.unwrap_or(1.0),
            patrol_weight: self.patrol_weight.unwrap_or(1.0),
            crowd_weight: self.crowd_weight.unwrap_or(1.0),
            builder_weight: self.builder_weight.unwrap_or(1.0),
            grammar: self.grammar.clone(),
        }
    }
}

impl CompiledRulePackSet {
    pub fn from_json_file(path: impl AsRef<Path>) -> StructureResult<Self> {
        RulePackDocument::from_json_file(path)?.compile()
    }

    pub fn is_empty(&self) -> bool {
        self.packs.is_empty()
    }

    pub fn packs(&self) -> &[CompiledRulePack] {
        &self.packs
    }

    pub fn find(
        &self,
        profile: GenerationProfile,
        typology: Option<MegastructureTypology>,
        district: &str,
        stratum: &str,
    ) -> Option<&CompiledRulePack> {
        for typology in [typology, None] {
            let key = RulePackKey {
                profile,
                typology,
                district: district.to_owned(),
                stratum: stratum.to_owned(),
            };
            if let Some(pack) = self
                .by_key
                .get(&key)
                .and_then(|index| self.packs.get(*index))
            {
                return Some(pack);
            }
        }
        None
    }
}

impl CompiledRulePack {
    fn key(&self) -> RulePackKey {
        RulePackKey {
            profile: self.profile,
            typology: self.typology,
            district: self.district.clone(),
            stratum: self.stratum.clone(),
        }
    }
}

pub fn validate_rule_file(path: impl AsRef<Path>) -> StructureResult<Vec<String>> {
    let compiled = CompiledRulePackSet::from_json_file(path)?;
    Ok(vec![format!(
        "valid rule pack document '{}' with {} compiled packs",
        compiled.source_name,
        compiled.packs().len()
    )])
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

fn validate_optional_weight(
    name: &str,
    value: Option<f32>,
    min: f32,
    max: f32,
) -> StructureResult<()> {
    if let Some(value) = value {
        validate_weight(name, value, min, max)?;
    }
    Ok(())
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
    fn compiles_rule_document_into_lookup_set() {
        let compiled = CompiledRulePackSet::from_json_file("rules/balanced-base.json").unwrap();
        let pack = compiled
            .find(GenerationProfile::Balanced, None, "SLUM", "SURFACE")
            .unwrap();
        assert_eq!(pack.name, "balanced_slum_surface");
        assert!(pack.grammar.iter().any(|rule| rule.contains("corridors")));
    }

    #[test]
    fn rejects_duplicate_rule_targets() {
        let pack = EditableRulePack {
            name: "one".to_owned(),
            profile: GenerationProfile::Balanced,
            typology: None,
            district: "SLUM".to_owned(),
            stratum: "SURFACE".to_owned(),
            density_weight: 1.0,
            route_weight: 1.0,
            decay_weight: 1.0,
            detail_weight: 0.5,
            entity_density_weight: None,
            entity_layout_weight: None,
            patrol_weight: None,
            crowd_weight: None,
            builder_weight: None,
            grammar: Vec::new(),
        };
        let document = RulePackDocument {
            name: "duplicate".to_owned(),
            description: String::new(),
            packs: vec![pack.clone(), pack],
        };
        assert!(document.compile().is_err());
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
                typology: None,
                district: "SLUM".to_owned(),
                stratum: "SURFACE".to_owned(),
                density_weight: 9.0,
                route_weight: 1.0,
                decay_weight: 1.0,
                detail_weight: 0.5,
                entity_density_weight: None,
                entity_layout_weight: None,
                patrol_weight: None,
                crowd_weight: None,
                builder_weight: None,
                grammar: Vec::new(),
            }],
        };
        assert!(document.validate().is_err());
    }

    #[test]
    fn optional_entity_rule_weights_default_to_neutral() {
        let compiled = CompiledRulePackSet::from_json_file("rules/balanced-base.json").unwrap();
        let pack = compiled
            .find(GenerationProfile::Balanced, None, "SLUM", "SURFACE")
            .unwrap();
        assert_eq!(pack.entity_density_weight, 1.0);
        assert_eq!(pack.entity_layout_weight, 1.0);
        assert_eq!(pack.patrol_weight, 1.0);
        assert_eq!(pack.crowd_weight, 1.0);
        assert_eq!(pack.builder_weight, 1.0);
    }

    #[test]
    fn all_checked_in_rule_presets_compile() {
        for entry in fs::read_dir("rules").unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
                let compiled = CompiledRulePackSet::from_json_file(&path).unwrap();
                assert!(
                    !compiled.is_empty(),
                    "expected compiled packs in {}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn typology_specific_rules_override_generic_rules() {
        let generic = EditableRulePack {
            name: "generic".to_owned(),
            profile: GenerationProfile::Balanced,
            typology: None,
            district: "COMMERCIAL".to_owned(),
            stratum: "SURFACE".to_owned(),
            density_weight: 1.0,
            route_weight: 1.0,
            decay_weight: 1.0,
            detail_weight: 0.5,
            entity_density_weight: None,
            entity_layout_weight: None,
            patrol_weight: None,
            crowd_weight: None,
            builder_weight: None,
            grammar: Vec::new(),
        };
        let mut specific = generic.clone();
        specific.name = "linear".to_owned();
        specific.typology = Some(MegastructureTypology::LinearCity);
        specific.route_weight = 1.7;
        let document = RulePackDocument {
            name: "typology".to_owned(),
            description: String::new(),
            packs: vec![generic, specific],
        };
        let compiled = document.compile().unwrap();
        assert_eq!(
            compiled
                .find(
                    GenerationProfile::Balanced,
                    Some(MegastructureTypology::LinearCity),
                    "COMMERCIAL",
                    "SURFACE"
                )
                .unwrap()
                .name,
            "linear"
        );
        assert_eq!(
            compiled
                .find(
                    GenerationProfile::Balanced,
                    Some(MegastructureTypology::OrbitalRing),
                    "COMMERCIAL",
                    "SURFACE"
                )
                .unwrap()
                .name,
            "generic"
        );
    }
}
