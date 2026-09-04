//! Descriptive, source-locked calibration intake for the Eindhoven platform data.
//!
//! This module does not alter simulation parameters. It produces a transparent
//! measurement report which a separately reviewed calibration protocol may use.
//! Keeping that distinction in code prevents a descriptive fit from silently
//! becoming a predictive or operational claim.

use crate::{EvidenceCatalog, EvidencePurpose, benchmark::DatasetRole, evidence::validate_catalog};
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

const TIME_COLUMN: &str = "time_ms";
const ID_COLUMN: &str = "object_identifier";
const X_COLUMN: &str = "x_position_mm";
const Y_COLUMN: &str = "y_position_mm";
const HISTOGRAM_BIN_WIDTH_MPS: f64 = 0.01;
const HISTOGRAM_BIN_COUNT: usize = 400;
const DEFAULT_MAX_GAP_MS: i64 = 500;
const DEFAULT_MAX_SPEED_MPS: f64 = 4.0;

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
    #[error("{path}: invalid parquet input: {message}")]
    Parquet { path: PathBuf, message: String },
    #[error("{path}: expected numeric column `{column}`")]
    UnsupportedColumnType { path: PathBuf, column: &'static str },
    #[error("{path}: missing required column `{column}`")]
    MissingColumn { path: PathBuf, column: &'static str },
    #[error("cannot serialize evidence catalog for provenance: {0}")]
    CatalogSerialization(serde_json::Error),
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

/// Generate a reproducible descriptive report for the known Eindhoven platform
/// schema. The caller must provide the exact root containing catalog-relative
/// files. Each source is size- and SHA-256-locked before it is read.
pub fn calibrate_eindhoven_platform(
    catalog: &EvidenceCatalog,
    data_root: &Path,
    partition: DatasetRole,
) -> Result<PlatformCalibrationReport, CalibrationError> {
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

#[cfg(test)]
mod tests {
    use super::{
        CalibrationError, FileAccumulator, Observation, ObservationFilter,
        PlatformCalibrationReport, RunningSpeedSummary, calibrate_eindhoven_platform, catalog_hash,
    };
    use crate::{EvidenceCatalog, EvidencePurpose, benchmark::DatasetRole, evidence::EvidenceFile};
    use std::path::Path;

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
}
