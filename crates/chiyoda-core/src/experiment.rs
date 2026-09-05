//! Manifest validation for one auditable, uncalibrated authored experiment.
//!
//! Sensitivity studies record explicit numeric alternatives. This contract
//! covers the complementary case: one structural scenario with a stated claim
//! boundary, disclosed assumptions, and optional source-report provenance.

use crate::sensitivity::{AssumptionBasis, SensitivityReference};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fmt,
    path::{Component, Path},
};

const LEGACY_EXPERIMENT_SCHEMA_VERSION: &str = "0.1";
const EXPERIMENT_SCHEMA_VERSION: &str = "0.2";
const DEFAULT_TRACE_EVERY_STEPS: u32 = 10;

/// An authored, uncalibrated scenario and the assumptions used to interpret it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentManifest {
    pub schema_version: String,
    pub name: String,
    pub description: String,
    /// Path interpreted by the CLI relative to the manifest file.
    pub scenario_source: String,
    #[serde(default = "default_trace_every_steps")]
    pub trace_every_steps: u32,
    /// At least one disclosed model input or structural choice is required.
    pub assumptions: Vec<ExperimentAssumption>,
    /// Open sources that informed assumptions. They do not calibrate the run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SensitivityReference>,
    /// Optional source-specific verification declarations. Schema 0.2 makes
    /// one source-observation workflow reproducible at artifact creation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_attestations: Vec<ExperimentSourceAttestation>,
    /// The author's intended-use and interpretation boundary.
    pub claim_boundary: String,
}

/// An external-source check that must succeed before an experiment artifact is
/// created. It documents source provenance; it does not validate the scenario
/// or upgrade the experiment beyond its uncalibrated claim boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExperimentSourceAttestation {
    /// Rebuild a content-locked OSM observation from its XML, then rebuild the
    /// source's declared local-coordinate projection from that observation.
    OsmLocalProjection {
        source_id: String,
        catalog_path: String,
        data_root: String,
        observation_report_path: String,
    },
    /// Reconstruct selected scenario-point coordinates from an already
    /// attested local OSM projection. This preserves a narrow source-point to
    /// authored-point link; it does not import a layout or validate a model.
    OsmScenarioAnchor {
        source_id: String,
        projection_source_id: String,
        anchor_manifest_path: String,
    },
}

impl ExperimentSourceAttestation {
    #[must_use]
    pub fn source_id(&self) -> &str {
        match self {
            Self::OsmLocalProjection { source_id, .. }
            | Self::OsmScenarioAnchor { source_id, .. } => source_id,
        }
    }
}

fn default_trace_every_steps() -> u32 {
    DEFAULT_TRACE_EVERY_STEPS
}

/// One explicit authored input or structural choice behind an experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentAssumption {
    pub id: String,
    /// A precise scenario field, element, or structural choice the reader can review.
    pub subject: String,
    pub basis: AssumptionBasis,
    pub rationale: String,
    /// Identifiers from the manifest's `sources` array. A documented estimate
    /// or measured input must name at least one source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExperimentValidationError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for ExperimentValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ExperimentValidationError {}

/// Validate manifest shape and provenance references before an experiment
/// creates any runtime artifact. This does not claim a source is applicable or
/// calibrate the reference runtime.
#[allow(clippy::too_many_lines)] // one pass accumulates every author-facing manifest issue
pub fn validate_experiment_manifest(
    manifest: &ExperimentManifest,
) -> Result<(), Vec<ExperimentValidationError>> {
    let mut errors = Vec::new();
    if !matches!(
        manifest.schema_version.as_str(),
        LEGACY_EXPERIMENT_SCHEMA_VERSION | EXPERIMENT_SCHEMA_VERSION
    ) {
        errors.push(issue(
            "schema_version",
            format!(
                "must be `{LEGACY_EXPERIMENT_SCHEMA_VERSION}` or `{EXPERIMENT_SCHEMA_VERSION}`"
            ),
        ));
    }
    if manifest.schema_version == LEGACY_EXPERIMENT_SCHEMA_VERSION
        && !manifest.source_attestations.is_empty()
    {
        errors.push(issue(
            "source_attestations",
            "requires schema_version `0.2`",
        ));
    }
    for (field, value) in [
        ("name", &manifest.name),
        ("description", &manifest.description),
        ("scenario_source", &manifest.scenario_source),
        ("claim_boundary", &manifest.claim_boundary),
    ] {
        if value.trim().is_empty() {
            errors.push(issue(field, "must not be empty"));
        }
    }
    if !is_safe_relative_path(&manifest.scenario_source) {
        errors.push(issue(
            "scenario_source",
            "must be a non-empty relative path without `.` or `..` components",
        ));
    }
    if manifest.trace_every_steps == 0 {
        errors.push(issue("trace_every_steps", "must be greater than zero"));
    }
    if manifest.assumptions.is_empty() {
        errors.push(issue("assumptions", "must not be empty"));
    }

    let mut source_ids = BTreeSet::new();
    for (index, source) in manifest.sources.iter().enumerate() {
        let path = format!("sources[{index}]");
        if !is_safe_identifier(&source.id) {
            errors.push(issue(&format!("{path}.id"), "must be a safe identifier"));
        }
        if !source_ids.insert(source.id.as_str()) {
            errors.push(issue(&format!("{path}.id"), "must be unique"));
        }
        if source.citation.trim().is_empty()
            || source.applicability.trim().is_empty()
            || source.limitation.trim().is_empty()
        {
            errors.push(issue(
                &path,
                "citation, applicability, and limitation must not be empty",
            ));
        }
        if !is_https_url(&source.url) {
            errors.push(issue(
                &format!("{path}.url"),
                "must be a non-empty HTTPS URL",
            ));
        }
        if source
            .source_sha256
            .as_deref()
            .is_some_and(|hash| !is_sha256(hash))
        {
            errors.push(issue(
                &format!("{path}.source_sha256"),
                "must be a SHA-256 hexadecimal digest when provided",
            ));
        }
        if let Some(report) = &source.derived_report {
            if !is_safe_relative_path(&report.path) {
                errors.push(issue(
                    &format!("{path}.derived_report.path"),
                    "must be a non-empty relative path without `.` or `..` components",
                ));
            }
            if !is_sha256(&report.sha256) {
                errors.push(issue(
                    &format!("{path}.derived_report.sha256"),
                    "must be a SHA-256 hexadecimal digest",
                ));
            }
        }
    }

    let mut assumption_ids = BTreeSet::new();
    for (index, assumption) in manifest.assumptions.iter().enumerate() {
        let path = format!("assumptions[{index}]");
        if !is_safe_identifier(&assumption.id) {
            errors.push(issue(&format!("{path}.id"), "must be a safe identifier"));
        }
        if !assumption_ids.insert(assumption.id.as_str()) {
            errors.push(issue(&format!("{path}.id"), "must be unique"));
        }
        if assumption.subject.trim().is_empty() || assumption.rationale.trim().is_empty() {
            errors.push(issue(&path, "subject and rationale must not be empty"));
        }
        if matches!(
            assumption.basis,
            AssumptionBasis::DocumentedEstimate | AssumptionBasis::MeasuredInput
        ) && assumption.source_ids.is_empty()
        {
            errors.push(issue(
                &format!("{path}.source_ids"),
                "must name at least one source for documented_estimate or measured_input",
            ));
        }
        let mut used_sources = BTreeSet::new();
        for source_id in &assumption.source_ids {
            if !is_safe_identifier(source_id) {
                errors.push(issue(
                    &format!("{path}.source_ids"),
                    "must contain only safe identifiers",
                ));
            }
            if !used_sources.insert(source_id.as_str()) {
                errors.push(issue(
                    &format!("{path}.source_ids"),
                    format!("repeats source `{source_id}`"),
                ));
            }
            if !source_ids.contains(source_id.as_str()) {
                errors.push(issue(
                    &format!("{path}.source_ids"),
                    format!("references unknown source `{source_id}`"),
                ));
            }
        }
    }

    let mut attested_sources = BTreeSet::new();
    let mut local_projection_sources = BTreeSet::new();
    let mut scenario_anchor_dependencies = Vec::new();
    for (index, attestation) in manifest.source_attestations.iter().enumerate() {
        let path = format!("source_attestations[{index}]");
        let source_id = attestation.source_id();
        if !is_safe_identifier(source_id) {
            errors.push(issue(
                &format!("{path}.source_id"),
                "must be a safe identifier",
            ));
        }
        if !attested_sources.insert(source_id) {
            errors.push(issue(
                &format!("{path}.source_id"),
                "must not be attested more than once",
            ));
        }
        let Some(source) = manifest
            .sources
            .iter()
            .find(|source| source.id == source_id)
        else {
            errors.push(issue(
                &format!("{path}.source_id"),
                format!("references unknown source `{source_id}`"),
            ));
            continue;
        };
        if source.derived_report.is_none() {
            errors.push(issue(
                &format!("{path}.source_id"),
                "must reference a source with a derived_report",
            ));
        }
        match attestation {
            ExperimentSourceAttestation::OsmLocalProjection {
                catalog_path,
                data_root,
                observation_report_path,
            } => {
                local_projection_sources.insert(source_id);
                for (field, value) in [
                    ("catalog_path", catalog_path),
                    ("data_root", data_root),
                    ("observation_report_path", observation_report_path),
                ] {
                    if !is_safe_relative_path(value) {
                        errors.push(issue(
                            &format!("{path}.{field}"),
                            "must be a non-empty relative path without `.` or `..` components",
                        ));
                    }
                }
            }
            ExperimentSourceAttestation::OsmScenarioAnchor {
                projection_source_id,
                anchor_manifest_path,
                ..
            } => {
                if source.source_sha256.is_none() {
                    errors.push(issue(
                        &format!("{path}.source_id"),
                        "must reference a source with source_sha256 for osm_scenario_anchor",
                    ));
                }
                if !is_safe_identifier(projection_source_id) {
                    errors.push(issue(
                        &format!("{path}.projection_source_id"),
                        "must be a safe identifier",
                    ));
                }
                if !is_safe_relative_path(anchor_manifest_path) {
                    errors.push(issue(
                        &format!("{path}.anchor_manifest_path"),
                        "must be a non-empty relative path without `.` or `..` components",
                    ));
                }
                scenario_anchor_dependencies.push((index, source_id, projection_source_id));
            }
        }
    }

    for (index, source_id, projection_source_id) in scenario_anchor_dependencies {
        let path = format!("source_attestations[{index}]");
        if source_id == projection_source_id {
            errors.push(issue(
                &format!("{path}.projection_source_id"),
                "must differ from source_id",
            ));
        }
        if !local_projection_sources.contains(projection_source_id) {
            errors.push(issue(
                &format!("{path}.projection_source_id"),
                "must name the source_id of an osm_local_projection attestation",
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn is_safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn is_safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn is_https_url(value: &str) -> bool {
    value.starts_with("https://") && value.len() > "https://".len()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn issue(path: &str, message: impl Into<String>) -> ExperimentValidationError {
    ExperimentValidationError {
        path: path.to_owned(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AssumptionBasis, ExperimentAssumption, ExperimentManifest, ExperimentSourceAttestation,
        SensitivityReference, validate_experiment_manifest,
    };
    use crate::SensitivityDerivedReport;

    fn manifest() -> ExperimentManifest {
        ExperimentManifest {
            schema_version: "0.1".to_owned(),
            name: "fixture".to_owned(),
            description: "one uncalibrated structural run".to_owned(),
            scenario_source: "scenario.chy".to_owned(),
            trace_every_steps: 10,
            assumptions: vec![ExperimentAssumption {
                id: "walking_speed".to_owned(),
                subject: "passengers.speed".to_owned(),
                basis: AssumptionBasis::DocumentedEstimate,
                rationale: "source bracket used as an explicit scenario input".to_owned(),
                source_ids: vec!["trajectory_report".to_owned()],
            }],
            sources: vec![SensitivityReference {
                id: "trajectory_report".to_owned(),
                citation: "Fixture (2026)".to_owned(),
                url: "https://example.test/report".to_owned(),
                applicability: "source selection context".to_owned(),
                limitation: "does not calibrate the runtime".to_owned(),
                source_sha256: None,
                derived_report: None,
            }],
            source_attestations: Vec::new(),
            claim_boundary: "not predictive or operational".to_owned(),
        }
    }

    #[test]
    fn manifest_requires_disclosed_assumptions_and_resolvable_sources() {
        assert!(validate_experiment_manifest(&manifest()).is_ok());

        let mut invalid = manifest();
        invalid.assumptions[0].source_ids = vec!["missing".to_owned()];
        invalid.scenario_source = "../scenario.chy".to_owned();
        assert!(validate_experiment_manifest(&invalid).is_err());

        let mut attested = manifest();
        attested.source_attestations = vec![ExperimentSourceAttestation::OsmLocalProjection {
            source_id: "trajectory_report".to_owned(),
            catalog_path: "catalog.json".to_owned(),
            data_root: "data".to_owned(),
            observation_report_path: "observation.json".to_owned(),
        }];
        assert!(validate_experiment_manifest(&attested).is_err());
        attested.schema_version = "0.2".to_owned();
        assert!(validate_experiment_manifest(&attested).is_err());
        attested.sources[0].derived_report = Some(SensitivityDerivedReport {
            path: "projection.json".to_owned(),
            sha256: "a".repeat(64),
        });
        assert!(validate_experiment_manifest(&attested).is_ok());

        let mut anchored = attested.clone();
        anchored.sources.push(SensitivityReference {
            id: "scenario_anchor".to_owned(),
            citation: "Fixture (2026)".to_owned(),
            url: "https://example.test/anchor".to_owned(),
            applicability: "retains one selected source point".to_owned(),
            limitation: "does not import a layout or calibrate the runtime".to_owned(),
            source_sha256: Some("b".repeat(64)),
            derived_report: Some(SensitivityDerivedReport {
                path: "anchor.json".to_owned(),
                sha256: "c".repeat(64),
            }),
        });
        anchored
            .source_attestations
            .push(ExperimentSourceAttestation::OsmScenarioAnchor {
                source_id: "scenario_anchor".to_owned(),
                projection_source_id: "trajectory_report".to_owned(),
                anchor_manifest_path: "anchors.json".to_owned(),
            });
        assert!(validate_experiment_manifest(&anchored).is_ok());

        anchored.source_attestations[1] = ExperimentSourceAttestation::OsmScenarioAnchor {
            source_id: "scenario_anchor".to_owned(),
            projection_source_id: "missing".to_owned(),
            anchor_manifest_path: "anchors.json".to_owned(),
        };
        assert!(validate_experiment_manifest(&anchored).is_err());
    }
}
