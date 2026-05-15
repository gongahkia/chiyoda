use std::path::PathBuf;
use std::str::FromStr;
use std::{error::Error, io};

use crate::config::{GenerationConfig, GenerationProfile};
use crate::seed::{generate_seed, validate_seed};
use crate::structure::StructureResult;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InspectSection {
    Summary,
    Routes,
    Landmarks,
    Path,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeOptions {
    pub seed: String,
    pub config: GenerationConfig,
    pub export_path: PathBuf,
    pub scenario_path: Option<PathBuf>,
    pub headless: bool,
    pub inspect_path: Option<PathBuf>,
    pub inspect_sections: Vec<InspectSection>,
}

impl RuntimeOptions {
    pub fn from_env() -> StructureResult<Self> {
        Self::parse(std::env::args().skip(1))
    }

    pub fn parse(args: impl IntoIterator<Item = String>) -> StructureResult<Self> {
        let mut positional_seed = None;
        let mut explicit_seed = None;
        let mut profile = GenerationProfile::Balanced;
        let mut config_path = None;
        let mut export_path = PathBuf::from(crate::structure::STRUCTURE_FILE);
        let mut scenario_path = None;
        let mut headless = false;
        let mut inspect_path = None;
        let mut inspect_sections = Vec::new();

        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--seed" => explicit_seed = Some(next_arg(&mut args, "--seed")?),
                "--profile" => {
                    let value = next_arg(&mut args, "--profile")?;
                    profile = GenerationProfile::from_str(&value).map_err(invalid_input)?;
                }
                "--config" => config_path = Some(PathBuf::from(next_arg(&mut args, "--config")?)),
                "--export" => export_path = PathBuf::from(next_arg(&mut args, "--export")?),
                "--scenario" => {
                    scenario_path = Some(PathBuf::from(next_arg(&mut args, "--scenario")?))
                }
                "--headless" => headless = true,
                "--inspect" => {
                    inspect_path = Some(PathBuf::from(next_arg(&mut args, "--inspect")?))
                }
                "--summary" => inspect_sections.push(InspectSection::Summary),
                "--routes" => inspect_sections.push(InspectSection::Routes),
                "--landmarks" => inspect_sections.push(InspectSection::Landmarks),
                "--path" => inspect_sections.push(InspectSection::Path),
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

        let config = if let Some(path) = config_path {
            GenerationConfig::from_json_file(path, profile)?
        } else {
            let config = GenerationConfig::profile(profile);
            config.validate().map_err(invalid_input)?;
            config
        };

        if inspect_path.is_some() && inspect_sections.is_empty() {
            inspect_sections.push(InspectSection::Summary);
        }

        Ok(Self {
            seed,
            config,
            export_path,
            scenario_path,
            headless,
            inspect_path,
            inspect_sections,
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
    "Usage: gibson-rust [SEED] [--seed SEED] [--profile balanced|dense|vertical|decayed|neon] [--config path.json] [--export path.json] [--scenario scenario.json] [--headless] [--inspect structure.json] [--summary] [--routes] [--landmarks] [--path]"
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
            "--export".to_owned(),
            "out.json".to_owned(),
            "--headless".to_owned(),
        ])
        .unwrap();
        assert_eq!(options.seed, "ABCD1234");
        assert_eq!(options.config.profile, GenerationProfile::Neon);
        assert_eq!(options.export_path, PathBuf::from("out.json"));
        assert!(options.headless);
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
    fn parses_inspection_sections() {
        let options = RuntimeOptions::parse([
            "--inspect".to_owned(),
            "structure.json".to_owned(),
            "--routes".to_owned(),
            "--landmarks".to_owned(),
            "--path".to_owned(),
        ])
        .unwrap();

        assert_eq!(options.inspect_path, Some(PathBuf::from("structure.json")));
        assert_eq!(
            options.inspect_sections,
            vec![
                InspectSection::Routes,
                InspectSection::Landmarks,
                InspectSection::Path
            ]
        );
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
