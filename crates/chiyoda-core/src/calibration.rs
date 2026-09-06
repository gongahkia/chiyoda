//! Descriptive, source-locked calibration intake for the Eindhoven platform data.
//!
//! This module does not alter simulation parameters. It produces a transparent
//! measurement report which a separately reviewed calibration protocol may use.
//! Keeping that distinction in code prevents a descriptive fit from silently
//! becoming a predictive or operational claim.

use crate::{
    EvidenceCatalog, EvidencePurpose,
    benchmark::DatasetRole,
    evidence::{EvidenceArchiveMember, validate_catalog},
};
use arrow_array::{
    Array, Float32Array, Float64Array, Int32Array, Int64Array, UInt32Array, UInt64Array,
};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};
use thiserror::Error;
use zip::ZipArchive;

const TIME_COLUMN: &str = "time_ms";
const ID_COLUMN: &str = "object_identifier";
const X_COLUMN: &str = "x_position_mm";
const Y_COLUMN: &str = "y_position_mm";
const HISTOGRAM_BIN_WIDTH_MPS: f64 = 0.01;
const HISTOGRAM_BIN_COUNT: usize = 400;
const DEFAULT_MAX_GAP_MS: i64 = 500;
const DEFAULT_MAX_SPEED_MPS: f64 = 4.0;
const DEFAULT_MIN_FREE_WALKING_SPEED_MPS: f64 = 0.5;

#[derive(Debug, Error)]
pub enum CalibrationError {
    #[error("evidence catalog is invalid: {0}")]
    InvalidCatalog(String),
    #[error("cannot read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: expected {expected} bytes, found {actual}")]
    SizeMismatch {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    #[error("{path}: SHA-256 mismatch; expected {expected}, calculated {actual}")]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    #[error("{path}: invalid ZIP archive: {message}")]
    Archive { path: PathBuf, message: String },
    #[error("{archive_path}: required archive member `{member_path}` is missing")]
    ArchiveMemberMissing {
        archive_path: PathBuf,
        member_path: String,
    },
    #[error("{archive_path}:{member_path}: expected {expected} bytes, found {actual}")]
    ArchiveMemberSizeMismatch {
        archive_path: PathBuf,
        member_path: String,
        expected: u64,
        actual: u64,
    },
    #[error(
        "{archive_path}:{member_path}: SHA-256 mismatch; expected {expected}, calculated {actual}"
    )]
    ArchiveMemberHashMismatch {
        archive_path: PathBuf,
        member_path: String,
        expected: String,
        actual: String,
    },
    #[error("{path}: invalid parquet input: {message}")]
    Parquet { path: PathBuf, message: String },
    #[error("{path}: expected numeric column `{column}`")]
    UnsupportedColumnType { path: PathBuf, column: &'static str },
    #[error("{path}: missing required column `{column}`")]
    MissingColumn { path: PathBuf, column: &'static str },
    #[error(
        "{path}: timestamps must be non-decreasing for the free-walking isolation screen; {actual} followed {previous}"
    )]
    TimestampOrder {
        path: PathBuf,
        previous: i64,
        actual: i64,
    },
    #[error("cannot serialize evidence catalog for provenance: {0}")]
    CatalogSerialization(serde_json::Error),
    #[error("cannot serialize calibration artifact for provenance: {0}")]
    ArtifactSerialization(serde_json::Error),
    #[error("invalid horizontal free-walking speed profile: {0}")]
    InvalidSpeedProfile(String),
    #[error("invalid horizontal free-walking held-out evaluation: {0}")]
    InvalidSpeedEvaluation(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservationFilter {
    pub max_gap_ms: i64,
    pub max_speed_mps: f64,
    pub histogram_bin_width_mps: f64,
}

impl Default for ObservationFilter {
    fn default() -> Self {
        Self {
            max_gap_ms: DEFAULT_MAX_GAP_MS,
            max_speed_mps: DEFAULT_MAX_SPEED_MPS,
            histogram_bin_width_mps: HISTOGRAM_BIN_WIDTH_MPS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ObservationCounts {
    pub rows: u64,
    pub usable_steps: u64,
    pub first_observations: u64,
    pub non_positive_time_steps: u64,
    pub gaps_over_limit: u64,
    pub speeds_over_limit: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeedSummary {
    pub samples: u64,
    pub mean_mps: f64,
    pub standard_deviation_mps: f64,
    /// Estimated from a fixed-width histogram; the bin width is in the report.
    pub p05_mps: f64,
    pub p50_mps: f64,
    pub p95_mps: f64,
    pub min_mps: f64,
    pub max_mps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileCalibrationSummary {
    pub id: String,
    pub role: DatasetRole,
    pub source_sha256: String,
    pub local_path: String,
    pub observations: ObservationCounts,
    pub speed: SpeedSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartitionCalibrationSummary {
    pub files: Vec<FileCalibrationSummary>,
    pub observations: ObservationCounts,
    pub speed: SpeedSummary,
}

/// Explicit accounting for the conservative free-walking isolation screen.
/// A retained step must have exactly one tracked object on the full platform at
/// both of its sampled endpoints. This is a reproducible screen for concurrent
/// tracked-pedestrian interaction, not a claim that all environmental effects
/// are absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FreeWalkingObservationCounts {
    pub rows: u64,
    pub frames: u64,
    pub singleton_frames: u64,
    pub multiple_object_frames: u64,
    pub first_observations: u64,
    pub non_positive_time_steps: u64,
    pub gaps_over_limit: u64,
    pub speeds_over_limit: u64,
    pub speeds_below_walking_threshold: u64,
    pub non_singleton_endpoint_steps: u64,
    pub usable_free_walking_steps: u64,
}

/// Fixed, prospective inclusion screen for the narrow free-walking profile.
/// Its minimum-speed threshold excludes stationary tracks and tracker jitter;
/// it is not inferred from either partition and is never a runtime default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FreeWalkingScreen {
    pub require_singleton_endpoint_frames: bool,
    pub minimum_speed_mps: f64,
}

impl Default for FreeWalkingScreen {
    fn default() -> Self {
        Self {
            require_singleton_endpoint_frames: true,
            minimum_speed_mps: DEFAULT_MIN_FREE_WALKING_SPEED_MPS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FreeWalkingFileSummary {
    pub id: String,
    pub role: DatasetRole,
    pub source_sha256: String,
    pub local_path: String,
    pub observations: FreeWalkingObservationCounts,
    pub speed: SpeedSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FreeWalkingPartitionSummary {
    pub files: Vec<FreeWalkingFileSummary>,
    pub observations: FreeWalkingObservationCounts,
    pub speed: SpeedSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatformCalibrationReport {
    pub schema_version: String,
    pub adapter_version: String,
    pub catalog_sha256: String,
    pub dataset_id: String,
    pub source_context: String,
    pub filter: ObservationFilter,
    pub partition: DatasetRole,
    pub summary: PartitionCalibrationSummary,
    /// The fixed claim boundary is data, rather than a caller-provided message.
    pub status: String,
    pub claim_boundary: String,
}

/// A source-locked, opt-in constant preferred-speed input. Its fitted value is
/// the isolation-screened calibration partition's fixed-histogram median, the
/// minimizer of the declared absolute-error objective. It is deliberately not
/// a population distribution, a route model, or an interaction model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FreeWalkingSpeedProfile {
    pub schema_version: String,
    pub profile_id: String,
    pub adapter_version: String,
    pub dataset_id: String,
    pub catalog_sha256: String,
    pub calibration_partition: DatasetRole,
    pub filter: ObservationFilter,
    pub screen: FreeWalkingScreen,
    pub model: String,
    pub calibration_objective: String,
    pub preferred_speed_mps: f64,
    pub calibration_summary: FreeWalkingPartitionSummary,
    /// SHA-256 of this artifact's canonical JSON payload with this field empty.
    pub profile_sha256: String,
    pub claim_boundary: String,
}

/// The locked held-out comparison for one [`FreeWalkingSpeedProfile`]. A
/// completed comparison is evidence about one temporal partition only; it is
/// intentionally not an automatic product-acceptance decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FreeWalkingSpeedHeldOutEvaluation {
    pub schema_version: String,
    pub adapter_version: String,
    pub dataset_id: String,
    pub catalog_sha256: String,
    pub profile_id: String,
    pub profile_sha256: String,
    pub held_out_partition: DatasetRole,
    pub filter: ObservationFilter,
    pub screen: FreeWalkingScreen,
    pub model: String,
    pub held_out_metric: String,
    pub preferred_speed_mps: f64,
    pub held_out_summary: FreeWalkingPartitionSummary,
    pub signed_median_error_mps: f64,
    pub absolute_median_error_mps: f64,
    pub status: String,
    /// SHA-256 of this artifact's canonical JSON payload with this field empty.
    pub evaluation_sha256: String,
    pub claim_boundary: String,
}

/// Generate a reproducible descriptive report for the known Eindhoven platform
/// schema. The caller must provide the exact root containing catalog-relative
/// files. Each source is size- and SHA-256-locked before it is read.
pub fn calibrate_eindhoven_platform(
    catalog: &EvidenceCatalog,
    data_root: &Path,
    partition: DatasetRole,
) -> Result<PlatformCalibrationReport, CalibrationError> {
    validate_eindhoven_catalog(catalog)?;

    let filter = ObservationFilter::default();
    let mut selected = PartitionAccumulator::default();
    for source in &catalog.files {
        if source.role.as_ref() != Some(&partition) {
            continue;
        }
        let path = data_root.join(&source.local_path);
        lock_source(&path, source.size_bytes, &source.sha256)?;
        let (summary, speeds) = summarize_eindhoven_file(&path, source, &filter)?;
        selected.add(summary, speeds);
    }

    Ok(PlatformCalibrationReport {
        schema_version: "0.1".to_owned(),
        adapter_version: crate::RUNTIME_VERSION.to_owned(),
        catalog_sha256: catalog_hash(catalog)?,
        dataset_id: catalog.dataset_id.clone(),
        source_context: "Anonymous 2D trajectories on a single Eindhoven Centraal train platform; positions are converted from millimetres to metres for speed estimation.".to_owned(),
        filter,
        partition,
        summary: selected.finish(),
        status: "descriptive_only".to_owned(),
        claim_boundary: "This report describes the locked source under its declared filter. It does not calibrate the reference runtime, validate stairs, lifts, gates, bodies, route choice, information effects, any population profile, or any facility; it cannot support operational or predictive claims.".to_owned(),
    })
}

fn validate_eindhoven_catalog(catalog: &EvidenceCatalog) -> Result<(), CalibrationError> {
    validate_catalog(catalog).map_err(|errors| {
        CalibrationError::InvalidCatalog(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    if catalog.purpose != EvidencePurpose::EmpiricalEvaluation {
        return Err(CalibrationError::InvalidCatalog(
            "the Eindhoven adapter accepts only an empirical_evaluation catalog".to_owned(),
        ));
    }
    if catalog.dataset_id != "eindhoven-centraal-platform-2024" {
        return Err(CalibrationError::InvalidCatalog(
            "the Eindhoven adapter requires dataset_id `eindhoven-centraal-platform-2024`"
                .to_owned(),
        ));
    }
    Ok(())
}

/// Fit the repository's single, pre-specified opt-in horizontal
/// free-walking-speed input on the locked calibration partition.
pub fn create_eindhoven_free_walking_speed_profile(
    catalog: &EvidenceCatalog,
    data_root: &Path,
    profile_id: &str,
) -> Result<FreeWalkingSpeedProfile, CalibrationError> {
    if profile_id.trim().is_empty() || profile_id.split_whitespace().count() != 1 {
        return Err(CalibrationError::InvalidSpeedProfile(
            "profile_id must be one non-empty DSL identifier token".to_owned(),
        ));
    }
    let calibration = summarize_eindhoven_free_walking_partition(
        catalog,
        data_root,
        &DatasetRole::Calibration,
        &FreeWalkingScreen::default(),
    )?;
    let preferred_speed_mps = calibration.speed.p50_mps;
    if !preferred_speed_mps.is_finite() || preferred_speed_mps <= 0.0 {
        return Err(CalibrationError::InvalidSpeedProfile(
            "calibration partition has no positive finite P50 speed under the fixed filter"
                .to_owned(),
        ));
    }
    let mut profile = FreeWalkingSpeedProfile {
        schema_version: "chiyoda.free-walking-speed-profile.v3".to_owned(),
        profile_id: profile_id.to_owned(),
        adapter_version: crate::RUNTIME_VERSION.to_owned(),
        dataset_id: catalog.dataset_id.clone(),
        catalog_sha256: catalog_hash(catalog)?,
        calibration_partition: DatasetRole::Calibration,
        filter: ObservationFilter::default(),
        screen: FreeWalkingScreen::default(),
        model: "constant_preferred_speed_equals_isolation_screened_calibration_histogram_p50"
            .to_owned(),
        calibration_objective: "Select the constant preferred speed minimizing absolute error to retained calibration consecutive-sample horizontal speeds whose two endpoint frames each contain exactly one tracked platform object; the fixed 0.01 m/s histogram P50 is the stored estimator.".to_owned(),
        preferred_speed_mps,
        calibration_summary: calibration,
        profile_sha256: String::new(),
        claim_boundary: "This opt-in input sets only AgentGroup.speed_mps for explicitly declared horizontal-free-walking groups. Its retained observations pass a strict endpoint screen requiring exactly one tracked platform object in each source frame; this screens concurrent tracked-pedestrian interaction but does not prove an absence of environmental influence. It is calibrated on one locked Eindhoven platform partition under the stated filter; it does not model or validate population distributions, free trajectories, interactions, avoidance, queues, routing, gates, stairs, lifts, escalators, station-wide behavior, prediction, operations, or safety.".to_owned(),
    };
    profile.profile_sha256 = free_walking_speed_profile_hash(&profile)?;
    Ok(profile)
}

/// Compare one source-locked profile with the catalog's locked held-out
/// partition. This function does not alter the profile or any runtime default.
pub fn evaluate_eindhoven_free_walking_speed_profile(
    catalog: &EvidenceCatalog,
    data_root: &Path,
    profile: &FreeWalkingSpeedProfile,
) -> Result<FreeWalkingSpeedHeldOutEvaluation, CalibrationError> {
    validate_free_walking_speed_profile(profile)?;
    let catalog_sha256 = catalog_hash(catalog)?;
    if profile.catalog_sha256 != catalog_sha256 {
        return Err(CalibrationError::InvalidSpeedProfile(
            "profile catalog SHA-256 does not match the supplied catalog".to_owned(),
        ));
    }
    if profile.dataset_id != catalog.dataset_id {
        return Err(CalibrationError::InvalidSpeedProfile(
            "profile dataset_id does not match the supplied catalog".to_owned(),
        ));
    }
    if profile.calibration_partition != DatasetRole::Calibration {
        return Err(CalibrationError::InvalidSpeedProfile(
            "profile must be fitted on the calibration partition".to_owned(),
        ));
    }
    let held_out = summarize_eindhoven_free_walking_partition(
        catalog,
        data_root,
        &DatasetRole::HeldOut,
        &profile.screen,
    )?;
    if profile.filter != ObservationFilter::default()
        || profile.screen != FreeWalkingScreen::default()
    {
        return Err(CalibrationError::InvalidSpeedProfile(
            "profile filter or free-walking screen does not match the fixed held-out adapter contract".to_owned(),
        ));
    }
    let held_out_p50_mps = held_out.speed.p50_mps;
    if !held_out_p50_mps.is_finite() || held_out.speed.samples == 0 {
        return Err(CalibrationError::InvalidSpeedEvaluation(
            "held-out partition has no finite retained horizontal-speed samples".to_owned(),
        ));
    }
    let signed_median_error_mps = profile.preferred_speed_mps - held_out_p50_mps;
    let mut evaluation = FreeWalkingSpeedHeldOutEvaluation {
        schema_version: "chiyoda.free-walking-speed-held-out-evaluation.v3".to_owned(),
        adapter_version: crate::RUNTIME_VERSION.to_owned(),
        dataset_id: catalog.dataset_id.clone(),
        catalog_sha256,
        profile_id: profile.profile_id.clone(),
        profile_sha256: profile.profile_sha256.clone(),
        held_out_partition: DatasetRole::HeldOut,
        filter: ObservationFilter::default(),
        screen: FreeWalkingScreen::default(),
        model: profile.model.clone(),
        held_out_metric: "Signed and absolute error between the calibration-fitted constant preferred speed and the held-out isolation-screened consecutive-sample horizontal-speed P50, using the same locked 0.01 m/s histogram estimator. A retained step requires exactly one tracked platform object in both endpoint frames.".to_owned(),
        preferred_speed_mps: profile.preferred_speed_mps,
        held_out_summary: held_out,
        signed_median_error_mps,
        absolute_median_error_mps: signed_median_error_mps.abs(),
        status: "held_out_comparison_complete_not_accepted".to_owned(),
        evaluation_sha256: String::new(),
        claim_boundary: "This comparison reports temporal held-out stability of one constant horizontal preferred-speed input on one Eindhoven platform under the fixed source filter and strict singleton-frame screen. It is not an automatic acceptance threshold or a validation that retained people are unaffected by all environmental factors; it does not validate trajectories, population behavior, avoidance, queues, routing, gates, stairs, lifts, escalators, a station, prediction, operations, or safety.".to_owned(),
    };
    evaluation.evaluation_sha256 = free_walking_speed_evaluation_hash(&evaluation)?;
    Ok(evaluation)
}

/// Render the complete DSL declaration required to use a reviewed profile.
/// Every required provenance digest is embedded so the runtime remains
/// self-contained and replayable without accessing the raw source files.
pub fn embedded_free_walking_speed_profile_declaration(
    profile: &FreeWalkingSpeedProfile,
    evaluation: &FreeWalkingSpeedHeldOutEvaluation,
) -> Result<String, CalibrationError> {
    validate_free_walking_speed_profile(profile)?;
    validate_free_walking_speed_evaluation(evaluation)?;
    if evaluation.profile_id != profile.profile_id
        || evaluation.profile_sha256 != profile.profile_sha256
        || evaluation.catalog_sha256 != profile.catalog_sha256
    {
        return Err(CalibrationError::InvalidSpeedEvaluation(
            "evaluation does not attest to the supplied profile and catalog".to_owned(),
        ));
    }
    Ok(format!(
        "walking-profile {} horizontal-free-walking speed {}m/s catalog-sha256 {} calibration-profile-sha256 {} held-out-evaluation-sha256 {}",
        profile.profile_id,
        profile.preferred_speed_mps,
        profile.catalog_sha256,
        profile.profile_sha256,
        evaluation.evaluation_sha256,
    ))
}

/// Verify the self-hash and minimum contract before an artifact can be used to
/// produce a DSL declaration or a held-out comparison.
pub fn validate_free_walking_speed_profile(
    profile: &FreeWalkingSpeedProfile,
) -> Result<(), CalibrationError> {
    if profile.schema_version != "chiyoda.free-walking-speed-profile.v3" {
        return Err(CalibrationError::InvalidSpeedProfile(
            "unsupported schema_version".to_owned(),
        ));
    }
    if profile.profile_id.trim().is_empty() || profile.profile_id.split_whitespace().count() != 1 {
        return Err(CalibrationError::InvalidSpeedProfile(
            "profile_id must be one non-empty DSL identifier token".to_owned(),
        ));
    }
    if !profile.preferred_speed_mps.is_finite() || profile.preferred_speed_mps <= 0.0 {
        return Err(CalibrationError::InvalidSpeedProfile(
            "preferred_speed_mps must be finite and greater than zero".to_owned(),
        ));
    }
    if profile.filter != ObservationFilter::default()
        || profile.screen != FreeWalkingScreen::default()
    {
        return Err(CalibrationError::InvalidSpeedProfile(
            "filter and free-walking screen must match the fixed protocol".to_owned(),
        ));
    }
    if profile.calibration_summary.speed.samples == 0 {
        return Err(CalibrationError::InvalidSpeedProfile(
            "calibration summary has no retained speed samples".to_owned(),
        ));
    }
    if !is_sha256(&profile.catalog_sha256) || !is_sha256(&profile.profile_sha256) {
        return Err(CalibrationError::InvalidSpeedProfile(
            "catalog_sha256 and profile_sha256 must be SHA-256 hexadecimal digests".to_owned(),
        ));
    }
    let expected_hash = free_walking_speed_profile_hash(profile)?;
    if profile.profile_sha256 != expected_hash {
        return Err(CalibrationError::InvalidSpeedProfile(format!(
            "profile_sha256 does not match the artifact payload; expected {expected_hash}, found {}",
            profile.profile_sha256
        )));
    }
    Ok(())
}

/// Verify that a held-out result is structurally linked to one self-verified
/// profile. It does not elevate the result to an acceptance claim.
pub fn validate_free_walking_speed_evaluation(
    evaluation: &FreeWalkingSpeedHeldOutEvaluation,
) -> Result<(), CalibrationError> {
    if evaluation.schema_version != "chiyoda.free-walking-speed-held-out-evaluation.v3" {
        return Err(CalibrationError::InvalidSpeedEvaluation(
            "unsupported schema_version".to_owned(),
        ));
    }
    if evaluation.held_out_partition != DatasetRole::HeldOut {
        return Err(CalibrationError::InvalidSpeedEvaluation(
            "held_out_partition must be held_out".to_owned(),
        ));
    }
    if evaluation.filter != ObservationFilter::default()
        || evaluation.screen != FreeWalkingScreen::default()
    {
        return Err(CalibrationError::InvalidSpeedEvaluation(
            "filter and free-walking screen must match the fixed protocol".to_owned(),
        ));
    }
    if evaluation.status != "held_out_comparison_complete_not_accepted" {
        return Err(CalibrationError::InvalidSpeedEvaluation(
            "status must preserve the non-acceptance boundary".to_owned(),
        ));
    }
    if evaluation.held_out_summary.speed.samples == 0
        || !evaluation.preferred_speed_mps.is_finite()
        || !evaluation.signed_median_error_mps.is_finite()
        || !evaluation.absolute_median_error_mps.is_finite()
        || evaluation.absolute_median_error_mps < 0.0
    {
        return Err(CalibrationError::InvalidSpeedEvaluation(
            "evaluation must contain finite retained summary and error values".to_owned(),
        ));
    }
    if !is_sha256(&evaluation.catalog_sha256)
        || !is_sha256(&evaluation.profile_sha256)
        || !is_sha256(&evaluation.evaluation_sha256)
    {
        return Err(CalibrationError::InvalidSpeedEvaluation(
            "catalog_sha256, profile_sha256, and evaluation_sha256 must be SHA-256 hexadecimal digests".to_owned(),
        ));
    }
    let expected_hash = free_walking_speed_evaluation_hash(evaluation)?;
    if evaluation.evaluation_sha256 != expected_hash {
        return Err(CalibrationError::InvalidSpeedEvaluation(format!(
            "evaluation_sha256 does not match the artifact payload; expected {expected_hash}, found {}",
            evaluation.evaluation_sha256
        )));
    }
    Ok(())
}

fn free_walking_speed_profile_hash(
    profile: &FreeWalkingSpeedProfile,
) -> Result<String, CalibrationError> {
    let mut unhashed = profile.clone();
    unhashed.profile_sha256.clear();
    artifact_hash(&unhashed)
}

fn free_walking_speed_evaluation_hash(
    evaluation: &FreeWalkingSpeedHeldOutEvaluation,
) -> Result<String, CalibrationError> {
    let mut unhashed = evaluation.clone();
    unhashed.evaluation_sha256.clear();
    artifact_hash(&unhashed)
}

fn artifact_hash<T: Serialize>(value: &T) -> Result<String, CalibrationError> {
    let encoded = serde_json::to_vec(value).map_err(CalibrationError::ArtifactSerialization)?;
    let canonical: serde_json::Value =
        serde_json::from_slice(&encoded).map_err(CalibrationError::ArtifactSerialization)?;
    let canonical =
        serde_json::to_vec(&canonical).map_err(CalibrationError::ArtifactSerialization)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn catalog_hash(catalog: &EvidenceCatalog) -> Result<String, CalibrationError> {
    let canonical = serde_json::to_vec(catalog).map_err(CalibrationError::CatalogSerialization)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

/// Verify every catalog-relative source file without interpreting its contents.
/// This is useful immediately after acquisition and before an expensive report.
pub fn verify_catalog_files(
    catalog: &EvidenceCatalog,
    data_root: &Path,
) -> Result<(), CalibrationError> {
    validate_catalog(catalog).map_err(|errors| {
        CalibrationError::InvalidCatalog(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    for source in &catalog.files {
        lock_source(
            &data_root.join(&source.local_path),
            source.size_bytes,
            &source.sha256,
        )?;
    }
    for member in &catalog.archive_members {
        let source = catalog
            .files
            .iter()
            .find(|source| source.id == member.archive_file_id)
            .ok_or_else(|| {
                CalibrationError::InvalidCatalog(format!(
                    "archive member `{}` references absent source `{}`",
                    member.id, member.archive_file_id
                ))
            })?;
        lock_archive_member(&data_root.join(&source.local_path), member)?;
    }
    Ok(())
}

fn lock_archive_member(
    archive_path: &Path,
    member: &EvidenceArchiveMember,
) -> Result<(), CalibrationError> {
    let file = File::open(archive_path).map_err(|source| CalibrationError::ReadFile {
        path: archive_path.to_owned(),
        source,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|error| CalibrationError::Archive {
        path: archive_path.to_owned(),
        message: error.to_string(),
    })?;
    let mut entry = archive.by_name(&member.member_path).map_err(|error| {
        if matches!(error, zip::result::ZipError::FileNotFound) {
            CalibrationError::ArchiveMemberMissing {
                archive_path: archive_path.to_owned(),
                member_path: member.member_path.clone(),
            }
        } else {
            CalibrationError::Archive {
                path: archive_path.to_owned(),
                message: error.to_string(),
            }
        }
    })?;
    let actual_size = entry.size();
    if actual_size != member.size_bytes {
        return Err(CalibrationError::ArchiveMemberSizeMismatch {
            archive_path: archive_path.to_owned(),
            member_path: member.member_path.clone(),
            expected: member.size_bytes,
            actual: actual_size,
        });
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 128 * 1024];
    loop {
        let read = entry
            .read(&mut buffer)
            .map_err(|source| CalibrationError::ReadFile {
                path: archive_path.to_owned(),
                source,
            })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual_hash = format!("{:x}", hasher.finalize());
    if !actual_hash.eq_ignore_ascii_case(&member.sha256) {
        return Err(CalibrationError::ArchiveMemberHashMismatch {
            archive_path: archive_path.to_owned(),
            member_path: member.member_path.clone(),
            expected: member.sha256.clone(),
            actual: actual_hash,
        });
    }
    Ok(())
}

fn lock_source(
    path: &Path,
    expected_size: u64,
    expected_hash: &str,
) -> Result<(), CalibrationError> {
    let metadata = std::fs::metadata(path).map_err(|source| CalibrationError::ReadFile {
        path: path.to_owned(),
        source,
    })?;
    if metadata.len() != expected_size {
        return Err(CalibrationError::SizeMismatch {
            path: path.to_owned(),
            expected: expected_size,
            actual: metadata.len(),
        });
    }
    let actual_hash = sha256_file(path)?;
    if !actual_hash.eq_ignore_ascii_case(expected_hash) {
        return Err(CalibrationError::HashMismatch {
            path: path.to_owned(),
            expected: expected_hash.to_owned(),
            actual: actual_hash,
        });
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, CalibrationError> {
    let file = File::open(path).map_err(|source| CalibrationError::ReadFile {
        path: path.to_owned(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 128 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|source| CalibrationError::ReadFile {
                path: path.to_owned(),
                source,
            })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn summarize_eindhoven_file(
    path: &Path,
    source: &crate::evidence::EvidenceFile,
    filter: &ObservationFilter,
) -> Result<(FileCalibrationSummary, RunningSpeedSummary), CalibrationError> {
    let file = File::open(path).map_err(|source_error| CalibrationError::ReadFile {
        path: path.to_owned(),
        source: source_error,
    })?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)
        .and_then(|builder| builder.with_batch_size(32_768).build())
        .map_err(|error| CalibrationError::Parquet {
            path: path.to_owned(),
            message: error.to_string(),
        })?;
    let mut accumulator = FileAccumulator::new(filter);
    for batch in reader {
        let batch = batch.map_err(|error| CalibrationError::Parquet {
            path: path.to_owned(),
            message: error.to_string(),
        })?;
        let time =
            batch
                .column_by_name(TIME_COLUMN)
                .ok_or_else(|| CalibrationError::MissingColumn {
                    path: path.to_owned(),
                    column: TIME_COLUMN,
                })?;
        let id =
            batch
                .column_by_name(ID_COLUMN)
                .ok_or_else(|| CalibrationError::MissingColumn {
                    path: path.to_owned(),
                    column: ID_COLUMN,
                })?;
        let x = batch
            .column_by_name(X_COLUMN)
            .ok_or_else(|| CalibrationError::MissingColumn {
                path: path.to_owned(),
                column: X_COLUMN,
            })?;
        let y = batch
            .column_by_name(Y_COLUMN)
            .ok_or_else(|| CalibrationError::MissingColumn {
                path: path.to_owned(),
                column: Y_COLUMN,
            })?;
        for index in 0..batch.num_rows() {
            let observation = Observation {
                id: identifier_at(id, index, path)?,
                time_ms: integer_at(time, index, TIME_COLUMN, path)?,
                x_mm: number_at(x, index, X_COLUMN, path)?,
                y_mm: number_at(y, index, Y_COLUMN, path)?,
            };
            accumulator.observe(observation);
        }
    }
    let role = source.role.clone().ok_or_else(|| {
        CalibrationError::InvalidCatalog(format!(
            "Eindhoven source `{}` must declare an empirical partition role",
            source.id
        ))
    })?;
    Ok(accumulator.finish(source, role))
}

fn summarize_eindhoven_free_walking_partition(
    catalog: &EvidenceCatalog,
    data_root: &Path,
    partition: &DatasetRole,
    screen: &FreeWalkingScreen,
) -> Result<FreeWalkingPartitionSummary, CalibrationError> {
    validate_eindhoven_catalog(catalog)?;
    let filter = ObservationFilter::default();
    let mut selected = FreeWalkingPartitionAccumulator::default();
    for source in &catalog.files {
        if source.role.as_ref() != Some(partition) {
            continue;
        }
        let path = data_root.join(&source.local_path);
        lock_source(&path, source.size_bytes, &source.sha256)?;
        let (summary, speeds) =
            summarize_eindhoven_free_walking_file(&path, source, &filter, screen)?;
        selected.add(summary, speeds);
    }
    Ok(selected.finish())
}

fn summarize_eindhoven_free_walking_file(
    path: &Path,
    source: &crate::evidence::EvidenceFile,
    filter: &ObservationFilter,
    screen: &FreeWalkingScreen,
) -> Result<(FreeWalkingFileSummary, RunningSpeedSummary), CalibrationError> {
    let file = File::open(path).map_err(|source_error| CalibrationError::ReadFile {
        path: path.to_owned(),
        source: source_error,
    })?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)
        .and_then(|builder| builder.with_batch_size(32_768).build())
        .map_err(|error| CalibrationError::Parquet {
            path: path.to_owned(),
            message: error.to_string(),
        })?;
    let mut accumulator = FreeWalkingFileAccumulator::new(filter, screen);
    for batch in reader {
        let batch = batch.map_err(|error| CalibrationError::Parquet {
            path: path.to_owned(),
            message: error.to_string(),
        })?;
        let time =
            batch
                .column_by_name(TIME_COLUMN)
                .ok_or_else(|| CalibrationError::MissingColumn {
                    path: path.to_owned(),
                    column: TIME_COLUMN,
                })?;
        let id =
            batch
                .column_by_name(ID_COLUMN)
                .ok_or_else(|| CalibrationError::MissingColumn {
                    path: path.to_owned(),
                    column: ID_COLUMN,
                })?;
        let x = batch
            .column_by_name(X_COLUMN)
            .ok_or_else(|| CalibrationError::MissingColumn {
                path: path.to_owned(),
                column: X_COLUMN,
            })?;
        let y = batch
            .column_by_name(Y_COLUMN)
            .ok_or_else(|| CalibrationError::MissingColumn {
                path: path.to_owned(),
                column: Y_COLUMN,
            })?;
        for index in 0..batch.num_rows() {
            accumulator.observe(
                Observation {
                    id: identifier_at(id, index, path)?,
                    time_ms: integer_at(time, index, TIME_COLUMN, path)?,
                    x_mm: number_at(x, index, X_COLUMN, path)?,
                    y_mm: number_at(y, index, Y_COLUMN, path)?,
                },
                path,
            )?;
        }
    }
    let role = source.role.clone().ok_or_else(|| {
        CalibrationError::InvalidCatalog(format!(
            "Eindhoven source `{}` must declare an empirical partition role",
            source.id
        ))
    })?;
    Ok(accumulator.finish(source, role))
}

fn identifier_at(
    column: &arrow_array::ArrayRef,
    index: usize,
    path: &Path,
) -> Result<u64, CalibrationError> {
    integer_at(column, index, ID_COLUMN, path).map(i64::cast_unsigned)
}

fn integer_at(
    column: &arrow_array::ArrayRef,
    index: usize,
    name: &'static str,
    path: &Path,
) -> Result<i64, CalibrationError> {
    if column.is_null(index) {
        return Err(CalibrationError::UnsupportedColumnType {
            path: path.to_owned(),
            column: name,
        });
    }
    if let Some(values) = column.as_any().downcast_ref::<Int64Array>() {
        Ok(values.value(index))
    } else if let Some(values) = column.as_any().downcast_ref::<Int32Array>() {
        Ok(i64::from(values.value(index)))
    } else if let Some(values) = column.as_any().downcast_ref::<UInt64Array>() {
        i64::try_from(values.value(index)).map_err(|_| CalibrationError::UnsupportedColumnType {
            path: path.to_owned(),
            column: name,
        })
    } else if let Some(values) = column.as_any().downcast_ref::<UInt32Array>() {
        Ok(i64::from(values.value(index)))
    } else {
        Err(CalibrationError::UnsupportedColumnType {
            path: path.to_owned(),
            column: name,
        })
    }
}

fn number_at(
    column: &arrow_array::ArrayRef,
    index: usize,
    name: &'static str,
    path: &Path,
) -> Result<f64, CalibrationError> {
    if column.is_null(index) {
        return Err(CalibrationError::UnsupportedColumnType {
            path: path.to_owned(),
            column: name,
        });
    }
    if let Some(values) = column.as_any().downcast_ref::<Int64Array>() {
        integer_to_f64(values.value(index), name, path)
    } else if let Some(values) = column.as_any().downcast_ref::<Int32Array>() {
        Ok(f64::from(values.value(index)))
    } else if let Some(values) = column.as_any().downcast_ref::<UInt64Array>() {
        unsigned_integer_to_f64(values.value(index), name, path)
    } else if let Some(values) = column.as_any().downcast_ref::<UInt32Array>() {
        Ok(f64::from(values.value(index)))
    } else if let Some(values) = column.as_any().downcast_ref::<Float64Array>() {
        Ok(values.value(index))
    } else if let Some(values) = column.as_any().downcast_ref::<Float32Array>() {
        Ok(f64::from(values.value(index)))
    } else {
        Err(CalibrationError::UnsupportedColumnType {
            path: path.to_owned(),
            column: name,
        })
    }
}

fn integer_to_f64(value: i64, name: &'static str, path: &Path) -> Result<f64, CalibrationError> {
    i32::try_from(value)
        .map(f64::from)
        .map_err(|_| CalibrationError::UnsupportedColumnType {
            path: path.to_owned(),
            column: name,
        })
}

fn unsigned_integer_to_f64(
    value: u64,
    name: &'static str,
    path: &Path,
) -> Result<f64, CalibrationError> {
    u32::try_from(value)
        .map(f64::from)
        .map_err(|_| CalibrationError::UnsupportedColumnType {
            path: path.to_owned(),
            column: name,
        })
}

#[derive(Debug, Clone, Copy)]
struct Observation {
    id: u64,
    time_ms: i64,
    x_mm: f64,
    y_mm: f64,
}

#[derive(Debug, Clone, Copy)]
struct PreviousObservation {
    time_ms: i64,
    x_mm: f64,
    y_mm: f64,
}

#[derive(Debug)]
struct FileAccumulator {
    filter: ObservationFilter,
    previous: HashMap<u64, PreviousObservation>,
    counts: ObservationCounts,
    speeds: RunningSpeedSummary,
}

impl FileAccumulator {
    fn new(filter: &ObservationFilter) -> Self {
        Self {
            filter: filter.clone(),
            previous: HashMap::new(),
            counts: ObservationCounts::default(),
            speeds: RunningSpeedSummary::new(filter),
        }
    }

    fn observe(&mut self, observation: Observation) {
        self.counts.rows += 1;
        let previous = self.previous.insert(
            observation.id,
            PreviousObservation {
                time_ms: observation.time_ms,
                x_mm: observation.x_mm,
                y_mm: observation.y_mm,
            },
        );
        let Some(previous) = previous else {
            self.counts.first_observations += 1;
            return;
        };
        let elapsed_ms = observation.time_ms - previous.time_ms;
        if elapsed_ms <= 0 {
            self.counts.non_positive_time_steps += 1;
            return;
        }
        if elapsed_ms > self.filter.max_gap_ms {
            self.counts.gaps_over_limit += 1;
            return;
        }
        let displacement_components_m = (
            (observation.x_mm - previous.x_mm) / 1000.0,
            (observation.y_mm - previous.y_mm) / 1000.0,
        );
        let elapsed_ms = i32::try_from(elapsed_ms).expect("positive gap is limited to 500 ms");
        let speed_mps = (displacement_components_m.0.mul_add(
            displacement_components_m.0,
            displacement_components_m.1 * displacement_components_m.1,
        ))
        .sqrt()
            / (f64::from(elapsed_ms) / 1000.0);
        if !speed_mps.is_finite() || speed_mps > self.filter.max_speed_mps {
            self.counts.speeds_over_limit += 1;
            return;
        }
        self.counts.usable_steps += 1;
        self.speeds.add(speed_mps);
    }

    fn finish(
        self,
        source: &crate::evidence::EvidenceFile,
        role: DatasetRole,
    ) -> (FileCalibrationSummary, RunningSpeedSummary) {
        let Self { counts, speeds, .. } = self;
        let summary = FileCalibrationSummary {
            id: source.id.clone(),
            role,
            source_sha256: source.sha256.clone(),
            local_path: source.local_path.clone(),
            observations: counts,
            speed: speeds.finish(),
        };
        (summary, speeds)
    }
}

#[derive(Debug, Clone, Copy)]
struct PreviousFreeWalkingObservation {
    observation: Observation,
    was_singleton_frame: bool,
}

#[derive(Debug)]
struct FreeWalkingFileAccumulator {
    filter: ObservationFilter,
    screen: FreeWalkingScreen,
    previous: HashMap<u64, PreviousFreeWalkingObservation>,
    current_time_ms: Option<i64>,
    current_frame: Vec<Observation>,
    counts: FreeWalkingObservationCounts,
    speeds: RunningSpeedSummary,
}

impl FreeWalkingFileAccumulator {
    fn new(filter: &ObservationFilter, screen: &FreeWalkingScreen) -> Self {
        Self {
            filter: filter.clone(),
            screen: screen.clone(),
            previous: HashMap::new(),
            current_time_ms: None,
            current_frame: Vec::new(),
            counts: FreeWalkingObservationCounts::default(),
            speeds: RunningSpeedSummary::new(filter),
        }
    }

    fn observe(&mut self, observation: Observation, path: &Path) -> Result<(), CalibrationError> {
        match self.current_time_ms {
            Some(current_time_ms) if observation.time_ms < current_time_ms => {
                return Err(CalibrationError::TimestampOrder {
                    path: path.to_owned(),
                    previous: current_time_ms,
                    actual: observation.time_ms,
                });
            }
            Some(current_time_ms) if observation.time_ms > current_time_ms => {
                self.finish_frame();
                self.current_time_ms = Some(observation.time_ms);
            }
            Some(_) => {}
            None => self.current_time_ms = Some(observation.time_ms),
        }
        self.counts.rows += 1;
        self.current_frame.push(observation);
        Ok(())
    }

    fn finish_frame(&mut self) {
        if self.current_frame.is_empty() {
            return;
        }
        let singleton_frame = self.current_frame.len() == 1;
        self.counts.frames += 1;
        if singleton_frame {
            self.counts.singleton_frames += 1;
        } else {
            self.counts.multiple_object_frames += 1;
        }
        for observation in self.current_frame.drain(..) {
            let previous = self.previous.insert(
                observation.id,
                PreviousFreeWalkingObservation {
                    observation,
                    was_singleton_frame: singleton_frame,
                },
            );
            let Some(previous) = previous else {
                self.counts.first_observations += 1;
                continue;
            };
            let elapsed_ms = observation.time_ms - previous.observation.time_ms;
            if elapsed_ms <= 0 {
                self.counts.non_positive_time_steps += 1;
                continue;
            }
            if elapsed_ms > self.filter.max_gap_ms {
                self.counts.gaps_over_limit += 1;
                continue;
            }
            let displacement_components_m = (
                (observation.x_mm - previous.observation.x_mm) / 1000.0,
                (observation.y_mm - previous.observation.y_mm) / 1000.0,
            );
            let elapsed_ms = i32::try_from(elapsed_ms).expect("positive gap is limited to 500 ms");
            let speed_mps = (displacement_components_m.0.mul_add(
                displacement_components_m.0,
                displacement_components_m.1 * displacement_components_m.1,
            ))
            .sqrt()
                / (f64::from(elapsed_ms) / 1000.0);
            if !speed_mps.is_finite() || speed_mps > self.filter.max_speed_mps {
                self.counts.speeds_over_limit += 1;
                continue;
            }
            if speed_mps < self.screen.minimum_speed_mps {
                self.counts.speeds_below_walking_threshold += 1;
                continue;
            }
            if self.screen.require_singleton_endpoint_frames
                && (!previous.was_singleton_frame || !singleton_frame)
            {
                self.counts.non_singleton_endpoint_steps += 1;
                continue;
            }
            self.counts.usable_free_walking_steps += 1;
            self.speeds.add(speed_mps);
        }
        self.current_time_ms = None;
    }

    fn finish(
        mut self,
        source: &crate::evidence::EvidenceFile,
        role: DatasetRole,
    ) -> (FreeWalkingFileSummary, RunningSpeedSummary) {
        self.finish_frame();
        let Self { counts, speeds, .. } = self;
        let summary = FreeWalkingFileSummary {
            id: source.id.clone(),
            role,
            source_sha256: source.sha256.clone(),
            local_path: source.local_path.clone(),
            observations: counts,
            speed: speeds.finish(),
        };
        (summary, speeds)
    }
}

#[derive(Debug, Clone)]
struct RunningSpeedSummary {
    samples: u64,
    mean_mps: f64,
    sum_squared_differences: f64,
    min_mps: f64,
    max_mps: f64,
    sample_weight: f64,
    bin_width_mps: f64,
    bins: Vec<u64>,
}

impl RunningSpeedSummary {
    fn new(filter: &ObservationFilter) -> Self {
        Self {
            samples: 0,
            mean_mps: 0.0,
            sum_squared_differences: 0.0,
            min_mps: f64::INFINITY,
            max_mps: f64::NEG_INFINITY,
            sample_weight: 0.0,
            bin_width_mps: filter.histogram_bin_width_mps,
            bins: vec![0; HISTOGRAM_BIN_COUNT],
        }
    }

    fn add(&mut self, speed_mps: f64) {
        self.samples += 1;
        self.sample_weight += 1.0;
        let difference = speed_mps - self.mean_mps;
        self.mean_mps += difference / self.sample_weight;
        self.sum_squared_differences += difference * (speed_mps - self.mean_mps);
        self.min_mps = self.min_mps.min(speed_mps);
        self.max_mps = self.max_mps.max(speed_mps);
        // The caller has already rejected negatives and values above the fixed
        // maximum. The finite conversion indexes a bounded 400-bin histogram.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bin = (speed_mps / self.bin_width_mps).floor() as usize;
        let last = self.bins.len() - 1;
        self.bins[bin.min(last)] += 1;
    }

    fn merge(&mut self, other: &Self) {
        if other.samples == 0 {
            return;
        }
        if self.samples == 0 {
            *self = other.clone();
            return;
        }
        let combined = self.samples + other.samples;
        let combined_weight = self.sample_weight + other.sample_weight;
        let difference = other.mean_mps - self.mean_mps;
        self.sum_squared_differences += other.sum_squared_differences
            + difference * difference * self.sample_weight * other.sample_weight / combined_weight;
        self.mean_mps += difference * other.sample_weight / combined_weight;
        self.samples = combined;
        self.sample_weight = combined_weight;
        self.min_mps = self.min_mps.min(other.min_mps);
        self.max_mps = self.max_mps.max(other.max_mps);
        for (left, right) in self.bins.iter_mut().zip(&other.bins) {
            *left += right;
        }
    }

    fn finish(&self) -> SpeedSummary {
        if self.samples == 0 {
            return SpeedSummary {
                samples: 0,
                mean_mps: 0.0,
                standard_deviation_mps: 0.0,
                p05_mps: 0.0,
                p50_mps: 0.0,
                p95_mps: 0.0,
                min_mps: 0.0,
                max_mps: 0.0,
            };
        }
        SpeedSummary {
            samples: self.samples,
            mean_mps: self.mean_mps,
            standard_deviation_mps: (self.sum_squared_differences / self.sample_weight).sqrt(),
            p05_mps: self.quantile(Quantile::P05),
            p50_mps: self.quantile(Quantile::P50),
            p95_mps: self.quantile(Quantile::P95),
            min_mps: self.min_mps,
            max_mps: self.max_mps,
        }
    }

    fn quantile(&self, quantile: Quantile) -> f64 {
        let (numerator, denominator) = match quantile {
            Quantile::P05 => (1, 20),
            Quantile::P50 => (1, 2),
            Quantile::P95 => (19, 20),
        };
        let target = (self.samples - 1)
            .saturating_mul(numerator)
            .div_ceil(denominator);
        let mut cumulative = 0;
        for (index, count) in self.bins.iter().enumerate() {
            cumulative += count;
            if cumulative > target {
                let index = u16::try_from(index).expect("histogram has 400 bins");
                return (f64::from(index) + 0.5) * self.bin_width_mps;
            }
        }
        self.max_mps
    }
}

#[derive(Debug, Clone, Copy)]
enum Quantile {
    P05,
    P50,
    P95,
}

#[derive(Debug, Default)]
struct PartitionAccumulator {
    files: Vec<FileCalibrationSummary>,
    counts: ObservationCounts,
    speeds: Option<RunningSpeedSummary>,
}

impl PartitionAccumulator {
    fn add(&mut self, summary: FileCalibrationSummary, speeds: RunningSpeedSummary) {
        self.counts.rows += summary.observations.rows;
        self.counts.usable_steps += summary.observations.usable_steps;
        self.counts.first_observations += summary.observations.first_observations;
        self.counts.non_positive_time_steps += summary.observations.non_positive_time_steps;
        self.counts.gaps_over_limit += summary.observations.gaps_over_limit;
        self.counts.speeds_over_limit += summary.observations.speeds_over_limit;
        if let Some(aggregate) = &mut self.speeds {
            aggregate.merge(&speeds);
        } else {
            self.speeds = Some(speeds);
        }
        self.files.push(summary);
    }

    fn finish(self) -> PartitionCalibrationSummary {
        let filter = ObservationFilter::default();
        let aggregate = self
            .speeds
            .unwrap_or_else(|| RunningSpeedSummary::new(&filter));
        PartitionCalibrationSummary {
            files: self.files,
            observations: self.counts,
            speed: aggregate.finish(),
        }
    }
}

#[derive(Debug, Default)]
struct FreeWalkingPartitionAccumulator {
    files: Vec<FreeWalkingFileSummary>,
    counts: FreeWalkingObservationCounts,
    speeds: Option<RunningSpeedSummary>,
}

impl FreeWalkingPartitionAccumulator {
    fn add(&mut self, summary: FreeWalkingFileSummary, speeds: RunningSpeedSummary) {
        self.counts.rows += summary.observations.rows;
        self.counts.frames += summary.observations.frames;
        self.counts.singleton_frames += summary.observations.singleton_frames;
        self.counts.multiple_object_frames += summary.observations.multiple_object_frames;
        self.counts.first_observations += summary.observations.first_observations;
        self.counts.non_positive_time_steps += summary.observations.non_positive_time_steps;
        self.counts.gaps_over_limit += summary.observations.gaps_over_limit;
        self.counts.speeds_over_limit += summary.observations.speeds_over_limit;
        self.counts.speeds_below_walking_threshold +=
            summary.observations.speeds_below_walking_threshold;
        self.counts.non_singleton_endpoint_steps +=
            summary.observations.non_singleton_endpoint_steps;
        self.counts.usable_free_walking_steps += summary.observations.usable_free_walking_steps;
        if let Some(aggregate) = &mut self.speeds {
            aggregate.merge(&speeds);
        } else {
            self.speeds = Some(speeds);
        }
        self.files.push(summary);
    }

    fn finish(self) -> FreeWalkingPartitionSummary {
        let filter = ObservationFilter::default();
        let aggregate = self
            .speeds
            .unwrap_or_else(|| RunningSpeedSummary::new(&filter));
        FreeWalkingPartitionSummary {
            files: self.files,
            observations: self.counts,
            speed: aggregate.finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CalibrationError, FileAccumulator, FreeWalkingFileAccumulator,
        FreeWalkingObservationCounts, FreeWalkingPartitionSummary, FreeWalkingScreen,
        FreeWalkingSpeedHeldOutEvaluation, FreeWalkingSpeedProfile, Observation, ObservationFilter,
        PlatformCalibrationReport, RunningSpeedSummary, SpeedSummary, calibrate_eindhoven_platform,
        catalog_hash, embedded_free_walking_speed_profile_declaration,
        free_walking_speed_evaluation_hash, free_walking_speed_profile_hash,
        validate_free_walking_speed_evaluation, validate_free_walking_speed_profile,
        verify_catalog_files,
    };
    use crate::{
        EvidenceArchiveMember, EvidenceCatalog, EvidencePurpose, benchmark::DatasetRole,
        evidence::EvidenceFile, parse, validate,
    };
    use sha2::{Digest, Sha256};
    use std::{fs, io::Write, path::Path};
    use zip::{ZipWriter, write::SimpleFileOptions};

    #[test]
    fn observation_filter_counts_gaps_and_outliers_without_hiding_them() {
        let filter = ObservationFilter::default();
        let mut accumulator = FileAccumulator::new(&filter);
        accumulator.observe(Observation {
            id: 1,
            time_ms: 0,
            x_mm: 0.0,
            y_mm: 0.0,
        });
        accumulator.observe(Observation {
            id: 1,
            time_ms: 100,
            x_mm: 100.0,
            y_mm: 0.0,
        });
        accumulator.observe(Observation {
            id: 1,
            time_ms: 700,
            x_mm: 200.0,
            y_mm: 0.0,
        });
        accumulator.observe(Observation {
            id: 1,
            time_ms: 800,
            x_mm: 2000.0,
            y_mm: 0.0,
        });
        assert_eq!(accumulator.counts.usable_steps, 1);
        assert_eq!(accumulator.counts.gaps_over_limit, 1);
        assert_eq!(accumulator.counts.speeds_over_limit, 1);
    }

    #[test]
    fn merged_speed_statistics_preserve_the_weighted_mean() {
        let filter = ObservationFilter::default();
        let mut first = RunningSpeedSummary::new(&filter);
        first.add(1.0);
        let mut second = RunningSpeedSummary::new(&filter);
        second.add(3.0);
        first.merge(&second);
        assert!((first.finish().mean_mps - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn free_walking_screen_requires_singleton_frames_at_both_step_endpoints() {
        let filter = ObservationFilter::default();
        let mut accumulator =
            FreeWalkingFileAccumulator::new(&filter, &FreeWalkingScreen::default());
        let path = Path::new("fixture.parquet");
        for observation in [
            Observation {
                id: 1,
                time_ms: 0,
                x_mm: 0.0,
                y_mm: 0.0,
            },
            Observation {
                id: 1,
                time_ms: 100,
                x_mm: 100.0,
                y_mm: 0.0,
            },
            Observation {
                id: 1,
                time_ms: 200,
                x_mm: 200.0,
                y_mm: 0.0,
            },
            Observation {
                id: 2,
                time_ms: 200,
                x_mm: 1_000.0,
                y_mm: 0.0,
            },
            Observation {
                id: 1,
                time_ms: 300,
                x_mm: 300.0,
                y_mm: 0.0,
            },
            Observation {
                id: 1,
                time_ms: 400,
                x_mm: 400.0,
                y_mm: 0.0,
            },
        ] {
            accumulator
                .observe(observation, path)
                .expect("fixture timestamps are ordered");
        }
        accumulator.finish_frame();

        assert_eq!(accumulator.counts.singleton_frames, 4);
        assert_eq!(accumulator.counts.multiple_object_frames, 1);
        assert_eq!(accumulator.counts.usable_free_walking_steps, 2);
        assert_eq!(accumulator.counts.non_singleton_endpoint_steps, 2);
        assert_eq!(accumulator.speeds.finish().samples, 2);
    }

    #[test]
    fn free_walking_screen_excludes_stationary_and_jitter_like_steps() {
        let filter = ObservationFilter::default();
        let mut accumulator =
            FreeWalkingFileAccumulator::new(&filter, &FreeWalkingScreen::default());
        let path = Path::new("fixture.parquet");
        for observation in [
            Observation {
                id: 1,
                time_ms: 0,
                x_mm: 0.0,
                y_mm: 0.0,
            },
            Observation {
                id: 1,
                time_ms: 100,
                x_mm: 10.0,
                y_mm: 0.0,
            },
            Observation {
                id: 1,
                time_ms: 200,
                x_mm: 110.0,
                y_mm: 0.0,
            },
        ] {
            accumulator
                .observe(observation, path)
                .expect("fixture timestamps are ordered");
        }
        accumulator.finish_frame();

        assert_eq!(accumulator.counts.speeds_below_walking_threshold, 1);
        assert_eq!(accumulator.counts.usable_free_walking_steps, 1);
        assert!((accumulator.speeds.finish().p50_mps - 1.005).abs() < 1e-12);
    }

    #[test]
    fn calibration_adapter_rejects_a_source_only_catalog_before_reading_files() {
        let catalog = EvidenceCatalog {
            schema_version: "0.1".to_owned(),
            purpose: EvidencePurpose::UncalibratedReference,
            dataset_id: "eindhoven-centraal-platform-2024".to_owned(),
            title: "Source-only fixture".to_owned(),
            landing_page: "https://example.test/record".to_owned(),
            license: "CC-BY-4.0".to_owned(),
            redistributable: true,
            attribution: None,
            citation: "Fixture (2026)".to_owned(),
            files: vec![EvidenceFile {
                id: "source".to_owned(),
                role: None,
                source_url: "https://example.test/source".to_owned(),
                local_path: "missing.parquet".to_owned(),
                sha256: "a".repeat(64),
                size_bytes: 1,
                upstream_checksum: Some("md5:fixture".to_owned()),
                transformation: "retain source values".to_owned(),
            }],
            archive_members: Vec::new(),
            supported_primitives: "fixture only".to_owned(),
            exclusions: "empirical evaluation".to_owned(),
            split_rationale: None,
        };

        let error =
            calibrate_eindhoven_platform(&catalog, Path::new("missing"), DatasetRole::Calibration)
                .expect_err("source-only catalog must be rejected");

        assert!(
            matches!(error, CalibrationError::InvalidCatalog(message) if message.contains("empirical_evaluation"))
        );
    }

    #[test]
    fn archive_member_locks_verify_the_published_split_members() {
        let directory = std::env::temp_dir().join(format!(
            "chiyoda-archive-lock-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        fs::create_dir_all(&directory).expect("creating temporary directory");
        let archive_path = directory.join("trials.zip");
        let calibration = b"calibration trial";
        let held_out = b"held-out trial";
        let archive_file = fs::File::create(&archive_path).expect("creating archive");
        let mut archive = ZipWriter::new(archive_file);
        archive
            .start_file("trials/calibration.txt", SimpleFileOptions::default())
            .expect("starting calibration member");
        archive
            .write_all(calibration)
            .expect("writing calibration member");
        archive
            .start_file("trials/held-out.txt", SimpleFileOptions::default())
            .expect("starting held-out member");
        archive
            .write_all(held_out)
            .expect("writing held-out member");
        archive.finish().expect("finishing archive");
        let archive_bytes = fs::read(&archive_path).expect("reading archive");
        let catalog = EvidenceCatalog {
            schema_version: "0.1".to_owned(),
            purpose: EvidencePurpose::EmpiricalEvaluation,
            dataset_id: "archive-fixture".to_owned(),
            title: "Archive fixture".to_owned(),
            landing_page: "https://example.test/archive".to_owned(),
            license: "CC0-1.0".to_owned(),
            redistributable: true,
            attribution: None,
            citation: "Fixture (2026)".to_owned(),
            files: vec![EvidenceFile {
                id: "archive".to_owned(),
                role: None,
                source_url: "https://example.test/trials.zip".to_owned(),
                local_path: "trials.zip".to_owned(),
                sha256: format!("{:x}", Sha256::digest(&archive_bytes)),
                size_bytes: u64::try_from(archive_bytes.len()).expect("usize fits u64"),
                upstream_checksum: None,
                transformation: "retain the publisher ZIP unchanged".to_owned(),
            }],
            archive_members: vec![
                EvidenceArchiveMember {
                    id: "calibration".to_owned(),
                    archive_file_id: "archive".to_owned(),
                    member_path: "trials/calibration.txt".to_owned(),
                    role: Some(DatasetRole::Calibration),
                    sha256: format!("{:x}", Sha256::digest(calibration)),
                    size_bytes: u64::try_from(calibration.len()).expect("usize fits u64"),
                    transformation: "read the source trial unchanged".to_owned(),
                },
                EvidenceArchiveMember {
                    id: "held-out".to_owned(),
                    archive_file_id: "archive".to_owned(),
                    member_path: "trials/held-out.txt".to_owned(),
                    role: Some(DatasetRole::HeldOut),
                    sha256: format!("{:x}", Sha256::digest(held_out)),
                    size_bytes: u64::try_from(held_out.len()).expect("usize fits u64"),
                    transformation: "read the source trial unchanged".to_owned(),
                },
            ],
            supported_primitives: "fixture horizontal avoidance".to_owned(),
            exclusions: "all other primitives".to_owned(),
            split_rationale: Some("named archive members are disjoint trials".to_owned()),
        };

        verify_catalog_files(&catalog, &directory).expect("archive and members lock");
        fs::remove_dir_all(directory).expect("removing temporary directory");
    }

    #[test]
    fn empirical_catalog_hash_is_compatible_with_the_checked_in_intake_report() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalog: EvidenceCatalog = serde_json::from_str(
            &std::fs::read_to_string(
                root.join("benchmarks/evidence/eindhoven-centraal-platform-2024.json"),
            )
            .expect("reading checked-in catalog"),
        )
        .expect("parsing checked-in catalog");
        let report: PlatformCalibrationReport = serde_json::from_str(
            &std::fs::read_to_string(
                root.join("benchmarks/reports/eindhoven-platform-calibration-intake.json"),
            )
            .expect("reading checked-in intake report"),
        )
        .expect("parsing checked-in intake report");

        assert_eq!(
            catalog_hash(&catalog).expect("hashing catalog"),
            report.catalog_sha256
        );
    }

    fn partition_summary(p50_mps: f64) -> FreeWalkingPartitionSummary {
        FreeWalkingPartitionSummary {
            files: Vec::new(),
            observations: FreeWalkingObservationCounts {
                usable_free_walking_steps: 10,
                ..FreeWalkingObservationCounts::default()
            },
            speed: SpeedSummary {
                samples: 10,
                mean_mps: p50_mps,
                standard_deviation_mps: 0.1,
                p05_mps: p50_mps - 0.1,
                p50_mps,
                p95_mps: p50_mps + 0.1,
                min_mps: p50_mps - 0.2,
                max_mps: p50_mps + 0.2,
            },
        }
    }

    fn profile_fixture() -> FreeWalkingSpeedProfile {
        let mut profile = FreeWalkingSpeedProfile {
            schema_version: "chiyoda.free-walking-speed-profile.v3".to_owned(),
            profile_id: "eindhoven_free".to_owned(),
            adapter_version: "fixture".to_owned(),
            dataset_id: "eindhoven-centraal-platform-2024".to_owned(),
            catalog_sha256: "a".repeat(64),
            calibration_partition: DatasetRole::Calibration,
            filter: ObservationFilter::default(),
            screen: FreeWalkingScreen::default(),
            model: "constant_preferred_speed_equals_isolation_screened_calibration_histogram_p50"
                .to_owned(),
            calibration_objective: "fixture objective".to_owned(),
            preferred_speed_mps: 1.25,
            calibration_summary: partition_summary(1.25),
            profile_sha256: String::new(),
            claim_boundary: "fixture boundary".to_owned(),
        };
        profile.profile_sha256 =
            free_walking_speed_profile_hash(&profile).expect("fixture profile hashes");
        profile
    }

    fn evaluation_fixture(profile: &FreeWalkingSpeedProfile) -> FreeWalkingSpeedHeldOutEvaluation {
        let mut evaluation = FreeWalkingSpeedHeldOutEvaluation {
            schema_version: "chiyoda.free-walking-speed-held-out-evaluation.v3".to_owned(),
            adapter_version: "fixture".to_owned(),
            dataset_id: "eindhoven-centraal-platform-2024".to_owned(),
            catalog_sha256: profile.catalog_sha256.clone(),
            profile_id: profile.profile_id.clone(),
            profile_sha256: profile.profile_sha256.clone(),
            held_out_partition: DatasetRole::HeldOut,
            filter: ObservationFilter::default(),
            screen: FreeWalkingScreen::default(),
            model: profile.model.clone(),
            held_out_metric: "fixture metric".to_owned(),
            preferred_speed_mps: profile.preferred_speed_mps,
            held_out_summary: partition_summary(1.2),
            signed_median_error_mps: 0.05,
            absolute_median_error_mps: 0.05,
            status: "held_out_comparison_complete_not_accepted".to_owned(),
            evaluation_sha256: String::new(),
            claim_boundary: "fixture boundary".to_owned(),
        };
        evaluation.evaluation_sha256 =
            free_walking_speed_evaluation_hash(&evaluation).expect("fixture evaluation hashes");
        evaluation
    }

    #[test]
    fn free_walking_artifacts_are_self_verifying_and_emit_embedded_provenance() {
        let profile = profile_fixture();
        let evaluation = evaluation_fixture(&profile);
        validate_free_walking_speed_profile(&profile).expect("profile self-verifies");
        validate_free_walking_speed_evaluation(&evaluation).expect("evaluation self-verifies");

        let declaration = embedded_free_walking_speed_profile_declaration(&profile, &evaluation)
            .expect("linked artifacts emit a DSL declaration");
        assert!(declaration.starts_with("walking-profile eindhoven_free horizontal-free-walking"));
        assert!(declaration.contains(&profile.profile_sha256));
        assert!(declaration.contains(&evaluation.evaluation_sha256));

        let source = format!(
            "scenario \"embedded profile fixture\"\nseed 7\nduration 10s\ntimestep 1s\n{declaration}\nsurface platform at (0m, 0m, 0m) size (20m, 10m)\nexit street on platform at (18m, 5m, 0m) width 2m\nagents passengers count 1 on platform at (1m, 5m, 0m) to street speed profile eindhoven_free radius 0.3m height 1.7m\n"
        );
        let scenario = parse(&source).expect("emitted declaration parses in a complete scenario");
        validate(&scenario).expect("emitted declaration validates in a complete scenario");
        assert_eq!(
            scenario.agents[0].speed_mps.to_bits(),
            profile.preferred_speed_mps.to_bits()
        );
    }

    #[test]
    fn tampering_a_free_walking_profile_breaks_its_self_hash() {
        let mut profile = profile_fixture();
        profile.preferred_speed_mps = 1.3;
        let error = validate_free_walking_speed_profile(&profile)
            .expect_err("the embedded profile hash must bind the speed");
        assert!(
            matches!(error, CalibrationError::InvalidSpeedProfile(message) if message.contains("does not match"))
        );
    }

    #[test]
    fn free_walking_artifact_hashes_survive_json_round_trip() {
        let profile: FreeWalkingSpeedProfile = serde_json::from_str(
            &serde_json::to_string(&profile_fixture()).expect("profile serializes"),
        )
        .expect("profile deserializes");
        let original_evaluation = evaluation_fixture(&profile);
        let evaluation: FreeWalkingSpeedHeldOutEvaluation = serde_json::from_str(
            &serde_json::to_string(&original_evaluation).expect("evaluation serializes"),
        )
        .expect("evaluation deserializes");

        validate_free_walking_speed_profile(&profile).expect("round-tripped profile verifies");
        assert_eq!(
            free_walking_speed_evaluation_hash(&original_evaluation)
                .expect("original evaluation hashes"),
            free_walking_speed_evaluation_hash(&evaluation)
                .expect("round-tripped evaluation hashes")
        );
        validate_free_walking_speed_evaluation(&evaluation)
            .expect("round-tripped evaluation verifies");
    }
}
