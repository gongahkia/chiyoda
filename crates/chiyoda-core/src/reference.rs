//! Descriptive adapters for content-locked, uncalibrated reference sources.
//!
//! These reports preserve data provenance and make source limitations visible.
//! They deliberately do not change runtime parameters or produce empirical
//! validation results.

use crate::{
    EvidenceCatalog, EvidencePurpose, calibration::verify_catalog_files, evidence::validate_catalog,
};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
};
use tar::Archive;
use thiserror::Error;
use zip::ZipArchive;

const VRU_DATASET_ID: &str = "vru-trajectory-2022";
const VRU_ARCHIVE_FILE_COUNT: u64 = 1;
const VRU_CSV_HEADER: &str = ",timestamp,x,y";
const VRU_PEDESTRIAN_MOTION_LABELS: &[&str] = &["moving", "starting", "stopping", "waiting"];
const DEFAULT_MAX_GAP_S: f64 = 0.1;
const DEFAULT_MAX_SPEED_MPS: f64 = 4.0;
const HISTOGRAM_BIN_WIDTH_MPS: f64 = 0.01;
const CROWD_QUEUE_DATASET_ID: &str = "wuppertal-crowdqueue-2018";
const CROWD_QUEUE_ARCHIVE_FILE_COUNT: u64 = 1;
const CROWD_QUEUE_FRAME_RATE_HZ: f64 = 25.0;
const CROWD_QUEUE_GATE_Y_M: f64 = 0.0;
const CROWD_QUEUE_MAX_GAP_S: f64 = 0.2;
const CROWD_QUEUE_MAX_SPEED_MPS: f64 = 4.0;
const CROWD_QUEUE_TRAJECTORY_HEADER: &str = "# id frame x/m y/m z/m";

#[derive(Debug, Error)]
pub enum ReferenceDataError {
    #[error("reference catalog is invalid: {0}")]
    InvalidCatalog(String),
    #[error("reference adapter requires dataset_id `{expected}`")]
    WrongDataset { expected: &'static str },
    #[error("reference adapter requires an uncalibrated_reference catalog")]
    WrongPurpose,
    #[error("reference adapter requires exactly {expected} content-locked archive file(s)")]
    UnexpectedFileCount { expected: u64 },
    #[error("reference source lock failed: {0}")]
    SourceLock(String),
    #[error("cannot read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: invalid archive: {message}")]
    Archive { path: PathBuf, message: String },
    #[error("{archive_path}: invalid pedestrian trajectory CSV at line {line}: {message}")]
    Csv {
        archive_path: String,
        line: u64,
        message: String,
    },
    #[error("archive contains no pedestrian trajectory CSV files")]
    NoPedestrianTrajectories,
    #[error("{archive_path}: invalid crowd-queue trajectory at line {line}: {message}")]
    CrowdQueueTrajectory {
        archive_path: String,
        line: u64,
        message: String,
    },
    #[error("archive contains no recognized crowd-queue trajectory text files")]
    NoCrowdQueueTrajectories,
    #[error("cannot serialize reference catalog for provenance: {0}")]
    CatalogSerialization(serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VruReferenceFilter {
    /// Consecutive rows further apart than this are recorded but excluded from
    /// speed estimates. The publisher documents 50 Hz camera trajectories.
    pub max_gap_s: f64,
    /// A transparent plausibility filter for head-position-derived speed.
    pub max_speed_mps: f64,
    pub histogram_bin_width_mps: f64,
}

impl Default for VruReferenceFilter {
    fn default() -> Self {
        Self {
            max_gap_s: DEFAULT_MAX_GAP_S,
            max_speed_mps: DEFAULT_MAX_SPEED_MPS,
            histogram_bin_width_mps: HISTOGRAM_BIN_WIDTH_MPS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VruObservationCounts {
    pub pedestrian_trajectories: u64,
    pub rows: u64,
    pub first_observations: u64,
    pub non_positive_time_steps: u64,
    pub gaps_over_limit: u64,
    pub speeds_over_limit: u64,
    pub usable_steps: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VruSpeedSummary {
    pub samples: u64,
    pub mean_mps: f64,
    pub standard_deviation_mps: f64,
    /// Estimated from a fixed-width histogram; the report records its width.
    pub p05_mps: f64,
    pub p50_mps: f64,
    pub p95_mps: f64,
    pub min_mps: f64,
    pub max_mps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VruReferenceReport {
    pub schema_version: String,
    pub adapter_version: String,
    pub catalog_sha256: String,
    pub dataset_id: String,
    pub source_context: String,
    pub filter: VruReferenceFilter,
    /// Trajectories are counted by the publisher's archive directory label.
    pub trajectories_by_motion_label: BTreeMap<String, u64>,
    pub observations: VruObservationCounts,
    pub speed: VruSpeedSummary,
    pub status: String,
    pub claim_boundary: String,
}

/// Fixed, source-declared treatment for the Wuppertal crowd-queue trajectories.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrowdQueueReferenceFilter {
    /// The archive declares 25 frames per second for every trajectory text file.
    pub frame_rate_hz: f64,
    /// The publisher locates the entry-gate beginning at y = 0 m.
    pub gate_crossing_y_m: f64,
    /// A wider-than-five-frame gap is counted but excluded from speed and gate
    /// crossing-time estimates.
    pub max_gap_s: f64,
    pub max_speed_mps: f64,
    pub histogram_bin_width_mps: f64,
}

impl Default for CrowdQueueReferenceFilter {
    fn default() -> Self {
        Self {
            frame_rate_hz: CROWD_QUEUE_FRAME_RATE_HZ,
            gate_crossing_y_m: CROWD_QUEUE_GATE_Y_M,
            max_gap_s: CROWD_QUEUE_MAX_GAP_S,
            max_speed_mps: CROWD_QUEUE_MAX_SPEED_MPS,
            histogram_bin_width_mps: HISTOGRAM_BIN_WIDTH_MPS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CrowdQueueObservationCounts {
    pub runs: u64,
    pub participants: u64,
    pub rows: u64,
    pub first_observations: u64,
    pub non_positive_frame_steps: u64,
    pub gaps_over_limit: u64,
    pub speeds_over_limit: u64,
    pub usable_steps: u64,
    /// First positive-to-nonpositive y crossings observed within the configured
    /// adjacent-frame gap limit.
    pub gate_crossings: u64,
    pub participants_without_usable_gate_crossing: u64,
}

/// One publisher-text-file run and its directly observed entry-gate crossings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrowdQueueRunSummary {
    pub archive_path: String,
    pub run_id: u64,
    pub corridor_width_m: f64,
    pub priming: String,
    pub motivation: String,
    pub participants: u64,
    pub gate_crossings: u64,
    pub first_gate_crossing_s: Option<f64>,
    pub last_gate_crossing_s: Option<f64>,
    /// Time between first and final observed gate crossings; it is not a system
    /// clearance time or a general queueing prediction.
    pub gate_crossing_window_s: Option<f64>,
    /// `(crossings - 1) / gate_crossing_window_s` when at least two usable
    /// crossings are observed.
    pub observed_gate_flow_per_s: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrowdQueueFlowSummary {
    pub measured_runs: u64,
    pub mean_persons_per_s: f64,
    pub standard_deviation_persons_per_s: f64,
    /// Exact order statistics across source runs, not an uncertainty interval.
    pub p05_persons_per_s: f64,
    pub p50_persons_per_s: f64,
    pub p95_persons_per_s: f64,
    pub min_persons_per_s: f64,
    pub max_persons_per_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrowdQueueReferenceReport {
    pub schema_version: String,
    pub adapter_version: String,
    pub catalog_sha256: String,
    pub dataset_id: String,
    pub source_context: String,
    pub filter: CrowdQueueReferenceFilter,
    pub observations: CrowdQueueObservationCounts,
    pub runs: Vec<CrowdQueueRunSummary>,
    pub observed_gate_flow: CrowdQueueFlowSummary,
    pub speed: VruSpeedSummary,
    pub status: String,
    pub claim_boundary: String,
}

/// Summarize the pedestrian CSV trajectories in the locked VRU source archive.
///
/// The source's urban-intersection domain remains out of scope for station
/// calibration. This function is only a reproducible descriptive reference.
pub fn summarize_vru_trajectory_reference(
    catalog: &EvidenceCatalog,
    data_root: &Path,
) -> Result<VruReferenceReport, ReferenceDataError> {
    validate_catalog(catalog).map_err(|errors| {
        ReferenceDataError::InvalidCatalog(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    if catalog.purpose != EvidencePurpose::UncalibratedReference {
        return Err(ReferenceDataError::WrongPurpose);
    }
    if catalog.dataset_id != VRU_DATASET_ID {
        return Err(ReferenceDataError::WrongDataset {
            expected: VRU_DATASET_ID,
        });
    }
    if u64::try_from(catalog.files.len()).expect("usize fits u64") != VRU_ARCHIVE_FILE_COUNT {
        return Err(ReferenceDataError::UnexpectedFileCount {
            expected: VRU_ARCHIVE_FILE_COUNT,
        });
    }
    verify_catalog_files(catalog, data_root)
        .map_err(|error| ReferenceDataError::SourceLock(error.to_string()))?;

    let filter = VruReferenceFilter::default();
    let source = &catalog.files[0];
    let source_path = data_root.join(&source.local_path);
    let file = File::open(&source_path).map_err(|source_error| ReferenceDataError::ReadFile {
        path: source_path.clone(),
        source: source_error,
    })?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| ReferenceDataError::Archive {
            path: source_path.clone(),
            message: error.to_string(),
        })?;
    let mut observations = VruObservationCounts::default();
    let mut speeds = RunningSpeedSummary::new(filter.max_speed_mps, filter.histogram_bin_width_mps);
    let mut trajectories_by_motion_label = BTreeMap::new();
    for entry in entries {
        let entry = entry.map_err(|error| ReferenceDataError::Archive {
            path: source_path.clone(),
            message: error.to_string(),
        })?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let archive_path = entry
            .path()
            .map_err(|error| ReferenceDataError::Archive {
                path: source_path.clone(),
                message: error.to_string(),
            })?
            .into_owned();
        let Some(motion_label) = vru_motion_label(&archive_path) else {
            continue;
        };
        let (counts, summary) = summarize_vru_csv(entry, &archive_path, &filter)?;
        observations.add(&counts);
        speeds.merge(&summary);
        *trajectories_by_motion_label
            .entry(motion_label)
            .or_insert(0) += 1;
    }
    if observations.pedestrian_trajectories == 0 {
        return Err(ReferenceDataError::NoPedestrianTrajectories);
    }

    Ok(VruReferenceReport {
        schema_version: "0.1".to_owned(),
        adapter_version: crate::RUNTIME_VERSION.to_owned(),
        catalog_sha256: catalog_hash(catalog)?,
        dataset_id: catalog.dataset_id.clone(),
        source_context: "Publisher-provided 2D pedestrian head-position trajectories from one urban intersection. CSV timestamp, x, and y values are interpreted in the publisher's declared seconds and metres units.".to_owned(),
        filter,
        trajectories_by_motion_label,
        observations,
        speed: speeds.finish(),
        status: "uncalibrated_reference_only".to_owned(),
        claim_boundary: "This report describes one source-locked urban-intersection dataset under its declared filter. It does not calibrate the reference runtime, validate a station or interchange, establish a pedestrian population profile, validate queues, route choice, information behavior, accessibility, evacuation, or safety, and cannot support empirical or operational claims.".to_owned(),
    })
}

/// Summarize the Wuppertal controlled-bottleneck source without turning it
/// into a station calibration or a general exit-capacity parameter.
pub fn summarize_crowd_queue_reference(
    catalog: &EvidenceCatalog,
    data_root: &Path,
) -> Result<CrowdQueueReferenceReport, ReferenceDataError> {
    validate_catalog(catalog).map_err(|errors| {
        ReferenceDataError::InvalidCatalog(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    if catalog.purpose != EvidencePurpose::UncalibratedReference {
        return Err(ReferenceDataError::WrongPurpose);
    }
    if catalog.dataset_id != CROWD_QUEUE_DATASET_ID {
        return Err(ReferenceDataError::WrongDataset {
            expected: CROWD_QUEUE_DATASET_ID,
        });
    }
    if u64::try_from(catalog.files.len()).expect("usize fits u64") != CROWD_QUEUE_ARCHIVE_FILE_COUNT
    {
        return Err(ReferenceDataError::UnexpectedFileCount {
            expected: CROWD_QUEUE_ARCHIVE_FILE_COUNT,
        });
    }
    verify_catalog_files(catalog, data_root)
        .map_err(|error| ReferenceDataError::SourceLock(error.to_string()))?;

    let filter = CrowdQueueReferenceFilter::default();
    let source = &catalog.files[0];
    let source_path = data_root.join(&source.local_path);
    let file = File::open(&source_path).map_err(|source_error| ReferenceDataError::ReadFile {
        path: source_path.clone(),
        source: source_error,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|error| ReferenceDataError::Archive {
        path: source_path.clone(),
        message: error.to_string(),
    })?;
    let mut observations = CrowdQueueObservationCounts::default();
    let mut speeds = RunningSpeedSummary::new(filter.max_speed_mps, filter.histogram_bin_width_mps);
    let mut flows = RunningFlowSummary::default();
    let mut runs = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| ReferenceDataError::Archive {
                path: source_path.clone(),
                message: error.to_string(),
            })?;
        if entry.is_dir() {
            continue;
        }
        let archive_path = entry.name().to_owned();
        let Some(descriptor) = crowd_queue_run_descriptor(&archive_path) else {
            continue;
        };
        let (run, counts, summary) =
            summarize_crowd_queue_txt(entry, &archive_path, descriptor, &filter)?;
        observations.add(&counts);
        speeds.merge(&summary);
        if let Some(flow) = run.observed_gate_flow_per_s {
            flows.add(flow);
        }
        runs.push(run);
    }
    if runs.is_empty() {
        return Err(ReferenceDataError::NoCrowdQueueTrajectories);
    }
    runs.sort_by(|left, right| left.archive_path.cmp(&right.archive_path));

    Ok(CrowdQueueReferenceReport {
        schema_version: "0.1".to_owned(),
        adapter_version: crate::RUNTIME_VERSION.to_owned(),
        catalog_sha256: catalog_hash(catalog)?,
        dataset_id: catalog.dataset_id.clone(),
        source_context: "Publisher-provided 25 Hz 2D trajectories from controlled Wuppertal entrance experiments. The source describes a fixed 0.5 m entry gate at y = 0 m; corridor width, priming, motivation, and participant count vary by published run.".to_owned(),
        filter,
        observations,
        runs,
        observed_gate_flow: flows.finish(),
        speed: speeds.finish(),
        status: "uncalibrated_reference_only".to_owned(),
        claim_boundary: "This report describes one controlled university bottleneck experiment under its declared crossing and speed filters. It does not calibrate the reference runtime, validate station exits, queues, route choice, information behavior, accessibility, evacuation, safety, a general exit-capacity law, or any population profile, and cannot support empirical or operational claims.".to_owned(),
    })
}

#[derive(Debug, Clone)]
struct CrowdQueueRunDescriptor {
    run_id: u64,
    corridor_width_m: f64,
    priming: String,
    motivation: String,
}

#[derive(Debug, Clone, Copy)]
struct CrowdQueueSample {
    frame: u64,
    x_m: f64,
    y_m: f64,
}

fn crowd_queue_run_descriptor(archive_path: &str) -> Option<CrowdQueueRunDescriptor> {
    let path = Path::new(archive_path);
    if path
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty())
        || !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
    {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let segments = stem.split('_').collect::<Vec<_>>();
    let [run_id, priming, width_decimetres, motivation] = segments.as_slice() else {
        return None;
    };
    if !matches!(*priming, "c" | "q") || !matches!(*motivation, "h0" | "h-" | "h+") {
        return None;
    }
    let run_id = run_id.parse::<u64>().ok()?;
    let width_decimetres = width_decimetres.parse::<u64>().ok()?;
    if width_decimetres == 0 {
        return None;
    }
    Some(CrowdQueueRunDescriptor {
        run_id,
        #[allow(clippy::cast_precision_loss)]
        corridor_width_m: width_decimetres as f64 / 10.0,
        priming: (*priming).to_owned(),
        motivation: (*motivation).to_owned(),
    })
}

#[allow(clippy::too_many_lines)] // one source-file pass keeps the published crossing rule auditable
fn summarize_crowd_queue_txt<R: Read>(
    entry: R,
    archive_path: &str,
    descriptor: CrowdQueueRunDescriptor,
    filter: &CrowdQueueReferenceFilter,
) -> Result<
    (
        CrowdQueueRunSummary,
        CrowdQueueObservationCounts,
        RunningSpeedSummary,
    ),
    ReferenceDataError,
> {
    let mut reader = BufReader::new(entry);
    let mut samples_by_participant = BTreeMap::<u64, Vec<CrowdQueueSample>>::new();
    let mut line = String::new();
    let mut line_number = 0_u64;
    let mut header_seen = false;
    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|source| crowd_queue_io_error(archive_path, line_number + 1, &source))?;
        if bytes == 0 {
            break;
        }
        line_number += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            header_seen |= trimmed == CROWD_QUEUE_TRAJECTORY_HEADER;
            continue;
        }
        if !header_seen {
            return Err(ReferenceDataError::CrowdQueueTrajectory {
                archive_path: archive_path.to_owned(),
                line: line_number,
                message: format!(
                    "expected header `{CROWD_QUEUE_TRAJECTORY_HEADER}` before samples"
                ),
            });
        }
        let (participant_id, sample) =
            parse_crowd_queue_sample(trimmed, archive_path, line_number)?;
        samples_by_participant
            .entry(participant_id)
            .or_default()
            .push(sample);
    }
    if !header_seen {
        return Err(ReferenceDataError::CrowdQueueTrajectory {
            archive_path: archive_path.to_owned(),
            line: 1,
            message: format!("expected header `{CROWD_QUEUE_TRAJECTORY_HEADER}`"),
        });
    }
    if samples_by_participant.is_empty() {
        return Err(ReferenceDataError::CrowdQueueTrajectory {
            archive_path: archive_path.to_owned(),
            line: line_number,
            message: "must contain at least one trajectory sample".to_owned(),
        });
    }

    let mut counts = CrowdQueueObservationCounts {
        runs: 1,
        participants: u64::try_from(samples_by_participant.len()).expect("usize fits u64"),
        ..CrowdQueueObservationCounts::default()
    };
    let mut speeds = RunningSpeedSummary::new(filter.max_speed_mps, filter.histogram_bin_width_mps);
    let mut crossing_times = Vec::new();
    for samples in samples_by_participant.values_mut() {
        samples.sort_by_key(|sample| sample.frame);
        counts.rows += u64::try_from(samples.len()).expect("usize fits u64");
        let Some(mut previous) = samples.first().copied() else {
            continue;
        };
        counts.first_observations += 1;
        let mut crossing_time = None;
        for sample in samples.iter().copied().skip(1) {
            if sample.frame <= previous.frame {
                counts.non_positive_frame_steps += 1;
                previous = sample;
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let elapsed_s = (sample.frame - previous.frame) as f64 / filter.frame_rate_hz;
            if elapsed_s > filter.max_gap_s {
                counts.gaps_over_limit += 1;
                previous = sample;
                continue;
            }
            if crossing_time.is_none()
                && previous.y_m > filter.gate_crossing_y_m
                && sample.y_m <= filter.gate_crossing_y_m
            {
                let fraction =
                    (previous.y_m - filter.gate_crossing_y_m) / (previous.y_m - sample.y_m);
                #[allow(clippy::cast_precision_loss)]
                let start_time_s = previous.frame as f64 / filter.frame_rate_hz;
                let time_s = start_time_s + elapsed_s * fraction;
                if time_s.is_finite() {
                    crossing_time = Some(time_s);
                }
            }
            let speed_mps =
                (sample.x_m - previous.x_m).hypot(sample.y_m - previous.y_m) / elapsed_s;
            if !speed_mps.is_finite() || speed_mps > filter.max_speed_mps {
                counts.speeds_over_limit += 1;
            } else {
                counts.usable_steps += 1;
                speeds.add(speed_mps);
            }
            previous = sample;
        }
        if let Some(time_s) = crossing_time {
            crossing_times.push(time_s);
        }
    }
    crossing_times.sort_by(f64::total_cmp);
    counts.gate_crossings = u64::try_from(crossing_times.len()).expect("usize fits u64");
    counts.participants_without_usable_gate_crossing =
        counts.participants.saturating_sub(counts.gate_crossings);
    let first_gate_crossing_s = crossing_times.first().copied();
    let last_gate_crossing_s = crossing_times.last().copied();
    let gate_crossing_window_s =
        first_gate_crossing_s
            .zip(last_gate_crossing_s)
            .and_then(|(first, last)| {
                let window_s = last - first;
                (window_s > 0.0).then_some(window_s)
            });
    let observed_gate_flow_per_s = gate_crossing_window_s.and_then(|window_s| {
        (counts.gate_crossings > 1).then(|| {
            #[allow(clippy::cast_precision_loss)]
            let intervals = (counts.gate_crossings - 1) as f64;
            intervals / window_s
        })
    });
    Ok((
        CrowdQueueRunSummary {
            archive_path: archive_path.to_owned(),
            run_id: descriptor.run_id,
            corridor_width_m: descriptor.corridor_width_m,
            priming: descriptor.priming,
            motivation: descriptor.motivation,
            participants: counts.participants,
            gate_crossings: counts.gate_crossings,
            first_gate_crossing_s,
            last_gate_crossing_s,
            gate_crossing_window_s,
            observed_gate_flow_per_s,
        },
        counts,
        speeds,
    ))
}

fn crowd_queue_io_error(
    archive_path: &str,
    line: u64,
    source: &std::io::Error,
) -> ReferenceDataError {
    ReferenceDataError::CrowdQueueTrajectory {
        archive_path: archive_path.to_owned(),
        line,
        message: source.to_string(),
    }
}

fn parse_crowd_queue_sample(
    row: &str,
    archive_path: &str,
    line: u64,
) -> Result<(u64, CrowdQueueSample), ReferenceDataError> {
    let fields = row.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err(ReferenceDataError::CrowdQueueTrajectory {
            archive_path: archive_path.to_owned(),
            line,
            message: "expected five whitespace-separated fields".to_owned(),
        });
    }
    let parse_u64 = |value: &str, field: &str| {
        value
            .parse::<u64>()
            .map_err(|_| ReferenceDataError::CrowdQueueTrajectory {
                archive_path: archive_path.to_owned(),
                line,
                message: format!("{field} must be an unsigned integer"),
            })
    };
    let parse_finite = |value: &str, field: &str| {
        value
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
            .ok_or_else(|| ReferenceDataError::CrowdQueueTrajectory {
                archive_path: archive_path.to_owned(),
                line,
                message: format!("{field} must be a finite number"),
            })
    };
    let participant_id = parse_u64(fields[0], "id")?;
    let frame = parse_u64(fields[1], "frame")?;
    let x_m = parse_finite(fields[2], "x/m")?;
    let y_m = parse_finite(fields[3], "y/m")?;
    let _z_m = parse_finite(fields[4], "z/m")?;
    Ok((participant_id, CrowdQueueSample { frame, x_m, y_m }))
}

impl CrowdQueueObservationCounts {
    fn add(&mut self, other: &Self) {
        self.runs += other.runs;
        self.participants += other.participants;
        self.rows += other.rows;
        self.first_observations += other.first_observations;
        self.non_positive_frame_steps += other.non_positive_frame_steps;
        self.gaps_over_limit += other.gaps_over_limit;
        self.speeds_over_limit += other.speeds_over_limit;
        self.usable_steps += other.usable_steps;
        self.gate_crossings += other.gate_crossings;
        self.participants_without_usable_gate_crossing +=
            other.participants_without_usable_gate_crossing;
    }
}

#[derive(Debug, Default)]
struct RunningFlowSummary {
    values: Vec<f64>,
}

impl RunningFlowSummary {
    fn add(&mut self, flow_per_s: f64) {
        self.values.push(flow_per_s);
    }

    fn finish(mut self) -> CrowdQueueFlowSummary {
        if self.values.is_empty() {
            return CrowdQueueFlowSummary {
                measured_runs: 0,
                mean_persons_per_s: 0.0,
                standard_deviation_persons_per_s: 0.0,
                p05_persons_per_s: 0.0,
                p50_persons_per_s: 0.0,
                p95_persons_per_s: 0.0,
                min_persons_per_s: 0.0,
                max_persons_per_s: 0.0,
            };
        }
        self.values.sort_by(f64::total_cmp);
        #[allow(clippy::cast_precision_loss)]
        let count = self.values.len() as f64;
        let mean = self.values.iter().sum::<f64>() / count;
        let standard_deviation = (self
            .values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / count)
            .sqrt();
        CrowdQueueFlowSummary {
            measured_runs: u64::try_from(self.values.len()).expect("usize fits u64"),
            mean_persons_per_s: mean,
            standard_deviation_persons_per_s: standard_deviation,
            p05_persons_per_s: order_statistic(&self.values, 1, 20),
            p50_persons_per_s: order_statistic(&self.values, 1, 2),
            p95_persons_per_s: order_statistic(&self.values, 19, 20),
            min_persons_per_s: self.values[0],
            max_persons_per_s: *self.values.last().expect("values is non-empty"),
        }
    }
}

fn order_statistic(values: &[f64], numerator: u64, denominator: u64) -> f64 {
    let index = (values.len() - 1) * usize::try_from(numerator).expect("u64 fits usize")
        / usize::try_from(denominator).expect("u64 fits usize");
    values[index]
}

fn catalog_hash(catalog: &EvidenceCatalog) -> Result<String, ReferenceDataError> {
    let canonical =
        serde_json::to_vec(catalog).map_err(ReferenceDataError::CatalogSerialization)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

fn vru_motion_label(path: &Path) -> Option<String> {
    let components = path
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?;
    let [root, actor, motion_label, file_name] = components.as_slice() else {
        return None;
    };
    (*root == "VRU_dataset"
        && *actor == "pedestrians"
        && VRU_PEDESTRIAN_MOTION_LABELS.contains(motion_label)
        && Path::new(file_name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("csv")))
    .then(|| (*motion_label).to_owned())
}

fn summarize_vru_csv<R: std::io::Read>(
    entry: R,
    archive_path: &Path,
    filter: &VruReferenceFilter,
) -> Result<(VruObservationCounts, RunningSpeedSummary), ReferenceDataError> {
    let archive_path = archive_path.display().to_string();
    let mut reader = BufReader::new(entry);
    let mut line = String::new();
    let header_read = reader
        .read_line(&mut line)
        .map_err(|source| csv_io_error(&archive_path, 1, &source))?;
    if header_read == 0 || line.trim() != VRU_CSV_HEADER {
        return Err(ReferenceDataError::Csv {
            archive_path,
            line: 1,
            message: format!("expected header `{VRU_CSV_HEADER}`"),
        });
    }

    let mut counts = VruObservationCounts {
        pedestrian_trajectories: 1,
        ..VruObservationCounts::default()
    };
    let mut speeds = RunningSpeedSummary::new(filter.max_speed_mps, filter.histogram_bin_width_mps);
    let mut previous = None;
    let mut line_number = 1_u64;
    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|source| csv_io_error(&archive_path, line_number + 1, &source))?;
        if bytes == 0 {
            break;
        }
        line_number += 1;
        let sample = parse_vru_sample(&line, &archive_path, line_number)?;
        counts.rows += 1;
        let Some(previous) = previous.replace(sample) else {
            counts.first_observations += 1;
            continue;
        };
        let elapsed_s = sample.timestamp_s - previous.timestamp_s;
        if elapsed_s <= 0.0 {
            counts.non_positive_time_steps += 1;
            continue;
        }
        if elapsed_s > filter.max_gap_s {
            counts.gaps_over_limit += 1;
            continue;
        }
        let speed_mps = (sample.x_m - previous.x_m).hypot(sample.y_m - previous.y_m) / elapsed_s;
        if !speed_mps.is_finite() || speed_mps > filter.max_speed_mps {
            counts.speeds_over_limit += 1;
            continue;
        }
        counts.usable_steps += 1;
        speeds.add(speed_mps);
    }
    Ok((counts, speeds))
}

fn csv_io_error(archive_path: &str, line: u64, source: &std::io::Error) -> ReferenceDataError {
    ReferenceDataError::Csv {
        archive_path: archive_path.to_owned(),
        line,
        message: source.to_string(),
    }
}

#[derive(Debug, Clone, Copy)]
struct VruSample {
    timestamp_s: f64,
    x_m: f64,
    y_m: f64,
}

fn parse_vru_sample(
    row: &str,
    archive_path: &str,
    line: u64,
) -> Result<VruSample, ReferenceDataError> {
    let fields = row.trim().split(',').collect::<Vec<_>>();
    if fields.len() != 4 {
        return Err(ReferenceDataError::Csv {
            archive_path: archive_path.to_owned(),
            line,
            message: "expected four CSV fields".to_owned(),
        });
    }
    let parse = |value: &str, field: &str| {
        value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
            .ok_or_else(|| ReferenceDataError::Csv {
                archive_path: archive_path.to_owned(),
                line,
                message: format!("{field} must be a finite number"),
            })
    };
    Ok(VruSample {
        timestamp_s: parse(fields[1], "timestamp")?,
        x_m: parse(fields[2], "x")?,
        y_m: parse(fields[3], "y")?,
    })
}

impl VruObservationCounts {
    fn add(&mut self, other: &Self) {
        self.pedestrian_trajectories += other.pedestrian_trajectories;
        self.rows += other.rows;
        self.first_observations += other.first_observations;
        self.non_positive_time_steps += other.non_positive_time_steps;
        self.gaps_over_limit += other.gaps_over_limit;
        self.speeds_over_limit += other.speeds_over_limit;
        self.usable_steps += other.usable_steps;
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
    fn new(max_speed_mps: f64, bin_width_mps: f64) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bin_count = (max_speed_mps / bin_width_mps).ceil() as usize;
        Self {
            samples: 0,
            mean_mps: 0.0,
            sum_squared_differences: 0.0,
            min_mps: f64::INFINITY,
            max_mps: f64::NEG_INFINITY,
            sample_weight: 0.0,
            bin_width_mps,
            bins: vec![0; bin_count],
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

    fn finish(&self) -> VruSpeedSummary {
        if self.samples == 0 {
            return VruSpeedSummary {
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
        VruSpeedSummary {
            samples: self.samples,
            mean_mps: self.mean_mps,
            standard_deviation_mps: (self.sum_squared_differences / self.sample_weight).sqrt(),
            p05_mps: self.quantile(1, 20),
            p50_mps: self.quantile(1, 2),
            p95_mps: self.quantile(19, 20),
            min_mps: self.min_mps,
            max_mps: self.max_mps,
        }
    }

    fn quantile(&self, numerator: u64, denominator: u64) -> f64 {
        let target = (self.samples - 1).saturating_mul(numerator) / denominator;
        let mut cumulative = 0_u64;
        for (index, count) in self.bins.iter().enumerate() {
            cumulative += count;
            if cumulative > target {
                let index = u32::try_from(index).expect("histogram index fits u32");
                return (f64::from(index) + 0.5) * self.bin_width_mps;
            }
        }
        self.max_mps
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CrowdQueueReferenceFilter, RunningSpeedSummary, VruReferenceFilter,
        crowd_queue_run_descriptor, parse_vru_sample, summarize_crowd_queue_txt, summarize_vru_csv,
        vru_motion_label,
    };
    use std::{io::Cursor, path::Path};

    #[test]
    fn recognizes_only_pedestrian_csv_trajectories() {
        assert_eq!(
            vru_motion_label(Path::new("VRU_dataset/pedestrians/moving/1.csv")),
            Some("moving".to_owned())
        );
        assert_eq!(
            vru_motion_label(Path::new("VRU_dataset/cyclists/moving/1.csv")),
            None
        );
        assert_eq!(
            vru_motion_label(Path::new("VRU_dataset/pedestrians/.svn/entries.csv")),
            None
        );
    }

    #[test]
    fn source_speed_summary_preserves_the_weighted_mean() {
        let filter = VruReferenceFilter::default();
        let mut first =
            RunningSpeedSummary::new(filter.max_speed_mps, filter.histogram_bin_width_mps);
        first.add(1.0);
        let mut second =
            RunningSpeedSummary::new(filter.max_speed_mps, filter.histogram_bin_width_mps);
        second.add(3.0);
        first.merge(&second);
        assert!((first.finish().mean_mps - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rejects_non_finite_trajectory_values() {
        assert!(parse_vru_sample("0,NaN,1,1", "fixture.csv", 2).is_err());
    }

    #[test]
    fn source_csv_summary_counts_filter_rejections_without_hiding_them() {
        let source =
            b",timestamp,x,y\n0,0.0,0.0,0.0\n1,0.02,0.02,0.0\n2,0.22,0.04,0.0\n3,0.24,1.04,0.0\n";
        let (counts, speed) = summarize_vru_csv(
            Cursor::new(source),
            Path::new("VRU_dataset/pedestrians/moving/fixture.csv"),
            &VruReferenceFilter::default(),
        )
        .expect("fixture CSV is valid");

        assert_eq!(counts.pedestrian_trajectories, 1);
        assert_eq!(counts.rows, 4);
        assert_eq!(counts.usable_steps, 1);
        assert_eq!(counts.gaps_over_limit, 1);
        assert_eq!(counts.speeds_over_limit, 1);
        assert_eq!(speed.finish().samples, 1);
    }

    #[test]
    fn crowd_queue_descriptor_and_summary_preserve_source_run_boundaries() {
        let descriptor = crowd_queue_run_descriptor("010_c_12_h0.txt")
            .expect("known publisher naming convention is recognized");
        assert_eq!(descriptor.run_id, 10);
        assert!((descriptor.corridor_width_m - 1.2).abs() < f64::EPSILON);
        let source = b"# PeTrack project: fixture.pet\n# framerate: 25 fps\n# id frame x/m y/m z/m\n1 0 0.0 0.04 1.76\n1 1 0.0 -0.04 1.76\n2 0 0.0 0.08 1.76\n2 2 0.0 -0.04 1.76\n3 0 0.0 0.04 1.76\n3 6 0.0 -0.04 1.76\n";

        let (run, counts, speed) = summarize_crowd_queue_txt(
            Cursor::new(source),
            "010_c_12_h0.txt",
            descriptor,
            &CrowdQueueReferenceFilter::default(),
        )
        .expect("fixture trajectory text is valid");

        assert_eq!(counts.runs, 1);
        assert_eq!(counts.participants, 3);
        assert_eq!(counts.rows, 6);
        assert_eq!(counts.gate_crossings, 2);
        assert_eq!(counts.participants_without_usable_gate_crossing, 1);
        assert_eq!(counts.gaps_over_limit, 1);
        assert_eq!(counts.usable_steps, 2);
        assert_eq!(run.gate_crossings, 2);
        assert!(run.observed_gate_flow_per_s.is_some());
        assert_eq!(speed.finish().samples, 2);
    }

    #[test]
    fn crowd_queue_summary_rejects_non_finite_coordinates() {
        let descriptor = crowd_queue_run_descriptor("010_c_12_h0.txt")
            .expect("known publisher naming convention is recognized");
        let source = b"# id frame x/m y/m z/m\n1 0 NaN 0.04 1.76\n";

        assert!(
            summarize_crowd_queue_txt(
                Cursor::new(source),
                "010_c_12_h0.txt",
                descriptor,
                &CrowdQueueReferenceFilter::default(),
            )
            .is_err()
        );
    }
}
