use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::GenerationConfig;
use crate::generation::generate_saved_structure;
use crate::scenario::{generate_scenario, save_scenario};
use crate::structure::{self, SavedStructure, StructureResult};
use crate::validation::{validate_scenario, validate_structure};

#[derive(Serialize)]
struct BundleManifest {
    seed: String,
    profile: String,
    files: Vec<String>,
}

#[derive(Serialize)]
struct ValidationReport {
    structure_valid: bool,
    scenario_valid: bool,
    messages: Vec<String>,
}

#[derive(Serialize)]
struct ScreenshotMetadata {
    seed: String,
    profile: String,
    screenshots: Vec<String>,
    note: String,
}

pub fn create_bundle(
    directory: impl AsRef<Path>,
    seed: String,
    config: GenerationConfig,
) -> StructureResult<()> {
    let directory = directory.as_ref();
    fs::create_dir_all(directory)?;
    let structure = generate_saved_structure(seed.clone(), config)?;
    let scenario = generate_scenario(&structure);

    let structure_path = directory.join("structure.json");
    let scenario_path = directory.join("scenario.json");
    structure::save_structure(&structure_path, &structure)?;
    save_scenario(&scenario_path, &scenario)?;
    write_summary(directory.join("summary.md"), &structure)?;
    write_schema_copy(
        "docs/structure.schema.json",
        directory.join("structure.schema.json"),
    )?;
    write_schema_copy(
        "docs/scenario.schema.json",
        directory.join("scenario.schema.json"),
    )?;

    let mut messages = Vec::new();
    let structure_valid = validate_structure(&structure)
        .map(|_| {
            messages.push("structure valid".to_owned());
            true
        })
        .unwrap_or_else(|error| {
            messages.push(format!("structure invalid: {error}"));
            false
        });
    let scenario_valid = validate_scenario(&scenario)
        .map(|_| {
            messages.push("scenario valid".to_owned());
            true
        })
        .unwrap_or_else(|error| {
            messages.push(format!("scenario invalid: {error}"));
            false
        });

    write_json(
        directory.join("validation-report.json"),
        &ValidationReport {
            structure_valid,
            scenario_valid,
            messages,
        },
    )?;
    write_json(
        directory.join("screenshots.json"),
        &ScreenshotMetadata {
            seed: structure.seed.clone(),
            profile: structure.metadata.profile.clone(),
            screenshots: Vec::new(),
            note:
                "Renderer-triggered screenshots can be associated with this bundle by seed/profile."
                    .to_owned(),
        },
    )?;
    write_json(
        directory.join("bundle-manifest.json"),
        &BundleManifest {
            seed,
            profile: structure.metadata.profile.clone(),
            files: vec![
                "structure.json".to_owned(),
                "scenario.json".to_owned(),
                "summary.md".to_owned(),
                "validation-report.json".to_owned(),
                "bundle-manifest.json".to_owned(),
                "screenshots.json".to_owned(),
                "structure.schema.json".to_owned(),
                "scenario.schema.json".to_owned(),
            ],
        },
    )
}

fn write_summary(path: PathBuf, structure: &SavedStructure) -> StructureResult<()> {
    let summary = format!(
        "# Gibson Bundle\n\nSeed: `{}`\nProfile: `{}`\nSchema: `{}`\n\n- Rooms: {}\n- Routes: {}\n- Hazards: {}\n- Resource networks: {}\n- Failure zones: {}\n- Quality score: {:.2}\n",
        structure.seed,
        structure.metadata.profile,
        structure.metadata.schema_version,
        structure.metadata.room_count,
        structure.metadata.transit_edge_count,
        structure.metadata.hazard_zone_count,
        structure.metadata.resource_network_count,
        structure.metadata.failure_zone_count,
        structure.path_analysis.quality_score
    );
    fs::write(path, summary)?;
    Ok(())
}

fn write_schema_copy(from: &str, to: PathBuf) -> StructureResult<()> {
    fs::copy(from, to)?;
    Ok(())
}

fn write_json(path: PathBuf, value: &impl Serialize) -> StructureResult<()> {
    fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn bundle_writes_expected_artifacts() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("gibson_bundle_test_{nonce}"));
        create_bundle(&dir, "ABCD1234".to_owned(), GenerationConfig::default()).unwrap();
        for file in [
            "structure.json",
            "scenario.json",
            "summary.md",
            "validation-report.json",
            "bundle-manifest.json",
            "screenshots.json",
            "structure.schema.json",
            "scenario.schema.json",
        ] {
            assert!(dir.join(file).exists(), "missing {file}");
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
