//! Source-lock validation for data acquired before an empirical benchmark round.
//!
//! An evidence catalog is intentionally weaker than a [`BenchmarkManifest`]: it
//! records a reproducible, redistributable source and an explicit split, but it
//! cannot authorize an empirical claim or benchmark round on its own.

use crate::benchmark::DatasetRole;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceFile {
    pub id: String,
    pub role: DatasetRole,
    /// HTTPS URL of the immutable file content, rather than a mutable landing page.
    pub source_url: String,
    /// A relative path below the caller-supplied data root. It is never a URL.
    pub local_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    /// Upstream-provided checksum, retained for audit even when it uses a
    /// different algorithm than Chiyoda's SHA-256 content lock.
    pub upstream_checksum: String,
    /// A concise, reproducible description of the unit/coordinate treatment.
    pub transformation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceCatalog {
    pub schema_version: String,
    pub dataset_id: String,
    pub title: String,
    pub landing_page: String,
    pub license: String,
    pub redistributable: bool,
    pub citation: String,
    pub files: Vec<EvidenceFile>,
    pub supported_primitives: String,
    pub exclusions: String,
    /// Must explain the split unit and why leakage is controlled. A later
    /// protocol review, not this string, decides scientific adequacy.
    pub split_rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceValidationError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for EvidenceValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for EvidenceValidationError {}

/// Validate that a catalog is specific enough to lock acquired files. It does
/// not judge measurement quality and does not make a source an empirical round.
pub fn validate_catalog(catalog: &EvidenceCatalog) -> Result<(), Vec<EvidenceValidationError>> {
    let mut errors = Vec::new();
    if catalog.schema_version != "0.1" {
        errors.push(issue("schema_version", "must be `0.1`"));
    }
    for (field, value) in [
        ("dataset_id", &catalog.dataset_id),
        ("title", &catalog.title),
        ("landing_page", &catalog.landing_page),
        ("license", &catalog.license),
        ("citation", &catalog.citation),
        ("supported_primitives", &catalog.supported_primitives),
        ("exclusions", &catalog.exclusions),
        ("split_rationale", &catalog.split_rationale),
    ] {
        if value.trim().is_empty() {
            errors.push(issue(field, "must not be empty"));
        }
    }
    if !is_https_url(&catalog.landing_page) {
        errors.push(issue("landing_page", "must be an HTTPS URL"));
    }
    if catalog.license != "CC-BY-4.0" {
        errors.push(issue(
            "license",
            "must be the normalized, redistributable license identifier `CC-BY-4.0`",
        ));
    }
    if !catalog.redistributable {
        errors.push(issue("redistributable", "must be true"));
    }
    if catalog.files.is_empty() {
        errors.push(issue("files", "must not be empty"));
    }

    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    let mut has_calibration = false;
    let mut has_held_out = false;
    for (index, file) in catalog.files.iter().enumerate() {
        let path = format!("files[{index}]");
        if file.id.trim().is_empty() || file.transformation.trim().is_empty() {
            errors.push(issue(&path, "id and transformation are required"));
        }
        if !ids.insert(file.id.as_str()) {
            errors.push(issue(&format!("{path}.id"), "must be unique"));
        }
        if !paths.insert(file.local_path.as_str()) {
            errors.push(issue(&format!("{path}.local_path"), "must be unique"));
        }
        if !is_https_url(&file.source_url) {
            errors.push(issue(&format!("{path}.source_url"), "must be an HTTPS URL"));
        }
        if !is_safe_relative_path(&file.local_path) {
            errors.push(issue(
                &format!("{path}.local_path"),
                "must be a non-empty relative path without `.` or `..` components",
            ));
        }
        if !is_sha256(&file.sha256) {
            errors.push(issue(
                &format!("{path}.sha256"),
                "must be a SHA-256 hexadecimal digest",
            ));
        }
        if file.size_bytes == 0 {
            errors.push(issue(&format!("{path}.size_bytes"), "must be non-zero"));
        }
        if file.upstream_checksum.trim().is_empty() {
            errors.push(issue(
                &format!("{path}.upstream_checksum"),
                "must retain the publisher-provided checksum",
            ));
        }
        match file.role {
            DatasetRole::Calibration => has_calibration = true,
            DatasetRole::HeldOut => has_held_out = true,
        }
    }
    if !has_calibration || !has_held_out {
        errors.push(issue(
            "files",
            "must designate at least one calibration and one held-out file",
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn is_https_url(value: &str) -> bool {
    value.starts_with("https://") && value.len() > "https://".len()
}

fn is_safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn issue(path: &str, message: impl Into<String>) -> EvidenceValidationError {
    EvidenceValidationError {
        path: path.to_owned(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{EvidenceCatalog, EvidenceFile, validate_catalog};
    use crate::benchmark::DatasetRole;

    fn catalog() -> EvidenceCatalog {
        let checksum = "a".repeat(64);
        EvidenceCatalog {
            schema_version: "0.1".to_owned(),
            dataset_id: "station".to_owned(),
            title: "Station trajectories".to_owned(),
            landing_page: "https://example.test/record".to_owned(),
            license: "CC-BY-4.0".to_owned(),
            redistributable: true,
            citation: "Example (2026)".to_owned(),
            files: vec![
                EvidenceFile {
                    id: "one".to_owned(),
                    role: DatasetRole::Calibration,
                    source_url: "https://example.test/one".to_owned(),
                    local_path: "one.parquet".to_owned(),
                    sha256: checksum.clone(),
                    size_bytes: 1,
                    upstream_checksum: "md5:1234".to_owned(),
                    transformation: "retain source values".to_owned(),
                },
                EvidenceFile {
                    id: "two".to_owned(),
                    role: DatasetRole::HeldOut,
                    source_url: "https://example.test/two".to_owned(),
                    local_path: "two.parquet".to_owned(),
                    sha256: checksum,
                    size_bytes: 1,
                    upstream_checksum: "md5:5678".to_owned(),
                    transformation: "retain source values".to_owned(),
                },
            ],
            supported_primitives: "horizontal walking".to_owned(),
            exclusions: "all other primitives".to_owned(),
            split_rationale: "files are disjoint".to_owned(),
        }
    }

    #[test]
    fn catalog_requires_a_safe_content_lock_and_two_way_split() {
        let valid = catalog();
        assert!(validate_catalog(&valid).is_ok());
        let mut invalid = valid;
        invalid.files[1].local_path = "../escape.parquet".to_owned();
        invalid.files[1].role = DatasetRole::Calibration;
        assert!(validate_catalog(&invalid).is_err());
    }
}
