//! Evidence manifest validation for reproducible benchmark rounds.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetRole {
    Calibration,
    HeldOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetEvidence {
    pub id: String,
    pub role: DatasetRole,
    pub source_url: String,
    pub license: String,
    pub sha256: String,
    pub redistributable: bool,
    pub transformation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratorRound {
    pub version: String,
    pub public_fixture_seeds: Vec<u64>,
    pub evaluation_seed_commitment: String,
    pub release_after_round: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkManifest {
    pub schema_version: String,
    pub round_id: String,
    pub generator: GeneratorRound,
    pub datasets: Vec<DatasetEvidence>,
    pub claim_boundary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkValidationError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for BenchmarkValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for BenchmarkValidationError {}

/// Enforce the public evidence contract before a round can advertise empirical
/// results. This deliberately rejects private, non-redistributable inputs.
pub fn validate_manifest(
    manifest: &BenchmarkManifest,
) -> Result<(), Vec<BenchmarkValidationError>> {
    let mut errors = Vec::new();
    if manifest.schema_version != "0.1" {
        errors.push(issue("schema_version", "must be `0.1`"));
    }
    if manifest.round_id.trim().is_empty() {
        errors.push(issue("round_id", "must not be empty"));
    }
    if manifest.generator.version.trim().is_empty() {
        errors.push(issue("generator.version", "must not be empty"));
    }
    if manifest.generator.public_fixture_seeds.is_empty() {
        errors.push(issue("generator.public_fixture_seeds", "must not be empty"));
    }
    if manifest.generator.evaluation_seed_commitment.len() != 64
        || !manifest
            .generator
            .evaluation_seed_commitment
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        errors.push(issue(
            "generator.evaluation_seed_commitment",
            "must be a SHA-256 hexadecimal commitment",
        ));
    }
    if !manifest.generator.release_after_round {
        errors.push(issue(
            "generator.release_after_round",
            "must be true for a reproducible public round",
        ));
    }
    let mut calibration = false;
    let mut held_out = false;
    for (index, dataset) in manifest.datasets.iter().enumerate() {
        let path = format!("datasets[{index}]");
        if dataset.id.trim().is_empty()
            || dataset.source_url.trim().is_empty()
            || dataset.license.trim().is_empty()
            || dataset.transformation.trim().is_empty()
        {
            errors.push(issue(
                &path,
                "id, URL, license, and transformation are required",
            ));
        }
        if !dataset.redistributable {
            errors.push(issue(&path, "must be redistributable"));
        }
        if dataset.sha256.len() != 64
            || !dataset
                .sha256
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            errors.push(issue(
                &format!("{path}.sha256"),
                "must be a SHA-256 hexadecimal digest",
            ));
        }
        match dataset.role {
            DatasetRole::Calibration => calibration = true,
            DatasetRole::HeldOut => held_out = true,
        }
    }
    if !calibration || !held_out {
        errors.push(issue(
            "datasets",
            "an empirical round requires at least one calibration and one held-out dataset",
        ));
    }
    if manifest.claim_boundary.trim().is_empty() {
        errors.push(issue("claim_boundary", "must state evidence limitations"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn issue(path: &str, message: impl Into<String>) -> BenchmarkValidationError {
    BenchmarkValidationError {
        path: path.to_owned(),
        message: message.into(),
    }
}
