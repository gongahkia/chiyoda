//! Provenance-preserving inspection of open layout sources.
//!
//! OpenStreetMap is useful for discovering publicly mapped station features,
//! but its geographic coordinates and incomplete tags cannot be converted into
//! a Chiyoda scenario without author review. This module intentionally emits a
//! source observation report, and can derive an explicitly anchored local
//! east/north reference artifact. Neither artifact creates elevation, capacity,
//! geometry, or DSL source.

use crate::{
    EvidenceCatalog, EvidencePurpose, calibration::verify_catalog_files, evidence::validate_catalog,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};
use thiserror::Error;
use xml::{
    attribute::OwnedAttribute,
    reader::{ParserConfig, XmlEvent},
};

const OSM_LICENSE: &str = "ODbL-1.0";
const REQUIRED_OSM_ATTRIBUTION: &str = "OpenStreetMap contributors";
const OSM_OBSERVATION_SCHEMA_VERSION: &str = "0.1";
const OSM_OBSERVATION_STATUS: &str = "source_observation_only";
const OSM_GEOGRAPHIC_COORDINATE_REFERENCE: &str = "WGS84 geographic latitude/longitude copied from the OSM XML; this adapter does not project coordinates into scenario metres or infer elevations.";
const WGS84_SEMI_MAJOR_AXIS_M: f64 = 6_378_137.0;
const WGS84_INVERSE_FLATTENING: f64 = 298.257_223_563;
const LOCAL_PROJECTION_METHOD: &str =
    "WGS84 geodetic to ECEF followed by ENU topocentric conversion (EPSG:9837)";
const LOCAL_PROJECTION_REFERENCE: &str =
    "https://proj.org/en/stable/operations/conversions/topocentric.html";
const LOCAL_PROJECTION_PRECISION_M: f64 = 1_000_000.0;
const RELEVANT_TAG_KEYS: &[&str] = &[
    "building",
    "building:part",
    "conveying",
    "entrance",
    "highway",
    "indoor",
    "level",
    "name",
    "public_transport",
    "railway",
    "ref",
    "wheelchair",
];

/// Resource bounds for a local OSM XML inspection. A station-sized extract is
/// the intended input; callers must deliberately raise a bound for a larger
/// extract rather than accidentally retaining a regional node table in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OsmInspectionLimits {
    pub max_nodes: usize,
    pub max_ways: usize,
    pub max_node_references_per_way: usize,
    pub max_tags_per_object: usize,
}

impl Default for OsmInspectionLimits {
    fn default() -> Self {
        Self {
            max_nodes: 250_000,
            max_ways: 50_000,
            max_node_references_per_way: 10_000,
            max_tags_per_object: 128,
        }
    }
}

#[derive(Debug, Error)]
pub enum OsmLayoutError {
    #[error("layout catalog is invalid: {0}")]
    InvalidCatalog(String),
    #[error("layout inspection requires an uncalibrated_reference catalog")]
    WrongPurpose,
    #[error("layout inspection requires an `ODbL-1.0` catalog")]
    WrongLicense,
    #[error("layout inspection requires OpenStreetMap contributor attribution in the catalog")]
    MissingOpenStreetMapAttribution,
    #[error("layout inspection requires exactly one content-locked OSM XML file")]
    UnexpectedFileCount,
    #[error("layout inspection accepts a `.osm` or `.xml` source file, found `{path}`")]
    UnsupportedSourceFormat { path: String },
    #[error("layout source lock failed: {0}")]
    SourceLock(String),
    #[error("cannot read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: invalid OSM XML: {message}")]
    Xml { path: PathBuf, message: String },
    #[error("{path}: OSM XML root element must be `osm`")]
    WrongRoot { path: PathBuf },
    #[error("OSM inspection bound `{name}` must be non-zero")]
    InvalidLimit { name: &'static str },
    #[error("OSM inspection exceeded its `{name}` bound of {limit}")]
    LimitExceeded { name: &'static str, limit: usize },
    #[error("{element} has an invalid `{attribute}` attribute: `{value}`")]
    InvalidAttribute {
        element: String,
        attribute: &'static str,
        value: String,
    },
    #[error("OSM XML is missing required `{attribute}` on {element}")]
    MissingAttribute {
        element: String,
        attribute: &'static str,
    },
    #[error("duplicate OSM {object_type} identifier `{id}`")]
    DuplicateIdentifier { object_type: &'static str, id: i64 },
    #[error("duplicate `{key}` tag on OSM {object_type} `{id}`")]
    DuplicateTag {
        object_type: &'static str,
        id: i64,
        key: String,
    },
    #[error("selected OSM way `{way_id}` references unavailable node `{node_id}`")]
    MissingNodeReference { way_id: i64, node_id: i64 },
    #[error("cannot serialize layout catalog for provenance: {0}")]
    CatalogSerialization(serde_json::Error),
    #[error("layout report `{field}` value cannot be represented on this platform")]
    ReportLimitOutOfRange { field: &'static str },
    #[error(
        "layout observation report does not match reconstruction from its catalog and locked source"
    )]
    ReportMismatch,
    #[error("local projection origin has an invalid `{field}` value: `{value}`")]
    InvalidProjectionOrigin { field: &'static str, value: f64 },
    #[error("layout observation has an invalid `{field}` value: `{value}`")]
    InvalidObservedCoordinate { field: &'static str, value: f64 },
    #[error("local projection produced a non-finite `{axis}` coordinate")]
    NonFiniteProjection { axis: &'static str },
    #[error(
        "local projection requires an OSM source-observation report with `{field}` equal to `{expected}`, found `{actual}`"
    )]
    InvalidProjectionSourceReport {
        field: &'static str,
        expected: &'static str,
        actual: String,
    },
    #[error(
        "OSM geographic bounds span {span_degrees} degrees of longitude and are ambiguous at the antimeridian"
    )]
    AmbiguousBoundsLongitudeSpan { span_degrees: f64 },
    #[error("cannot serialize layout observation report for projection provenance: {0}")]
    ProjectionSerialization(serde_json::Error),
    #[error(
        "local coordinate projection does not match reconstruction from its source-observation report"
    )]
    ProjectionMismatch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenStreetMapLayoutSource {
    pub catalog_sha256: String,
    pub dataset_id: String,
    pub source_url: String,
    pub source_sha256: String,
    pub size_bytes: u64,
    pub license: String,
    pub required_attribution: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeographicPoint {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeographicBounds {
    pub minimum_latitude: f64,
    pub minimum_longitude: f64,
    pub maximum_latitude: f64,
    pub maximum_longitude: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OsmFeatureGeometry {
    Point {
        coordinate: GeographicPoint,
    },
    Bounds {
        bounds: GeographicBounds,
        referenced_node_count: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsmFeatureObservation {
    pub object_type: String,
    pub object_id: i64,
    pub categories: Vec<String>,
    pub relevant_tags: BTreeMap<String, String>,
    pub geometry: OsmFeatureGeometry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OsmLayoutCounts {
    pub parsed_nodes: u64,
    pub parsed_ways: u64,
    pub selected_node_features: u64,
    pub selected_way_features: u64,
    pub selected_features_by_category: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenStreetMapLayoutReport {
    pub schema_version: String,
    pub adapter_version: String,
    pub source: OpenStreetMapLayoutSource,
    pub coordinate_reference: String,
    pub inspection_limits: OsmInspectionLimitsReport,
    pub counts: OsmLayoutCounts,
    pub features: Vec<OsmFeatureObservation>,
    pub status: String,
    pub claim_boundary: String,
    pub required_authoring: Vec<String>,
}

/// A deliberately chosen local topocentric frame. `east_m` and `north_m` are
/// derived from WGS84 coordinates at ellipsoidal height zero; neither is a
/// surveyed facility coordinate or a physical elevation measurement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalTangentPlane {
    pub method: String,
    pub origin: GeographicPoint,
    pub origin_ellipsoidal_height_m: f64,
    pub output_axes: String,
    pub output_precision_m: f64,
    pub reference: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LocalMetrePoint {
    pub east_m: f64,
    pub north_m: f64,
}

/// The four local points made by projecting a geographic OSM bounding box's
/// corners. It is intentionally not a projected bounding envelope, path,
/// polygon, or walkable boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalMetreBoundsCornerReference {
    pub source_bounds: GeographicBounds,
    pub southwest: LocalMetrePoint,
    pub southeast: LocalMetrePoint,
    pub northwest: LocalMetrePoint,
    pub northeast: LocalMetrePoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectedOsmFeatureGeometry {
    Point {
        coordinate: LocalMetrePoint,
    },
    BoundsCorners {
        corners: LocalMetreBoundsCornerReference,
        referenced_node_count: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectedOsmFeatureObservation {
    pub object_type: String,
    pub object_id: i64,
    pub categories: Vec<String>,
    pub relevant_tags: BTreeMap<String, String>,
    pub geometry: ProjectedOsmFeatureGeometry,
}

/// A reproducible, uncalibrated local-coordinate reference derived from one
/// verified OSM source-observation report. It is deliberately not Chiyoda DSL
/// geometry and contains no vertical coordinate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenStreetMapLocalProjectionReport {
    pub schema_version: String,
    pub adapter_version: String,
    pub source_observation_report_sha256: String,
    pub source: OpenStreetMapLayoutSource,
    pub coordinate_reference: LocalTangentPlane,
    pub counts: OsmLayoutCounts,
    pub features: Vec<ProjectedOsmFeatureObservation>,
    pub status: String,
    pub claim_boundary: String,
    pub required_authoring: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsmInspectionLimitsReport {
    pub max_nodes: u64,
    pub max_ways: u64,
    pub max_node_references_per_way: u64,
    pub max_tags_per_object: u64,
}

impl From<OsmInspectionLimits> for OsmInspectionLimitsReport {
    fn from(limits: OsmInspectionLimits) -> Self {
        Self {
            max_nodes: u64::try_from(limits.max_nodes).expect("usize fits u64"),
            max_ways: u64::try_from(limits.max_ways).expect("usize fits u64"),
            max_node_references_per_way: u64::try_from(limits.max_node_references_per_way)
                .expect("usize fits u64"),
            max_tags_per_object: u64::try_from(limits.max_tags_per_object).expect("usize fits u64"),
        }
    }
}

#[derive(Debug)]
struct NodeBuilder {
    id: i64,
    coordinate: GeographicPoint,
    tags: BTreeMap<String, String>,
    tag_count: usize,
}

#[derive(Debug)]
struct WayBuilder {
    id: i64,
    node_references: Vec<i64>,
    tags: BTreeMap<String, String>,
    tag_count: usize,
}

/// Inspect one content-locked OSM XML extract and write only map observations.
/// The report is explicitly not an import into Chiyoda's metre-based DSL.
pub fn inspect_openstreetmap_layout(
    catalog: &EvidenceCatalog,
    data_root: &Path,
    limits: OsmInspectionLimits,
) -> Result<OpenStreetMapLayoutReport, OsmLayoutError> {
    validate_limits(limits)?;
    validate_catalog(catalog).map_err(|errors| {
        OsmLayoutError::InvalidCatalog(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    if catalog.purpose != EvidencePurpose::UncalibratedReference {
        return Err(OsmLayoutError::WrongPurpose);
    }
    if catalog.license != OSM_LICENSE {
        return Err(OsmLayoutError::WrongLicense);
    }
    let attribution = catalog.attribution.as_deref().unwrap_or_default();
    if !attribution.contains(REQUIRED_OSM_ATTRIBUTION) {
        return Err(OsmLayoutError::MissingOpenStreetMapAttribution);
    }
    if catalog.files.len() != 1 {
        return Err(OsmLayoutError::UnexpectedFileCount);
    }
    let source = &catalog.files[0];
    if !is_osm_xml_path(&source.local_path) {
        return Err(OsmLayoutError::UnsupportedSourceFormat {
            path: source.local_path.clone(),
        });
    }
    verify_catalog_files(catalog, data_root)
        .map_err(|error| OsmLayoutError::SourceLock(error.to_string()))?;

    let source_path = data_root.join(&source.local_path);
    let catalog_sha256 =
        sha256_bytes(&serde_json::to_vec(catalog).map_err(OsmLayoutError::CatalogSerialization)?);
    let (counts, features) = inspect_osm_xml(&source_path, limits)?;

    Ok(OpenStreetMapLayoutReport {
        schema_version: OSM_OBSERVATION_SCHEMA_VERSION.to_owned(),
        adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
        source: OpenStreetMapLayoutSource {
            catalog_sha256,
            dataset_id: catalog.dataset_id.clone(),
            source_url: source.source_url.clone(),
            source_sha256: source.sha256.clone(),
            size_bytes: source.size_bytes,
            license: catalog.license.clone(),
            required_attribution: attribution.to_owned(),
        },
        coordinate_reference: OSM_GEOGRAPHIC_COORDINATE_REFERENCE.to_owned(),
        inspection_limits: limits.into(),
        counts,
        features,
        status: OSM_OBSERVATION_STATUS.to_owned(),
        claim_boundary: "This report preserves a content-locked public map observation. It does not establish map completeness, legal access, indoor connectivity, geometry, elevation, widths, capacities, accessibility, demand, route choice, runtime validity, or any operational or safety outcome.".to_owned(),
        required_authoring: vec![
            "Confirm the source extract, ODbL obligations, and the stated OpenStreetMap attribution before reuse or publication.".to_owned(),
            "Survey or otherwise verify every modeled walkable boundary, obstacle, entrance, connector, elevation, width, direction, and accessibility property.".to_owned(),
            "Choose and document a local metre coordinate system; do not copy geographic degrees into Chiyoda geometry.".to_owned(),
            "Author all capacities, agent demand, releases, destinations, and behavioral assumptions explicitly, then run sensitivity studies for material best guesses.".to_owned(),
        ],
    })
}

/// Rebuild a persisted layout observation report from its catalog and locked
/// OSM XML source. The persisted limits are part of the report contract, so a
/// later default-limit change cannot make a prior observation unverifiable.
pub fn verify_openstreetmap_layout_report(
    catalog: &EvidenceCatalog,
    data_root: &Path,
    report: &OpenStreetMapLayoutReport,
) -> Result<(), OsmLayoutError> {
    let limits = OsmInspectionLimits {
        max_nodes: report_limit("max_nodes", report.inspection_limits.max_nodes)?,
        max_ways: report_limit("max_ways", report.inspection_limits.max_ways)?,
        max_node_references_per_way: report_limit(
            "max_node_references_per_way",
            report.inspection_limits.max_node_references_per_way,
        )?,
        max_tags_per_object: report_limit(
            "max_tags_per_object",
            report.inspection_limits.max_tags_per_object,
        )?,
    };
    let rebuilt = inspect_openstreetmap_layout(catalog, data_root, limits)?;
    if report != &rebuilt {
        return Err(OsmLayoutError::ReportMismatch);
    }
    Ok(())
}

/// Derive an explicitly anchored local east/north reference from a source
/// observation report. This is the EPSG:9837 sequence: WGS84 geographic
/// coordinates at ellipsoidal height zero are converted to ECEF, then to a
/// local ENU frame at the supplied origin. Values are rounded to a micrometre
/// for a reproducible artifact; the precision is not a survey-accuracy claim.
pub fn project_openstreetmap_layout_report(
    report: &OpenStreetMapLayoutReport,
    origin: GeographicPoint,
) -> Result<OpenStreetMapLocalProjectionReport, OsmLayoutError> {
    validate_projection_source_report(report)?;
    validate_projection_origin(origin)?;
    let source_observation_report_sha256 =
        sha256_bytes(&serde_json::to_vec(report).map_err(OsmLayoutError::ProjectionSerialization)?);
    let coordinate_reference = LocalTangentPlane {
        method: LOCAL_PROJECTION_METHOD.to_owned(),
        origin,
        origin_ellipsoidal_height_m: 0.0,
        output_axes: "east_m and north_m in a local tangent plane; no vertical output".to_owned(),
        output_precision_m: 1.0 / LOCAL_PROJECTION_PRECISION_M,
        reference: LOCAL_PROJECTION_REFERENCE.to_owned(),
    };
    let features = report
        .features
        .iter()
        .map(|feature| project_feature(feature, origin))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(OpenStreetMapLocalProjectionReport {
        schema_version: "0.1".to_owned(),
        adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
        source_observation_report_sha256,
        source: report.source.clone(),
        coordinate_reference,
        counts: report.counts.clone(),
        features,
        status: "source_projection_only".to_owned(),
        claim_boundary: "This report applies one explicitly chosen local WGS84 tangent-plane transformation to a verified map observation. It is not a survey, does not establish map completeness, legal access, indoor connectivity, exact facility geometry, elevation, widths, capacities, accessibility, demand, route choice, runtime validity, or any operational or safety outcome.".to_owned(),
        required_authoring: vec![
            "Confirm the source extract, ODbL obligations, and the stated OpenStreetMap attribution before reuse or publication.".to_owned(),
            "Review whether the chosen tangent-plane origin and the source coordinate reference are suitable for the intended local authoring task.".to_owned(),
            "Treat projected point coordinates as an authoring reference only; survey or otherwise verify every modeled boundary, obstacle, entrance, connector, elevation, width, direction, and accessibility property.".to_owned(),
            "Do not treat projected way-bound corners as a walkable polygon, path, or projected extent. Author all scenario geometry, capacities, demand, releases, destinations, and behavioral assumptions explicitly, then run sensitivity studies for material best guesses.".to_owned(),
        ],
    })
}

fn validate_projection_source_report(
    report: &OpenStreetMapLayoutReport,
) -> Result<(), OsmLayoutError> {
    for (field, expected, actual) in [
        (
            "schema_version",
            OSM_OBSERVATION_SCHEMA_VERSION,
            report.schema_version.as_str(),
        ),
        ("status", OSM_OBSERVATION_STATUS, report.status.as_str()),
        (
            "coordinate_reference",
            OSM_GEOGRAPHIC_COORDINATE_REFERENCE,
            report.coordinate_reference.as_str(),
        ),
        (
            "source.license",
            OSM_LICENSE,
            report.source.license.as_str(),
        ),
    ] {
        if actual != expected {
            return Err(OsmLayoutError::InvalidProjectionSourceReport {
                field,
                expected,
                actual: actual.to_owned(),
            });
        }
    }
    if !report
        .source
        .required_attribution
        .contains(REQUIRED_OSM_ATTRIBUTION)
    {
        return Err(OsmLayoutError::InvalidProjectionSourceReport {
            field: "source.required_attribution",
            expected: REQUIRED_OSM_ATTRIBUTION,
            actual: report.source.required_attribution.clone(),
        });
    }
    Ok(())
}

/// Rebuild a projected reference from its source-observation report and the
/// persisted origin. Callers that accept external artifacts must verify the
/// source observation report against its catalog and locked XML first.
pub fn verify_openstreetmap_local_projection_report(
    report: &OpenStreetMapLayoutReport,
    projection: &OpenStreetMapLocalProjectionReport,
) -> Result<(), OsmLayoutError> {
    let rebuilt =
        project_openstreetmap_layout_report(report, projection.coordinate_reference.origin)?;
    if projection != &rebuilt {
        return Err(OsmLayoutError::ProjectionMismatch);
    }
    Ok(())
}

fn project_feature(
    feature: &OsmFeatureObservation,
    origin: GeographicPoint,
) -> Result<ProjectedOsmFeatureObservation, OsmLayoutError> {
    let geometry = match &feature.geometry {
        OsmFeatureGeometry::Point { coordinate } => ProjectedOsmFeatureGeometry::Point {
            coordinate: project_point(*coordinate, origin)?,
        },
        OsmFeatureGeometry::Bounds {
            bounds,
            referenced_node_count,
        } => ProjectedOsmFeatureGeometry::BoundsCorners {
            corners: project_bounds_corners(*bounds, origin)?,
            referenced_node_count: *referenced_node_count,
        },
    };
    Ok(ProjectedOsmFeatureObservation {
        object_type: feature.object_type.clone(),
        object_id: feature.object_id,
        categories: feature.categories.clone(),
        relevant_tags: feature.relevant_tags.clone(),
        geometry,
    })
}

fn project_bounds_corners(
    bounds: GeographicBounds,
    origin: GeographicPoint,
) -> Result<LocalMetreBoundsCornerReference, OsmLayoutError> {
    validate_observed_point(
        GeographicPoint {
            latitude: bounds.minimum_latitude,
            longitude: bounds.minimum_longitude,
        },
        "minimum",
    )?;
    validate_observed_point(
        GeographicPoint {
            latitude: bounds.maximum_latitude,
            longitude: bounds.maximum_longitude,
        },
        "maximum",
    )?;
    if bounds.minimum_latitude > bounds.maximum_latitude {
        return Err(OsmLayoutError::InvalidObservedCoordinate {
            field: "bounds latitude order",
            value: bounds.minimum_latitude,
        });
    }
    if bounds.minimum_longitude > bounds.maximum_longitude {
        return Err(OsmLayoutError::InvalidObservedCoordinate {
            field: "bounds longitude order",
            value: bounds.minimum_longitude,
        });
    }
    let longitude_span = bounds.maximum_longitude - bounds.minimum_longitude;
    if longitude_span > 180.0 {
        return Err(OsmLayoutError::AmbiguousBoundsLongitudeSpan {
            span_degrees: longitude_span,
        });
    }
    let southwest = project_point(
        GeographicPoint {
            latitude: bounds.minimum_latitude,
            longitude: bounds.minimum_longitude,
        },
        origin,
    )?;
    let southeast = project_point(
        GeographicPoint {
            latitude: bounds.minimum_latitude,
            longitude: bounds.maximum_longitude,
        },
        origin,
    )?;
    let northwest = project_point(
        GeographicPoint {
            latitude: bounds.maximum_latitude,
            longitude: bounds.minimum_longitude,
        },
        origin,
    )?;
    let northeast = project_point(
        GeographicPoint {
            latitude: bounds.maximum_latitude,
            longitude: bounds.maximum_longitude,
        },
        origin,
    )?;
    Ok(LocalMetreBoundsCornerReference {
        source_bounds: bounds,
        southwest,
        southeast,
        northwest,
        northeast,
    })
}

fn project_point(
    coordinate: GeographicPoint,
    origin: GeographicPoint,
) -> Result<LocalMetrePoint, OsmLayoutError> {
    validate_observed_point(coordinate, "coordinate")?;
    let (x, y, z) = geodetic_to_ecef(coordinate);
    let (origin_x, origin_y, origin_z) = geodetic_to_ecef(origin);
    let latitude = origin.latitude.to_radians();
    let longitude = origin.longitude.to_radians();
    let delta_x = x - origin_x;
    let delta_y = y - origin_y;
    let delta_z = z - origin_z;
    let east = -longitude.sin() * delta_x + longitude.cos() * delta_y;
    let north = -latitude.sin() * longitude.cos() * delta_x
        - latitude.sin() * longitude.sin() * delta_y
        + latitude.cos() * delta_z;
    Ok(LocalMetrePoint {
        east_m: quantize_projection_axis(east, "east")?,
        north_m: quantize_projection_axis(north, "north")?,
    })
}

fn geodetic_to_ecef(point: GeographicPoint) -> (f64, f64, f64) {
    let latitude = point.latitude.to_radians();
    let longitude = point.longitude.to_radians();
    let flattening = 1.0 / WGS84_INVERSE_FLATTENING;
    let eccentricity_squared = flattening * (2.0 - flattening);
    let prime_vertical_radius =
        WGS84_SEMI_MAJOR_AXIS_M / (1.0 - eccentricity_squared * latitude.sin().powi(2)).sqrt();
    (
        prime_vertical_radius * latitude.cos() * longitude.cos(),
        prime_vertical_radius * latitude.cos() * longitude.sin(),
        prime_vertical_radius * (1.0 - eccentricity_squared) * latitude.sin(),
    )
}

fn quantize_projection_axis(value: f64, axis: &'static str) -> Result<f64, OsmLayoutError> {
    if !value.is_finite() {
        return Err(OsmLayoutError::NonFiniteProjection { axis });
    }
    let quantized = (value * LOCAL_PROJECTION_PRECISION_M).round() / LOCAL_PROJECTION_PRECISION_M;
    if !quantized.is_finite() {
        return Err(OsmLayoutError::NonFiniteProjection { axis });
    }
    Ok(if quantized == 0.0 { 0.0 } else { quantized })
}

fn validate_projection_origin(origin: GeographicPoint) -> Result<(), OsmLayoutError> {
    validate_coordinate(origin.latitude, true).map_err(|value| {
        OsmLayoutError::InvalidProjectionOrigin {
            field: "latitude",
            value,
        }
    })?;
    validate_coordinate(origin.longitude, false).map_err(|value| {
        OsmLayoutError::InvalidProjectionOrigin {
            field: "longitude",
            value,
        }
    })?;
    Ok(())
}

fn validate_observed_point(
    point: GeographicPoint,
    prefix: &'static str,
) -> Result<(), OsmLayoutError> {
    validate_coordinate(point.latitude, true).map_err(|value| {
        OsmLayoutError::InvalidObservedCoordinate {
            field: if prefix == "coordinate" {
                "coordinate latitude"
            } else if prefix == "minimum" {
                "minimum latitude"
            } else {
                "maximum latitude"
            },
            value,
        }
    })?;
    validate_coordinate(point.longitude, false).map_err(|value| {
        OsmLayoutError::InvalidObservedCoordinate {
            field: if prefix == "coordinate" {
                "coordinate longitude"
            } else if prefix == "minimum" {
                "minimum longitude"
            } else {
                "maximum longitude"
            },
            value,
        }
    })?;
    Ok(())
}

fn validate_coordinate(value: f64, latitude: bool) -> Result<(), f64> {
    let (minimum, maximum) = if latitude {
        (-90.0, 90.0)
    } else {
        (-180.0, 180.0)
    };
    if value.is_finite() && (minimum..=maximum).contains(&value) {
        Ok(())
    } else {
        Err(value)
    }
}

fn report_limit(field: &'static str, value: u64) -> Result<usize, OsmLayoutError> {
    usize::try_from(value).map_err(|_| OsmLayoutError::ReportLimitOutOfRange { field })
}

fn validate_limits(limits: OsmInspectionLimits) -> Result<(), OsmLayoutError> {
    for (name, value) in [
        ("max_nodes", limits.max_nodes),
        ("max_ways", limits.max_ways),
        (
            "max_node_references_per_way",
            limits.max_node_references_per_way,
        ),
        ("max_tags_per_object", limits.max_tags_per_object),
    ] {
        if value == 0 {
            return Err(OsmLayoutError::InvalidLimit { name });
        }
    }
    Ok(())
}

fn is_osm_xml_path(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("osm") || extension.eq_ignore_ascii_case("xml")
    })
}

#[allow(clippy::too_many_lines)] // one streaming state machine keeps XML object ownership explicit
fn inspect_osm_xml(
    path: &Path,
    limits: OsmInspectionLimits,
) -> Result<(OsmLayoutCounts, Vec<OsmFeatureObservation>), OsmLayoutError> {
    let file = File::open(path).map_err(|source| OsmLayoutError::ReadFile {
        path: path.to_owned(),
        source,
    })?;
    let reader = ParserConfig::new()
        .trim_whitespace(true)
        .ignore_comments(true)
        .coalesce_characters(false)
        .allow_multiple_root_elements(false)
        .max_entity_expansion_length(8 * 1024)
        .max_entity_expansion_depth(8)
        .max_attributes(256)
        .max_name_length(256)
        .max_attribute_length(8 * 1024)
        .max_data_length(8 * 1024)
        .create_reader(BufReader::new(file));

    let mut root_seen = false;
    let mut nodes = BTreeMap::new();
    let mut selected_ways = Vec::new();
    let mut features = Vec::new();
    let mut counts = OsmLayoutCounts::default();
    let mut current_node = None;
    let mut current_way = None;

    for event in reader {
        let event = event.map_err(|error| OsmLayoutError::Xml {
            path: path.to_owned(),
            message: error.to_string(),
        })?;
        match event {
            XmlEvent::StartElement {
                name, attributes, ..
            } => {
                let is_root_start = !root_seen;
                if is_root_start {
                    if name.local_name != "osm" {
                        return Err(OsmLayoutError::WrongRoot {
                            path: path.to_owned(),
                        });
                    }
                    root_seen = true;
                }
                match name.local_name.as_str() {
                    "osm" => {
                        if !is_root_start || current_node.is_some() || current_way.is_some() {
                            return Err(OsmLayoutError::WrongRoot {
                                path: path.to_owned(),
                            });
                        }
                    }
                    "node" => {
                        counts.parsed_nodes =
                            counts.parsed_nodes.checked_add(1).expect("u64 overflow");
                        if counts.parsed_nodes
                            > u64::try_from(limits.max_nodes).expect("usize fits u64")
                        {
                            return Err(OsmLayoutError::LimitExceeded {
                                name: "max_nodes",
                                limit: limits.max_nodes,
                            });
                        }
                        current_node = Some(node_from_attributes(&attributes)?);
                    }
                    "way" => {
                        counts.parsed_ways =
                            counts.parsed_ways.checked_add(1).expect("u64 overflow");
                        if counts.parsed_ways
                            > u64::try_from(limits.max_ways).expect("usize fits u64")
                        {
                            return Err(OsmLayoutError::LimitExceeded {
                                name: "max_ways",
                                limit: limits.max_ways,
                            });
                        }
                        current_way = Some(way_from_attributes(&attributes)?);
                    }
                    "tag" => {
                        let key = required_attribute(&attributes, "k", "tag")?;
                        let value = required_attribute(&attributes, "v", "tag")?;
                        if let Some(node) = &mut current_node {
                            insert_tag(
                                &mut node.tags,
                                &mut node.tag_count,
                                node.id,
                                "node",
                                key,
                                value,
                                limits,
                            )?;
                        } else if let Some(way) = &mut current_way {
                            insert_tag(
                                &mut way.tags,
                                &mut way.tag_count,
                                way.id,
                                "way",
                                key,
                                value,
                                limits,
                            )?;
                        }
                    }
                    "nd" => {
                        if let Some(way) = &mut current_way {
                            if way.node_references.len() == limits.max_node_references_per_way {
                                return Err(OsmLayoutError::LimitExceeded {
                                    name: "max_node_references_per_way",
                                    limit: limits.max_node_references_per_way,
                                });
                            }
                            let value = required_attribute(&attributes, "ref", "nd")?;
                            way.node_references
                                .push(parse_identifier("nd", "ref", &value)?);
                        }
                    }
                    _ => {}
                }
            }
            XmlEvent::EndElement { name } => match name.local_name.as_str() {
                "node" => {
                    if let Some(node) = current_node.take() {
                        if nodes.insert(node.id, node.coordinate).is_some() {
                            return Err(OsmLayoutError::DuplicateIdentifier {
                                object_type: "node",
                                id: node.id,
                            });
                        }
                        if let Some(categories) = classify(&node.tags) {
                            counts.selected_node_features = counts
                                .selected_node_features
                                .checked_add(1)
                                .expect("u64 overflow");
                            increment_categories(&mut counts, &categories);
                            features.push(OsmFeatureObservation {
                                object_type: "node".to_owned(),
                                object_id: node.id,
                                categories,
                                relevant_tags: relevant_tags(&node.tags),
                                geometry: OsmFeatureGeometry::Point {
                                    coordinate: node.coordinate,
                                },
                            });
                        }
                    }
                }
                "way" => {
                    if let Some(way) = current_way.take()
                        && let Some(categories) = classify(&way.tags)
                    {
                        selected_ways.push((way, categories));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    if !root_seen {
        return Err(OsmLayoutError::WrongRoot {
            path: path.to_owned(),
        });
    }

    let mut seen_ways = BTreeSet::new();
    for (way, categories) in selected_ways {
        if !seen_ways.insert(way.id) {
            return Err(OsmLayoutError::DuplicateIdentifier {
                object_type: "way",
                id: way.id,
            });
        }
        let bounds = way_bounds(&way, &nodes)?;
        counts.selected_way_features = counts
            .selected_way_features
            .checked_add(1)
            .expect("u64 overflow");
        increment_categories(&mut counts, &categories);
        features.push(OsmFeatureObservation {
            object_type: "way".to_owned(),
            object_id: way.id,
            categories,
            relevant_tags: relevant_tags(&way.tags),
            geometry: OsmFeatureGeometry::Bounds {
                bounds,
                referenced_node_count: u64::try_from(way.node_references.len())
                    .expect("usize fits u64"),
            },
        });
    }
    features.sort_by(|left, right| {
        left.object_type
            .cmp(&right.object_type)
            .then(left.object_id.cmp(&right.object_id))
    });
    Ok((counts, features))
}

fn node_from_attributes(attributes: &[OwnedAttribute]) -> Result<NodeBuilder, OsmLayoutError> {
    let id = parse_identifier("node", "id", &required_attribute(attributes, "id", "node")?)?;
    let latitude = parse_coordinate(
        "node",
        "lat",
        &required_attribute(attributes, "lat", "node")?,
        -90.0,
        90.0,
    )?;
    let longitude = parse_coordinate(
        "node",
        "lon",
        &required_attribute(attributes, "lon", "node")?,
        -180.0,
        180.0,
    )?;
    Ok(NodeBuilder {
        id,
        coordinate: GeographicPoint {
            latitude,
            longitude,
        },
        tags: BTreeMap::new(),
        tag_count: 0,
    })
}

fn way_from_attributes(attributes: &[OwnedAttribute]) -> Result<WayBuilder, OsmLayoutError> {
    Ok(WayBuilder {
        id: parse_identifier("way", "id", &required_attribute(attributes, "id", "way")?)?,
        node_references: Vec::new(),
        tags: BTreeMap::new(),
        tag_count: 0,
    })
}

fn required_attribute(
    attributes: &[OwnedAttribute],
    name: &'static str,
    element: &'static str,
) -> Result<String, OsmLayoutError> {
    attributes
        .iter()
        .find(|attribute| attribute.name.local_name == name)
        .map(|attribute| attribute.value.clone())
        .ok_or_else(|| OsmLayoutError::MissingAttribute {
            element: element.to_owned(),
            attribute: name,
        })
}

fn parse_identifier(
    element: &'static str,
    attribute: &'static str,
    value: &str,
) -> Result<i64, OsmLayoutError> {
    value.parse().map_err(|_| OsmLayoutError::InvalidAttribute {
        element: element.to_owned(),
        attribute,
        value: value.to_owned(),
    })
}

fn parse_coordinate(
    element: &'static str,
    attribute: &'static str,
    value: &str,
    minimum: f64,
    maximum: f64,
) -> Result<f64, OsmLayoutError> {
    let coordinate = value
        .parse::<f64>()
        .map_err(|_| OsmLayoutError::InvalidAttribute {
            element: element.to_owned(),
            attribute,
            value: value.to_owned(),
        })?;
    if !coordinate.is_finite() || !(minimum..=maximum).contains(&coordinate) {
        return Err(OsmLayoutError::InvalidAttribute {
            element: element.to_owned(),
            attribute,
            value: value.to_owned(),
        });
    }
    Ok(coordinate)
}

fn insert_tag(
    tags: &mut BTreeMap<String, String>,
    tag_count: &mut usize,
    id: i64,
    object_type: &'static str,
    key: String,
    value: String,
    limits: OsmInspectionLimits,
) -> Result<(), OsmLayoutError> {
    if *tag_count == limits.max_tags_per_object {
        return Err(OsmLayoutError::LimitExceeded {
            name: "max_tags_per_object",
            limit: limits.max_tags_per_object,
        });
    }
    *tag_count += 1;
    if tags.insert(key.clone(), value).is_some() {
        return Err(OsmLayoutError::DuplicateTag {
            object_type,
            id,
            key,
        });
    }
    Ok(())
}

fn classify(tags: &BTreeMap<String, String>) -> Option<Vec<String>> {
    let mut categories = BTreeSet::new();
    match tags.get("highway").map(String::as_str) {
        Some("elevator") => {
            categories.insert("elevator".to_owned());
        }
        Some("steps") => {
            categories.insert("steps".to_owned());
        }
        Some("footway" | "pedestrian") => {
            categories.insert("pedestrian_way".to_owned());
        }
        _ => {}
    }
    if matches!(tags.get("building"), Some(value) if value != "no")
        || tags.contains_key("building:part")
    {
        categories.insert("building".to_owned());
    }
    if matches!(
        tags.get("indoor").map(String::as_str),
        Some("area" | "corridor" | "room")
    ) {
        categories.insert("indoor_area".to_owned());
    }
    if tags.contains_key("entrance") {
        categories.insert("entrance".to_owned());
    }
    if matches!(
        tags.get("public_transport").map(String::as_str),
        Some("platform")
    ) || matches!(tags.get("railway").map(String::as_str), Some("platform"))
    {
        categories.insert("platform".to_owned());
    }
    if matches!(
        tags.get("public_transport").map(String::as_str),
        Some("station")
    ) || matches!(tags.get("railway").map(String::as_str), Some("station"))
    {
        categories.insert("station".to_owned());
    }
    (!categories.is_empty()).then(|| categories.into_iter().collect())
}

fn relevant_tags(tags: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    tags.iter()
        .filter(|(key, _)| RELEVANT_TAG_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn increment_categories(counts: &mut OsmLayoutCounts, categories: &[String]) {
    for category in categories {
        *counts
            .selected_features_by_category
            .entry(category.clone())
            .or_default() += 1;
    }
}

fn way_bounds(
    way: &WayBuilder,
    nodes: &BTreeMap<i64, GeographicPoint>,
) -> Result<GeographicBounds, OsmLayoutError> {
    let mut coordinates = way.node_references.iter().map(|node_id| {
        nodes
            .get(node_id)
            .copied()
            .ok_or(OsmLayoutError::MissingNodeReference {
                way_id: way.id,
                node_id: *node_id,
            })
    });
    let first = coordinates
        .next()
        .transpose()?
        .ok_or(OsmLayoutError::MissingNodeReference {
            way_id: way.id,
            node_id: 0,
        })?;
    let mut bounds = GeographicBounds {
        minimum_latitude: first.latitude,
        minimum_longitude: first.longitude,
        maximum_latitude: first.latitude,
        maximum_longitude: first.longitude,
    };
    for coordinate in coordinates {
        let coordinate = coordinate?;
        bounds.minimum_latitude = bounds.minimum_latitude.min(coordinate.latitude);
        bounds.minimum_longitude = bounds.minimum_longitude.min(coordinate.longitude);
        bounds.maximum_latitude = bounds.maximum_latitude.max(coordinate.latitude);
        bounds.maximum_longitude = bounds.maximum_longitude.max(coordinate.longitude);
    }
    Ok(bounds)
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::{
        EvidenceCatalog, EvidencePurpose, GeographicPoint, OpenStreetMapLayoutReport,
        OsmInspectionLimits, OsmLayoutError, ProjectedOsmFeatureGeometry,
        inspect_openstreetmap_layout, project_openstreetmap_layout_report,
        verify_openstreetmap_layout_report, verify_openstreetmap_local_projection_report,
    };
    use crate::evidence::EvidenceFile;
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TestDirectory(PathBuf);

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn test_directory() -> TestDirectory {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after the Unix epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "chiyoda-layout-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("creating temporary test directory");
        TestDirectory(directory)
    }

    fn catalog(file: &str, bytes: &[u8]) -> EvidenceCatalog {
        EvidenceCatalog {
            schema_version: "0.1".to_owned(),
            purpose: EvidencePurpose::UncalibratedReference,
            dataset_id: "fixture-openstreetmap-layout".to_owned(),
            title: "fixture OSM layout".to_owned(),
            landing_page: "https://www.openstreetmap.org/".to_owned(),
            license: "ODbL-1.0".to_owned(),
            redistributable: true,
            attribution: Some("© OpenStreetMap contributors".to_owned()),
            citation: "OpenStreetMap contributors".to_owned(),
            files: vec![EvidenceFile {
                id: "extract".to_owned(),
                role: None,
                source_url: "https://example.test/station.osm".to_owned(),
                local_path: file.to_owned(),
                sha256: format!("{:x}", Sha256::digest(bytes)),
                size_bytes: u64::try_from(bytes.len()).expect("usize fits u64"),
                upstream_checksum: None,
                transformation: "inspect OSM XML as geographic map observations only".to_owned(),
            }],
            supported_primitives: "Mapped station, entrance, pedestrian-way, step, elevator, platform, building, and indoor tags only.".to_owned(),
            exclusions: "No geometry, elevation, capacity, connectivity, or behavioral inference.".to_owned(),
            split_rationale: None,
        }
    }

    fn report_from_fixture(xml: &[u8]) -> OpenStreetMapLayoutReport {
        let directory = test_directory();
        let file_name = "station.osm";
        fs::write(directory.0.join(file_name), xml).expect("writing OSM fixture");
        inspect_openstreetmap_layout(
            &catalog(file_name, xml),
            &directory.0,
            OsmInspectionLimits::default(),
        )
        .expect("fixture inspection succeeds")
    }

    #[test]
    fn inspection_preserves_recognized_map_observations_without_scenario_geometry() {
        let report = report_from_fixture(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<osm version="0.6">
  <node id="1" lat="1.3000" lon="103.8000"><tag k="railway" v="station"/></node>
  <node id="2" lat="1.3001" lon="103.8000"><tag k="entrance" v="yes"/></node>
  <node id="3" lat="1.3001" lon="103.8001"/>
  <way id="9"><nd ref="1"/><nd ref="2"/><nd ref="3"/><tag k="highway" v="steps"/><tag k="level" v="-1;0"/></way>
  <way id="10"><nd ref="1"/><nd ref="3"/><tag k="public_transport" v="platform"/></way>
</osm>"#,
        );

        assert_eq!(report.status, "source_observation_only");
        assert_eq!(report.counts.parsed_nodes, 3);
        assert_eq!(report.counts.parsed_ways, 2);
        assert_eq!(report.counts.selected_node_features, 2);
        assert_eq!(report.counts.selected_way_features, 2);
        assert_eq!(report.counts.selected_features_by_category["steps"], 1);
        assert_eq!(report.counts.selected_features_by_category["platform"], 1);
        assert_eq!(report.features[2].object_id, 9);
        assert_eq!(report.features[2].relevant_tags["level"], "-1;0");
        assert!(report.coordinate_reference.contains("WGS84"));
        assert!(report.claim_boundary.contains("does not establish"));
    }

    #[test]
    fn inspection_rejects_a_selected_way_with_an_unavailable_node() {
        let directory = test_directory();
        let xml = br#"<osm version="0.6"><way id="9"><nd ref="5"/><tag k="highway" v="steps"/></way></osm>"#;
        let file_name = "station.osm";
        fs::write(directory.0.join(file_name), xml).expect("writing OSM fixture");

        let error = inspect_openstreetmap_layout(
            &catalog(file_name, xml),
            &directory.0,
            OsmInspectionLimits::default(),
        )
        .expect_err("missing selected-way nodes must fail");

        assert!(matches!(
            error,
            OsmLayoutError::MissingNodeReference {
                way_id: 9,
                node_id: 5
            }
        ));
    }

    #[test]
    fn inspection_requires_contributor_attribution() {
        let directory = test_directory();
        let xml = br#"<osm version="0.6"/>"#;
        let file_name = "station.osm";
        fs::write(directory.0.join(file_name), xml).expect("writing OSM fixture");
        let mut source_catalog = catalog(file_name, xml);
        source_catalog.attribution = Some("map source".to_owned());

        let error = inspect_openstreetmap_layout(
            &source_catalog,
            &directory.0,
            OsmInspectionLimits::default(),
        )
        .expect_err("OSM attribution must be explicit");

        assert!(matches!(
            error,
            OsmLayoutError::MissingOpenStreetMapAttribution
        ));
    }

    #[test]
    fn inspection_rejects_non_osm_xml_roots() {
        let directory = test_directory();
        let xml = br"<layout/>";
        let file_name = "station.osm";
        fs::write(directory.0.join(file_name), xml).expect("writing XML fixture");

        let error = inspect_openstreetmap_layout(
            &catalog(file_name, xml),
            &directory.0,
            OsmInspectionLimits::default(),
        )
        .expect_err("non-OSM roots must fail");

        assert!(matches!(error, OsmLayoutError::WrongRoot { .. }));
    }

    #[test]
    fn verification_reconstructs_the_report_and_rejects_tampering() {
        let directory = test_directory();
        let xml = br#"<osm version="0.6"><node id="1" lat="1.3" lon="103.8"><tag k="entrance" v="yes"/></node></osm>"#;
        let file_name = "station.osm";
        fs::write(directory.0.join(file_name), xml).expect("writing OSM fixture");
        let source_catalog = catalog(file_name, xml);
        let report = inspect_openstreetmap_layout(
            &source_catalog,
            &directory.0,
            OsmInspectionLimits::default(),
        )
        .expect("source inspection succeeds");

        verify_openstreetmap_layout_report(&source_catalog, &directory.0, &report)
            .expect("matching report verifies");
        let mut altered = report;
        altered.counts.selected_node_features = 0;
        assert!(matches!(
            verify_openstreetmap_layout_report(&source_catalog, &directory.0, &altered),
            Err(OsmLayoutError::ReportMismatch)
        ));
    }

    #[test]
    fn local_projection_anchors_points_and_preserves_way_bounds_as_corner_references() {
        let report = report_from_fixture(
            br#"<osm version="0.6">
  <node id="1" lat="1.3000" lon="103.8000"><tag k="railway" v="station"/></node>
  <node id="2" lat="1.3001" lon="103.8000"><tag k="entrance" v="yes"/></node>
  <node id="3" lat="1.3001" lon="103.8001"/>
  <way id="9"><nd ref="1"/><nd ref="2"/><nd ref="3"/><tag k="highway" v="steps"/></way>
</osm>"#,
        );
        let projection = project_openstreetmap_layout_report(
            &report,
            GeographicPoint {
                latitude: 1.3,
                longitude: 103.8,
            },
        )
        .expect("projection succeeds");

        assert_eq!(projection.status, "source_projection_only");
        assert!((projection.coordinate_reference.origin.latitude - 1.3).abs() < f64::EPSILON);
        assert!((projection.coordinate_reference.origin.longitude - 103.8).abs() < f64::EPSILON);
        assert!(
            projection
                .coordinate_reference
                .origin_ellipsoidal_height_m
                .abs()
                < f64::EPSILON
        );
        assert!(
            (projection.coordinate_reference.output_precision_m - 0.000_001).abs() < f64::EPSILON
        );
        assert!(projection.coordinate_reference.method.contains("EPSG:9837"));
        assert_eq!(projection.source_observation_report_sha256.len(), 64);

        let origin_feature = projection
            .features
            .iter()
            .find(|feature| feature.object_id == 1)
            .expect("origin feature exists");
        let ProjectedOsmFeatureGeometry::Point { coordinate } = &origin_feature.geometry else {
            panic!("station node must remain a point")
        };
        assert!(coordinate.east_m.abs() < f64::EPSILON);
        assert!(coordinate.north_m.abs() < f64::EPSILON);

        let way_feature = projection
            .features
            .iter()
            .find(|feature| feature.object_id == 9)
            .expect("way feature exists");
        let ProjectedOsmFeatureGeometry::BoundsCorners {
            corners,
            referenced_node_count,
        } = &way_feature.geometry
        else {
            panic!("way must remain transformed bounds corners")
        };
        assert_eq!(*referenced_node_count, 3);
        assert!(corners.southwest.east_m.abs() < f64::EPSILON);
        assert!(corners.southwest.north_m.abs() < f64::EPSILON);
        assert!(corners.southeast.east_m > 10.0);
        assert!(corners.northwest.north_m > 10.0);

        verify_openstreetmap_local_projection_report(&report, &projection)
            .expect("matching projection verifies");
    }

    #[test]
    fn local_projection_rejects_invalid_origins_and_tampering() {
        let report = report_from_fixture(
            br#"<osm version="0.6"><node id="1" lat="1.3" lon="103.8"><tag k="entrance" v="yes"/></node></osm>"#,
        );
        let mut wrong_reference = report.clone();
        wrong_reference.coordinate_reference = "unverified coordinates".to_owned();
        let error = project_openstreetmap_layout_report(
            &wrong_reference,
            GeographicPoint {
                latitude: 1.3,
                longitude: 103.8,
            },
        )
        .expect_err("projection must reject an unverified coordinate reference");
        assert!(matches!(
            error,
            OsmLayoutError::InvalidProjectionSourceReport {
                field: "coordinate_reference",
                ..
            }
        ));

        let error = project_openstreetmap_layout_report(
            &report,
            GeographicPoint {
                latitude: 91.0,
                longitude: 103.8,
            },
        )
        .expect_err("out-of-range projection origin must fail");
        assert!(matches!(
            error,
            OsmLayoutError::InvalidProjectionOrigin {
                field: "latitude",
                ..
            }
        ));

        let mut projection = project_openstreetmap_layout_report(
            &report,
            GeographicPoint {
                latitude: 1.3,
                longitude: 103.8,
            },
        )
        .expect("projection succeeds");
        projection.source_observation_report_sha256 = "tampered".to_owned();
        assert!(matches!(
            verify_openstreetmap_local_projection_report(&report, &projection),
            Err(OsmLayoutError::ProjectionMismatch)
        ));
    }

    #[test]
    fn local_projection_rejects_antimeridian_ambiguous_way_bounds() {
        let report = report_from_fixture(
            br#"<osm version="0.6">
  <node id="1" lat="0" lon="-179.9"><tag k="railway" v="station"/></node>
  <node id="2" lat="0" lon="179.9"/>
  <way id="9"><nd ref="1"/><nd ref="2"/><tag k="highway" v="steps"/></way>
</osm>"#,
        );
        let error = project_openstreetmap_layout_report(
            &report,
            GeographicPoint {
                latitude: 0.0,
                longitude: 180.0,
            },
        )
        .expect_err("antimeridian-ambiguous source bounds must not be projected");
        assert!(matches!(
            error,
            OsmLayoutError::AmbiguousBoundsLongitudeSpan { .. }
        ));
    }
}
