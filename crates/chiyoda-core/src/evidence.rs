//! Source-lock validation for data acquired before an empirical benchmark round.
//!
//! An evidence catalog is intentionally weaker than a [`BenchmarkManifest`]: it
//! records a reproducible, redistributable source and an explicit split, but it
//! cannot authorize an empirical claim or benchmark round on its own.

use crate::benchmark::DatasetRole;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt, path::Path};

/// The claim-bearing role of a content-locked source catalog.
///
/// A reference catalog is intentionally not a weakened empirical catalog: it
/// has no calibration/held-out designation and cannot be passed to an adapter
/// that reports empirical partitions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePurpose {
    #[default]
    EmpiricalEvaluation,
    UncalibratedReference,
}

// Serde's `skip_serializing_if` callback is defined over a reference, even for
// a Copy enum. Keeping this callback preserves the serialized form of existing
// empirical catalogs and therefore their catalog hashes.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_empirical_evaluation(purpose: &EvidencePurpose) -> bool {
    *purpose == EvidencePurpose::EmpiricalEvaluation
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceFile {
    pub id: String,
    /// Required only for an empirical-evaluation catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<DatasetRole>,
    /// HTTPS URL of the immutable file content, rather than a mutable landing page.
    pub source_url: String,
    /// A relative path below the caller-supplied data root. It is never a URL.
    pub local_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    /// An upstream-provided checksum, when the publisher exposes one. Chiyoda's
    /// required SHA-256 is the independent content lock for every source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_checksum: Option<String>,
    /// A concise, reproducible description of the unit/coordinate treatment.
    pub transformation: String,
}

/// A content-locked logical source stored inside a content-locked ZIP archive.
///
/// Keeping the archive itself in `files` lets acquisition remain one immutable
/// HTTPS transfer. Locking named members separately permits disjoint published
/// trials to carry calibration and held-out roles without pretending that the
/// entire archive belongs to both partitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceArchiveMember {
    pub id: String,
    /// The `EvidenceFile.id` of the ZIP archive containing this member.
    pub archive_file_id: String,
    /// A relative ZIP entry name. It is never extracted by the lock verifier.
    pub member_path: String,
    /// Required only for an empirical-evaluation catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<DatasetRole>,
    pub sha256: String,
    pub size_bytes: u64,
    /// A concise, reproducible description of how this member is interpreted.
    pub transformation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceCatalog {
    pub schema_version: String,
    /// Defaults to the historical empirical-evaluation contract so existing
    /// catalogs retain their meaning and serialized content hash.
    #[serde(default, skip_serializing_if = "is_empirical_evaluation")]
    pub purpose: EvidencePurpose,
    pub dataset_id: String,
    pub title: String,
    pub landing_page: String,
    /// SPDX-like license identifier. `ODbL` sources additionally require an
    /// attribution statement because their output may carry attribution duties.
    pub license: String,
    pub redistributable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    pub citation: String,
    pub files: Vec<EvidenceFile>,
    /// Optional logical source locks for named ZIP members. The absent/empty
    /// form is preserved for historical catalog hashes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub archive_members: Vec<EvidenceArchiveMember>,
    pub supported_primitives: String,
    pub exclusions: String,
    /// Required for empirical evaluation, where it explains the split unit and
    /// leakage boundary. It is deliberately absent from source-only catalogs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_rationale: Option<String>,
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
#[allow(clippy::too_many_lines)] // one complete field-level validation pass keeps catalog errors accumulative
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
    ] {
        if value.trim().is_empty() {
            errors.push(issue(field, "must not be empty"));
        }
    }
    if !is_https_url(&catalog.landing_page) {
        errors.push(issue("landing_page", "must be an HTTPS URL"));
    }
    if !matches!(
        catalog.license.as_str(),
        "CC-BY-4.0" | "CC0-1.0" | "ODbL-1.0"
    ) {
        errors.push(issue(
            "license",
            "must be the normalized, redistributable license identifier `CC-BY-4.0`, `CC0-1.0`, or `ODbL-1.0`",
        ));
    }
    if catalog.license == "ODbL-1.0"
        && catalog
            .attribution
            .as_deref()
            .is_none_or(|attribution| attribution.trim().is_empty())
    {
        errors.push(issue(
            "attribution",
            "must be non-empty for an `ODbL-1.0` source",
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
        if file
            .upstream_checksum
            .as_deref()
            .is_some_and(|checksum| checksum.trim().is_empty())
        {
            errors.push(issue(
                &format!("{path}.upstream_checksum"),
                "must not be empty when the publisher provides one",
            ));
        }
    }
    let file_ids = catalog
        .files
        .iter()
        .map(|file| file.id.as_str())
        .collect::<HashSet<_>>();
    let mut archive_paths = HashSet::new();
    for (index, member) in catalog.archive_members.iter().enumerate() {
        let path = format!("archive_members[{index}]");
        if member.id.trim().is_empty() || member.transformation.trim().is_empty() {
            errors.push(issue(&path, "id and transformation are required"));
        }
        if !ids.insert(member.id.as_str()) {
            errors.push(issue(
                &format!("{path}.id"),
                "must be unique across files and archive members",
            ));
        }
        if !file_ids.contains(member.archive_file_id.as_str()) {
            errors.push(issue(
                &format!("{path}.archive_file_id"),
                "must identify a declared source file",
            ));
        }
        if !is_safe_relative_path(&member.member_path) {
            errors.push(issue(
                &format!("{path}.member_path"),
                "must be a non-empty relative path without `.` or `..` components",
            ));
        }
        if !archive_paths.insert((member.archive_file_id.as_str(), member.member_path.as_str())) {
            errors.push(issue(
                &format!("{path}.member_path"),
                "must be unique within its declared archive",
            ));
        }
        if !is_sha256(&member.sha256) {
            errors.push(issue(
                &format!("{path}.sha256"),
                "must be a SHA-256 hexadecimal digest",
            ));
        }
        if member.size_bytes == 0 {
            errors.push(issue(&format!("{path}.size_bytes"), "must be non-zero"));
        }
    }
    validate_catalog_purpose(catalog, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_catalog_purpose(catalog: &EvidenceCatalog, errors: &mut Vec<EvidenceValidationError>) {
    match catalog.purpose {
        EvidencePurpose::EmpiricalEvaluation => {
            if !matches!(catalog.license.as_str(), "CC-BY-4.0" | "CC0-1.0") {
                errors.push(issue(
                    "license",
                    "empirical evaluation requires `CC-BY-4.0` or `CC0-1.0`; `ODbL-1.0` is limited to uncalibrated source observation",
                ));
            }
            if catalog
                .split_rationale
                .as_deref()
                .is_none_or(|rationale| rationale.trim().is_empty())
            {
                errors.push(issue(
                    "split_rationale",
                    "is required for empirical evaluation and must not be empty",
                ));
            }
            let mut has_calibration = false;
            let mut has_held_out = false;
            let archive_file_ids = catalog
                .archive_members
                .iter()
                .map(|member| member.archive_file_id.as_str())
                .collect::<HashSet<_>>();
            for file in &catalog.files {
                match file.role {
                    Some(DatasetRole::Calibration) => has_calibration = true,
                    Some(DatasetRole::HeldOut) => has_held_out = true,
                    None if !archive_file_ids.contains(file.id.as_str()) => {
                        errors.push(issue(
                            "files",
                            "each empirical-evaluation source must designate calibration or held_out unless it is a declared archive backing file",
                        ));
                    }
                    None => {}
                }
            }
            for member in &catalog.archive_members {
                match member.role {
                    Some(DatasetRole::Calibration) => has_calibration = true,
                    Some(DatasetRole::HeldOut) => has_held_out = true,
                    None => errors.push(issue(
                        "archive_members",
                        "each empirical-evaluation archive member must designate calibration or held_out",
                    )),
                }
            }
            if !has_calibration || !has_held_out {
                errors.push(issue(
                    "files",
                    "must designate at least one calibration and one held-out file",
                ));
            }
        }
        EvidencePurpose::UncalibratedReference => {
            if catalog.files.iter().any(|file| file.role.is_some())
                || catalog
                    .archive_members
                    .iter()
                    .any(|member| member.role.is_some())
            {
                errors.push(issue(
                    "files",
                    "uncalibrated reference sources and archive members must not declare calibration or held_out roles",
                ));
            }
        }
    }
}

fn is_https_url(value: &str) -> bool {
    value.starts_with("https://") && value.len() > "https://".len()
}

fn is_safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && !value.contains('\\')
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
    use super::{
        EvidenceArchiveMember, EvidenceCatalog, EvidenceFile, EvidencePurpose, validate_catalog,
    };
    use crate::benchmark::DatasetRole;

    fn catalog() -> EvidenceCatalog {
        let checksum = "a".repeat(64);
        EvidenceCatalog {
            schema_version: "0.1".to_owned(),
            purpose: EvidencePurpose::EmpiricalEvaluation,
            dataset_id: "station".to_owned(),
            title: "Station trajectories".to_owned(),
            landing_page: "https://example.test/record".to_owned(),
            license: "CC-BY-4.0".to_owned(),
            redistributable: true,
            attribution: None,
            citation: "Example (2026)".to_owned(),
            files: vec![
                EvidenceFile {
                    id: "one".to_owned(),
                    role: Some(DatasetRole::Calibration),
                    source_url: "https://example.test/one".to_owned(),
                    local_path: "one.parquet".to_owned(),
                    sha256: checksum.clone(),
                    size_bytes: 1,
                    upstream_checksum: Some("md5:1234".to_owned()),
                    transformation: "retain source values".to_owned(),
                },
                EvidenceFile {
                    id: "two".to_owned(),
                    role: Some(DatasetRole::HeldOut),
                    source_url: "https://example.test/two".to_owned(),
                    local_path: "two.parquet".to_owned(),
                    sha256: checksum,
                    size_bytes: 1,
                    upstream_checksum: Some("md5:5678".to_owned()),
                    transformation: "retain source values".to_owned(),
                },
            ],
            archive_members: Vec::new(),
            supported_primitives: "horizontal walking".to_owned(),
            exclusions: "all other primitives".to_owned(),
            split_rationale: Some("files are disjoint".to_owned()),
        }
    }

    #[test]
    fn catalog_requires_a_safe_content_lock_and_two_way_split() {
        let valid = catalog();
        assert!(validate_catalog(&valid).is_ok());
        let mut invalid = valid;
        invalid.files[1].local_path = "../escape.parquet".to_owned();
        invalid.files[1].role = Some(DatasetRole::Calibration);
        assert!(validate_catalog(&invalid).is_err());
    }

    #[test]
    fn source_only_catalog_locks_open_data_without_faking_a_split() {
        let checksum = "a".repeat(64);
        let catalog = EvidenceCatalog {
            schema_version: "0.1".to_owned(),
            purpose: EvidencePurpose::UncalibratedReference,
            dataset_id: "reference".to_owned(),
            title: "Reference trajectories".to_owned(),
            landing_page: "https://example.test/record".to_owned(),
            license: "CC-BY-4.0".to_owned(),
            redistributable: true,
            attribution: None,
            citation: "Example (2026)".to_owned(),
            files: vec![EvidenceFile {
                id: "source".to_owned(),
                role: None,
                source_url: "https://example.test/source".to_owned(),
                local_path: "source.csv".to_owned(),
                sha256: checksum,
                size_bytes: 1,
                upstream_checksum: None,
                transformation: "retain source values".to_owned(),
            }],
            archive_members: Vec::new(),
            supported_primitives: "descriptive trajectories".to_owned(),
            exclusions: "empirical evaluation".to_owned(),
            split_rationale: None,
        };
        assert!(validate_catalog(&catalog).is_ok());
    }

    #[test]
    fn odbl_source_observation_requires_attribution_and_cannot_be_empirical() {
        let mut catalog = catalog();
        catalog.purpose = EvidencePurpose::UncalibratedReference;
        for source in &mut catalog.files {
            source.role = None;
        }
        catalog.split_rationale = None;
        catalog.license = "ODbL-1.0".to_owned();
        catalog.attribution = Some("© OpenStreetMap contributors".to_owned());
        assert!(validate_catalog(&catalog).is_ok());

        catalog.attribution = None;
        assert!(validate_catalog(&catalog).is_err());

        catalog.attribution = Some("© OpenStreetMap contributors".to_owned());
        catalog.purpose = EvidencePurpose::EmpiricalEvaluation;
        for source in &mut catalog.files {
            source.role = Some(DatasetRole::Calibration);
        }
        catalog.split_rationale = Some("fixture split".to_owned());
        assert!(validate_catalog(&catalog).is_err());
    }

    #[test]
    fn archive_members_can_form_a_content_locked_empirical_split() {
        let mut catalog = catalog();
        catalog.license = "CC0-1.0".to_owned();
        catalog.files.truncate(1);
        catalog.files[0].id = "archive".to_owned();
        catalog.files[0].role = None;
        catalog.files[0].local_path = "trials.zip".to_owned();
        catalog.archive_members = vec![
            EvidenceArchiveMember {
                id: "trial-calibration".to_owned(),
                archive_file_id: "archive".to_owned(),
                member_path: "trials/one.txt".to_owned(),
                role: Some(DatasetRole::Calibration),
                sha256: "b".repeat(64),
                size_bytes: 1,
                transformation: "read source trajectory rows unchanged".to_owned(),
            },
            EvidenceArchiveMember {
                id: "trial-held-out".to_owned(),
                archive_file_id: "archive".to_owned(),
                member_path: "trials/two.txt".to_owned(),
                role: Some(DatasetRole::HeldOut),
                sha256: "c".repeat(64),
                size_bytes: 1,
                transformation: "read source trajectory rows unchanged".to_owned(),
            },
        ];

        assert!(validate_catalog(&catalog).is_ok());

        catalog.archive_members[1].member_path = "trials/one.txt".to_owned();
        assert!(validate_catalog(&catalog).is_err());
    }
}
