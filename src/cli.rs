use std::path::PathBuf;
use std::str::FromStr;
use std::{error::Error, io};

use crate::config::{GenerationConfig, GenerationProfile, MegastructureTypology};
use crate::rules::CompiledRulePackSet;
use crate::seed::{generate_seed, validate_seed};
use crate::structure::StructureResult;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InspectSection {
    Summary,
    Routes,
    Landmarks,
    Path,
    Simulation,
    Factions,
    Hazards,
    Quality,
    Entities,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeOptions {
    pub seed: String,
    pub config: GenerationConfig,
    pub export_path: PathBuf,
    pub scenario_path: Option<PathBuf>,
    pub bundle_path: Option<PathBuf>,
    pub rules_path: Option<PathBuf>,
    pub rule_packs: CompiledRulePackSet,
    pub headless: bool,
    pub validate_path: Option<PathBuf>,
    pub validate_rules_path: Option<PathBuf>,
    pub inspect_path: Option<PathBuf>,
    pub inspect_sections: Vec<InspectSection>,
    pub inspect_json: bool,
}

impl RuntimeOptions {
    pub fn from_env() -> StructureResult<Self> {
        Self::parse(std::env::args().skip(1))
    }

    pub fn parse(args: impl IntoIterator<Item = String>) -> StructureResult<Self> {
        let mut positional_seed = None;
        let mut explicit_seed = None;
        let mut profile = GenerationProfile::Balanced;
        let mut typology = None;
        let mut config_path = None;
        let mut export_path = PathBuf::from(crate::structure::STRUCTURE_FILE);
        let mut scenario_path = None;
        let mut bundle_path = None;
        let mut rules_path = None;
        let mut headless = false;
        let mut validate_path = None;
        let mut validate_rules_path = None;
        let mut inspect_path = None;
        let mut inspect_sections = Vec::new();
        let mut inspect_json = false;

        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--seed" => explicit_seed = Some(next_arg(&mut args, "--seed")?),
                "--profile" => {
                    let value = next_arg(&mut args, "--profile")?;
                    profile = GenerationProfile::from_str(&value).map_err(invalid_input)?;
                }
                "--typology" => {
                    let value = next_arg(&mut args, "--typology")?;
                    typology =
                        Some(MegastructureTypology::from_str(&value).map_err(invalid_input)?);
                }
                "--config" => config_path = Some(PathBuf::from(next_arg(&mut args, "--config")?)),
                "--export" => export_path = PathBuf::from(next_arg(&mut args, "--export")?),
                "--scenario" => {
                    scenario_path = Some(PathBuf::from(next_arg(&mut args, "--scenario")?))
                }
                "--bundle" => bundle_path = Some(PathBuf::from(next_arg(&mut args, "--bundle")?)),
                "--rules" => rules_path = Some(PathBuf::from(next_arg(&mut args, "--rules")?)),
                "--headless" => headless = true,
                "--validate" => {
                    validate_path = Some(PathBuf::from(next_arg(&mut args, "--validate")?))
                }
                "--validate-rules" => {
                    validate_rules_path =
                        Some(PathBuf::from(next_arg(&mut args, "--validate-rules")?))
                }
                "--inspect" => {
                    inspect_path = Some(PathBuf::from(next_arg(&mut args, "--inspect")?))
                }
                "--summary" => inspect_sections.push(InspectSection::Summary),
                "--routes" => inspect_sections.push(InspectSection::Routes),
                "--landmarks" => inspect_sections.push(InspectSection::Landmarks),
                "--path" => inspect_sections.push(InspectSection::Path),
                "--simulation" => inspect_sections.push(InspectSection::Simulation),
                "--factions" => inspect_sections.push(InspectSection::Factions),
                "--hazards" => inspect_sections.push(InspectSection::Hazards),
                "--quality" => inspect_sections.push(InspectSection::Quality),
                "--entities" => inspect_sections.push(InspectSection::Entities),
                "--json" => inspect_json = true,
                "--help" | "-h" => return Err(invalid_input(usage())),
                value if value.starts_with("--") => {
                    return Err(invalid_input(format!("unknown option '{value}'")));
                }
                value => {
                    if positional_seed.is_some() {
                        return Err(invalid_input("only one positional seed is supported"));
                    }
                    positional_seed = Some(value.to_owned());
                }
            }
        }

        let seed = explicit_seed
            .or(positional_seed)
            .unwrap_or_else(generate_seed)
            .to_uppercase();
        if !validate_seed(&seed) {
            return Err(invalid_input(format!(
                "Invalid seed '{seed}'. Must be 8 alphanumeric chars."
            )));
        }

        let mut config = if let Some(path) = config_path {
            GenerationConfig::from_json_file(path, profile)?
        } else {
            let config = GenerationConfig::profile(profile);
            config.validate().map_err(invalid_input)?;
            config
        };
        if let Some(typology) = typology {
            config.typology = typology;
            config.validate().map_err(invalid_input)?;
        }
        let rule_packs = if let Some(path) = &rules_path {
            CompiledRulePackSet::from_json_file(path)?
        } else {
            CompiledRulePackSet::default()
        };

        if inspect_path.is_some() && inspect_sections.is_empty() {
            inspect_sections.push(InspectSection::Summary);
        }

        Ok(Self {
            seed,
            config,
            export_path,
            scenario_path,
            bundle_path,
            rules_path,
            rule_packs,
            headless,
            validate_path,
            validate_rules_path,
            inspect_path,
            inspect_sections,
            inspect_json,
        })
    }
}

fn next_arg(args: &mut impl Iterator<Item = String>, name: &str) -> StructureResult<String> {
    args.next()
        .ok_or_else(|| invalid_input(format!("{name} requires a value")))
}

fn invalid_input(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

pub fn usage() -> &'static str {
    "Usage: gibson-rust [SEED] [--seed SEED] [--profile balanced|dense|vertical|decayed|neon] [--typology dense-enclave|arcology-spire|linear-city|bridge-void|marine-platform|orbital-ring|underground-hive|mountain-burrow|desert-arcology|airport-city|dam-city|shipyard-stack|volcanic-caldera|ice-shelf-city|canopy-babel|space-elevator-anchor|crawler-city|reef-atoll-arcology|stratosphere-platform|sinkhole-citadel] [--config path.json] [--rules rules.json] [--export path.json] [--scenario scenario.json] [--bundle out/] [--headless] [--validate artifact.json] [--validate-rules rules.json] [--inspect structure.json] [--summary] [--routes] [--landmarks] [--path] [--simulation] [--factions] [--hazards] [--quality] [--entities] [--json]"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_test_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "gibson_cli_{name}_{}_{}",
            std::process::id(),
            nonce
        ))
    }

    #[test]
    fn accepts_legacy_positional_seed() {
        let options = RuntimeOptions::parse(["ABCD1234".to_owned()]).unwrap();
        assert_eq!(options.seed, "ABCD1234");
        assert_eq!(options.config.profile, GenerationProfile::Balanced);
        assert!(!options.headless);
        assert!(options.inspect_path.is_none());
    }

    #[test]
    fn parses_explicit_profile_export_and_headless() {
        let options = RuntimeOptions::parse([
            "--seed".to_owned(),
            "abcd1234".to_owned(),
            "--profile".to_owned(),
            "neon".to_owned(),
            "--typology".to_owned(),
            "space-elevator-anchor".to_owned(),
            "--export".to_owned(),
            "out.json".to_owned(),
            "--headless".to_owned(),
        ])
        .unwrap();
        assert_eq!(options.seed, "ABCD1234");
        assert_eq!(options.config.profile, GenerationProfile::Neon);
        assert_eq!(
            options.config.typology,
            MegastructureTypology::SpaceElevatorAnchor
        );
        assert_eq!(options.export_path, PathBuf::from("out.json"));
        assert!(options.headless);
    }

    #[test]
    fn parses_validation_path() {
        let options =
            RuntimeOptions::parse(["--validate".to_owned(), "structure.json".to_owned()]).unwrap();

        assert_eq!(options.validate_path, Some(PathBuf::from("structure.json")));
    }

    #[test]
    fn parses_scenario_export_path() {
        let options = RuntimeOptions::parse([
            "--seed".to_owned(),
            "ABCD1234".to_owned(),
            "--headless".to_owned(),
            "--scenario".to_owned(),
            "scenario.json".to_owned(),
        ])
        .unwrap();

        assert_eq!(options.scenario_path, Some(PathBuf::from("scenario.json")));
    }

    #[test]
    fn parses_bundle_path() {
        let options = RuntimeOptions::parse([
            "--seed".to_owned(),
            "ABCD1234".to_owned(),
            "--bundle".to_owned(),
            "out".to_owned(),
        ])
        .unwrap();

        assert_eq!(options.bundle_path, Some(PathBuf::from("out")));
    }

    #[test]
    fn parses_rule_pack_path_and_compiles_rules() {
        let options = RuntimeOptions::parse([
            "--seed".to_owned(),
            "ABCD1234".to_owned(),
            "--profile".to_owned(),
            "decayed".to_owned(),
            "--rules".to_owned(),
            "rules/kowloon-decay.json".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            options.rules_path,
            Some(PathBuf::from("rules/kowloon-decay.json"))
        );
        assert!(options
            .rule_packs
            .find(GenerationProfile::Decayed, None, "SLUM", "SURFACE")
            .is_some());
    }

    #[test]
    fn parses_rule_validation_path() {
        let options = RuntimeOptions::parse([
            "--validate-rules".to_owned(),
            "rules/kowloon-decay.json".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            options.validate_rules_path,
            Some(PathBuf::from("rules/kowloon-decay.json"))
        );
    }

    #[test]
    fn parses_inspection_sections() {
        let options = RuntimeOptions::parse([
            "--inspect".to_owned(),
            "structure.json".to_owned(),
            "--routes".to_owned(),
            "--landmarks".to_owned(),
            "--path".to_owned(),
            "--simulation".to_owned(),
            "--entities".to_owned(),
            "--json".to_owned(),
        ])
        .unwrap();

        assert_eq!(options.inspect_path, Some(PathBuf::from("structure.json")));
        assert_eq!(
            options.inspect_sections,
            vec![
                InspectSection::Routes,
                InspectSection::Landmarks,
                InspectSection::Path,
                InspectSection::Simulation,
                InspectSection::Entities
            ]
        );
        assert!(options.inspect_json);
    }

    #[test]
    fn defaults_inspection_to_summary() {
        let options =
            RuntimeOptions::parse(["--inspect".to_owned(), "structure.json".to_owned()]).unwrap();

        assert_eq!(options.inspect_sections, vec![InspectSection::Summary]);
    }

    #[test]
    fn config_file_overrides_selected_profile_defaults() {
        let dir = temp_test_dir("config");
        fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config.json");
        fs::write(
            &config_path,
            r#"{"grid_layers": 20, "neon_intensity": 1.75}"#,
        )
        .unwrap();

        let options = RuntimeOptions::parse([
            "--seed".to_owned(),
            "ABCD1234".to_owned(),
            "--profile".to_owned(),
            "dense".to_owned(),
            "--config".to_owned(),
            config_path.to_string_lossy().to_string(),
        ])
        .unwrap();

        assert_eq!(options.config.profile, GenerationProfile::Dense);
        assert_eq!(options.config.grid_size, 36);
        assert_eq!(options.config.grid_layers, 20);
        assert_eq!(options.config.neon_intensity, 1.75);
        fs::remove_dir_all(dir).unwrap();
    }
}
