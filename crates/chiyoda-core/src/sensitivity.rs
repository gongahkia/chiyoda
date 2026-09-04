//! Declarative, data-free sensitivity-study planning for authored scenarios.
//!
//! A sensitivity study enumerates explicit alternatives to authored inputs. It
//! deliberately does not attach probability distributions, confidence levels,
//! or empirical interpretations to those alternatives.

use crate::{
    model::{Connector, Scenario},
    validate,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use thiserror::Error;

const STUDY_SCHEMA_VERSION: &str = "0.1";
const DEFAULT_TRACE_EVERY_STEPS: u32 = 10;
const DEFAULT_MAX_CONDITIONS: u32 = 256;

/// An explicit set of authored alternatives for an uncalibrated sensitivity study.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensitivityManifest {
    pub schema_version: String,
    pub name: String,
    pub description: String,
    /// Path interpreted by the CLI relative to the manifest file.
    pub baseline_source: String,
    pub first_seed: u64,
    pub count: u32,
    #[serde(default = "default_trace_every_steps")]
    pub trace_every_steps: u32,
    #[serde(default)]
    pub design: SensitivityDesign,
    #[serde(default = "default_max_conditions")]
    pub max_conditions: u32,
    pub factors: Vec<SensitivityFactor>,
    /// The author's own intended-use and interpretation boundary.
    pub claim_boundary: String,
}

fn default_trace_every_steps() -> u32 {
    DEFAULT_TRACE_EVERY_STEPS
}

fn default_max_conditions() -> u32 {
    DEFAULT_MAX_CONDITIONS
}

/// How the explicitly listed factor alternatives are composed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitivityDesign {
    /// Vary one factor from the baseline at a time.
    #[default]
    OneAtATime,
    /// Evaluate the Cartesian product of every factor's alternatives.
    FullFactorial,
}

/// How an author obtained or selected a factor's baseline and alternatives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssumptionBasis {
    /// A deliberately disclosed value chosen without a qualifying dataset.
    BestGuess,
    /// A documented but uncalibrated engineering or domain estimate.
    DocumentedEstimate,
    /// A value chosen to expose a reference-semantics or structural assumption.
    StructuralAssumption,
    /// A directly measured scenario input; this label alone does not calibrate the runtime.
    MeasuredInput,
}

/// One mutable numeric input and its declared alternative values.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensitivityFactor {
    pub id: String,
    pub target: SensitivityTarget,
    /// Identifier of the agent group, exit, connector, gate, message, or countermeasure.
    pub subject: String,
    pub values: Vec<f64>,
    pub basis: AssumptionBasis,
    pub rationale: String,
    /// Retained source context for a documented estimate or measured input.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<SensitivityReference>,
}

/// A disclosed source and its applicability boundary for one sensitivity factor.
///
/// The record preserves why an author selected an alternative. It does not
/// attach a statistical distribution to the alternative or calibrate the
/// runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensitivityReference {
    pub id: String,
    pub citation: String,
    pub url: String,
    pub applicability: String,
    pub limitation: String,
    /// Optional SHA-256 of the precise open-data file informing the factor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sha256: Option<String>,
    /// An optional local descriptive report that selected these alternatives.
    /// The CLI content-locks and snapshots it with the executed study.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_report: Option<SensitivityDerivedReport>,
}

/// A source report declared by a sensitivity author.
///
/// `path` is resolved relative to the sensitivity manifest. The report is not
/// interpreted as calibration evidence; its byte hash only pins the exact
/// descriptive artifact that informed a factor's values.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensitivityDerivedReport {
    pub path: String,
    pub sha256: String,
}

/// Numeric scenario fields that can be varied without changing the DSL grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitivityTarget {
    AgentCount,
    AgentSpeedMps,
    AgentRadiusM,
    AgentHeightM,
    AgentReleaseAtS,
    AgentReleaseIntervalS,
    AgentReleaseBatchSize,
    ExitCapacityPerS,
    ConnectorCapacityPerS,
    EscalatorBeltSpeedMps,
    GateServiceRatePerS,
    MessageTrust,
    MessageReachM,
    CountermeasureTrust,
    CountermeasureReachM,
}

impl SensitivityTarget {
    #[must_use]
    pub fn unit(self) -> &'static str {
        match self {
            Self::AgentCount => "agents",
            Self::AgentSpeedMps | Self::EscalatorBeltSpeedMps => "m/s",
            Self::AgentRadiusM
            | Self::AgentHeightM
            | Self::MessageReachM
            | Self::CountermeasureReachM => "m",
            Self::AgentReleaseAtS | Self::AgentReleaseIntervalS => "s",
            Self::AgentReleaseBatchSize => "agents",
            Self::ExitCapacityPerS | Self::ConnectorCapacityPerS | Self::GateServiceRatePerS => {
                "/s"
            }
            Self::MessageTrust | Self::CountermeasureTrust => "probability",
        }
    }

    fn validate_value(self, value: f64, factor_id: &str) -> Result<(), SensitivityError> {
        if !value.is_finite() {
            return Err(SensitivityError::InvalidValue {
                factor_id: factor_id.to_owned(),
                message: "must be finite".to_owned(),
            });
        }
        match self {
            Self::AgentReleaseAtS if value < 0.0 => Err(SensitivityError::InvalidValue {
                factor_id: factor_id.to_owned(),
                message: "must be zero or greater".to_owned(),
            }),
            Self::AgentReleaseIntervalS if value <= 0.0 => Err(SensitivityError::InvalidValue {
                factor_id: factor_id.to_owned(),
                message: "must be greater than zero".to_owned(),
            }),
            Self::MessageTrust | Self::CountermeasureTrust if !(0.0..=1.0).contains(&value) => {
                Err(SensitivityError::InvalidValue {
                    factor_id: factor_id.to_owned(),
                    message: "must be between zero and one".to_owned(),
                })
            }
            Self::AgentCount | Self::AgentReleaseBatchSize
                if value < 1.0 || value.fract() != 0.0 || value > f64::from(u32::MAX) =>
            {
                Err(SensitivityError::InvalidValue {
                    factor_id: factor_id.to_owned(),
                    message: "must be a whole number between one and u32::MAX".to_owned(),
                })
            }
            Self::AgentCount
            | Self::AgentSpeedMps
            | Self::AgentRadiusM
            | Self::AgentHeightM
            | Self::ExitCapacityPerS
            | Self::ConnectorCapacityPerS
            | Self::EscalatorBeltSpeedMps
            | Self::GateServiceRatePerS
            | Self::MessageReachM
            | Self::CountermeasureReachM
                if value <= 0.0 =>
            {
                Err(SensitivityError::InvalidValue {
                    factor_id: factor_id.to_owned(),
                    message: "must be greater than zero".to_owned(),
                })
            }
            _ => Ok(()),
        }
    }
}

/// A concrete, validated scenario generated from one set of factor values.
#[derive(Debug, Clone)]
pub struct SensitivityCondition {
    pub id: String,
    pub factor_values: BTreeMap<String, f64>,
    pub scenario: Scenario,
}

/// A resolved sensitivity study, ready for deterministic replication by the CLI.
#[derive(Debug, Clone)]
pub struct SensitivityStudy {
    pub baseline_values: BTreeMap<String, f64>,
    pub conditions: Vec<SensitivityCondition>,
}

#[derive(Debug, Error)]
pub enum SensitivityError {
    #[error("sensitivity manifest schema must be `{STUDY_SCHEMA_VERSION}`")]
    UnsupportedSchema,
    #[error("sensitivity manifest {field} must not be empty")]
    EmptyField { field: &'static str },
    #[error("sensitivity manifest {field} must be greater than zero")]
    NonPositive { field: &'static str },
    #[error("sensitivity manifest must declare at least one factor")]
    NoFactors,
    #[error("sensitivity factor id `{id}` is not a safe path component")]
    UnsafeFactorId { id: String },
    #[error("duplicate sensitivity factor id `{id}`")]
    DuplicateFactorId { id: String },
    #[error("sensitivity factors `{first}` and `{second}` target the same field")]
    DuplicateTarget { first: String, second: String },
    #[error("sensitivity factor `{factor_id}` must declare at least two values")]
    InsufficientValues { factor_id: String },
    #[error("sensitivity factor `{factor_id}` repeats value `{value}`")]
    DuplicateValue { factor_id: String, value: f64 },
    #[error("sensitivity factor `{factor_id}` has an invalid value: {message}")]
    InvalidValue { factor_id: String, message: String },
    #[error(
        "sensitivity factor `{factor_id}` with basis `{basis}` requires at least one reference"
    )]
    MissingReference {
        factor_id: String,
        basis: &'static str,
    },
    #[error("sensitivity factor `{factor_id}` repeats reference id `{reference_id}`")]
    DuplicateReference {
        factor_id: String,
        reference_id: String,
    },
    #[error(
        "sensitivity reference `{reference_id}` for factor `{factor_id}` is invalid: {message}"
    )]
    InvalidReference {
        factor_id: String,
        reference_id: String,
        message: String,
    },
    #[error("sensitivity factor `{factor_id}` references no {kind} named `{subject}")]
    UnknownSubject {
        factor_id: String,
        kind: &'static str,
        subject: String,
    },
    #[error("sensitivity factor `{factor_id}` cannot vary {message}")]
    UnsupportedSubject { factor_id: String, message: String },
    #[error(
        "sensitivity design would create {condition_count} conditions, exceeding max_conditions {max_conditions}"
    )]
    TooManyConditions {
        condition_count: u64,
        max_conditions: u32,
    },
    #[error("sensitivity alternatives do not differ from the baseline scenario")]
    NoAlternatives,
    #[error("generated sensitivity condition `{condition_id}` is invalid: {message}")]
    InvalidCondition {
        condition_id: String,
        message: String,
    },
}

/// Validate and resolve a sensitivity manifest against its baseline scenario.
pub fn plan_sensitivity(
    manifest: &SensitivityManifest,
    baseline: &Scenario,
) -> Result<SensitivityStudy, SensitivityError> {
    validate_manifest_shape(manifest)?;
    let baseline_values = baseline_values(manifest, baseline)?;
    let factor_value_sets = factor_value_sets(manifest);
    let condition_count =
        planned_condition_count(manifest.design, &factor_value_sets, &baseline_values);
    if condition_count > u64::from(manifest.max_conditions) {
        return Err(SensitivityError::TooManyConditions {
            condition_count,
            max_conditions: manifest.max_conditions,
        });
    }
    let assignments = match manifest.design {
        SensitivityDesign::OneAtATime => {
            one_at_a_time_assignments(&factor_value_sets, &baseline_values)
        }
        SensitivityDesign::FullFactorial => {
            full_factorial_assignments(&factor_value_sets, &baseline_values)
        }
    };
    if assignments.is_empty() {
        return Err(SensitivityError::NoAlternatives);
    }

    let mut conditions = Vec::with_capacity(assignments.len());
    for (index, factor_values) in assignments.into_iter().enumerate() {
        let id = format!("case-{:04}", index + 1);
        let mut scenario = baseline.clone();
        for factor in &manifest.factors {
            if let Some(value) = factor_values.get(&factor.id) {
                apply_factor(&mut scenario, factor, *value)?;
            }
        }
        if let Err(errors) = validate(&scenario) {
            return Err(SensitivityError::InvalidCondition {
                condition_id: id,
                message: errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; "),
            });
        }
        conditions.push(SensitivityCondition {
            id,
            factor_values,
            scenario,
        });
    }
    Ok(SensitivityStudy {
        baseline_values,
        conditions,
    })
}

fn validate_manifest_shape(manifest: &SensitivityManifest) -> Result<(), SensitivityError> {
    if manifest.schema_version != STUDY_SCHEMA_VERSION {
        return Err(SensitivityError::UnsupportedSchema);
    }
    for (field, value) in [
        ("name", &manifest.name),
        ("description", &manifest.description),
        ("baseline_source", &manifest.baseline_source),
        ("claim_boundary", &manifest.claim_boundary),
    ] {
        if value.trim().is_empty() {
            return Err(SensitivityError::EmptyField { field });
        }
    }
    if manifest.count == 0 {
        return Err(SensitivityError::NonPositive { field: "count" });
    }
    if manifest.trace_every_steps == 0 {
        return Err(SensitivityError::NonPositive {
            field: "trace_every_steps",
        });
    }
    if manifest.max_conditions == 0 {
        return Err(SensitivityError::NonPositive {
            field: "max_conditions",
        });
    }
    if manifest.factors.is_empty() {
        return Err(SensitivityError::NoFactors);
    }
    let mut factor_ids = BTreeSet::new();
    let mut targets = BTreeMap::new();
    for factor in &manifest.factors {
        if !is_safe_factor_id(&factor.id) {
            return Err(SensitivityError::UnsafeFactorId {
                id: factor.id.clone(),
            });
        }
        if !factor_ids.insert(factor.id.as_str()) {
            return Err(SensitivityError::DuplicateFactorId {
                id: factor.id.clone(),
            });
        }
        if factor.subject.trim().is_empty() {
            return Err(SensitivityError::EmptyField {
                field: "factor.subject",
            });
        }
        if factor.rationale.trim().is_empty() {
            return Err(SensitivityError::EmptyField {
                field: "factor.rationale",
            });
        }
        if matches!(
            factor.basis,
            AssumptionBasis::DocumentedEstimate | AssumptionBasis::MeasuredInput
        ) && factor.references.is_empty()
        {
            return Err(SensitivityError::MissingReference {
                factor_id: factor.id.clone(),
                basis: assumption_basis_name(factor.basis),
            });
        }
        validate_factor_references(factor)?;
        if factor.values.len() < 2 {
            return Err(SensitivityError::InsufficientValues {
                factor_id: factor.id.clone(),
            });
        }
        let key = (factor.target, factor.subject.as_str());
        if let Some(first) = targets.insert(key, factor.id.as_str()) {
            return Err(SensitivityError::DuplicateTarget {
                first: first.to_owned(),
                second: factor.id.clone(),
            });
        }
        let mut values = factor.values.clone();
        values.sort_by(f64::total_cmp);
        for pair in values.windows(2) {
            if pair[0].total_cmp(&pair[1]).is_eq() {
                return Err(SensitivityError::DuplicateValue {
                    factor_id: factor.id.clone(),
                    value: pair[0],
                });
            }
        }
        for value in &factor.values {
            factor.target.validate_value(*value, &factor.id)?;
        }
    }
    Ok(())
}

fn assumption_basis_name(basis: AssumptionBasis) -> &'static str {
    match basis {
        AssumptionBasis::BestGuess => "best_guess",
        AssumptionBasis::DocumentedEstimate => "documented_estimate",
        AssumptionBasis::StructuralAssumption => "structural_assumption",
        AssumptionBasis::MeasuredInput => "measured_input",
    }
}

fn validate_factor_references(factor: &SensitivityFactor) -> Result<(), SensitivityError> {
    let mut ids = BTreeSet::new();
    for reference in &factor.references {
        let invalid = |message: &str| SensitivityError::InvalidReference {
            factor_id: factor.id.clone(),
            reference_id: reference.id.clone(),
            message: message.to_owned(),
        };
        if !is_safe_factor_id(&reference.id) {
            return Err(invalid("id must be a safe identifier"));
        }
        if !ids.insert(reference.id.as_str()) {
            return Err(SensitivityError::DuplicateReference {
                factor_id: factor.id.clone(),
                reference_id: reference.id.clone(),
            });
        }
        if reference.citation.trim().is_empty()
            || reference.applicability.trim().is_empty()
            || reference.limitation.trim().is_empty()
        {
            return Err(invalid(
                "citation, applicability, and limitation must not be empty",
            ));
        }
        if !reference.url.starts_with("https://") || reference.url.len() == "https://".len() {
            return Err(invalid("url must be a non-empty HTTPS URL"));
        }
        if let Some(hash) = &reference.source_sha256
            && (hash.len() != 64 || !hash.chars().all(|character| character.is_ascii_hexdigit()))
        {
            return Err(invalid(
                "source_sha256 must be a SHA-256 hexadecimal digest",
            ));
        }
        if let Some(report) = &reference.derived_report {
            if report.path.trim().is_empty() || Path::new(&report.path).is_absolute() {
                return Err(invalid(
                    "derived_report.path must be a non-empty relative path",
                ));
            }
            if report.sha256.len() != 64
                || !report
                    .sha256
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
            {
                return Err(invalid(
                    "derived_report.sha256 must be a SHA-256 hexadecimal digest",
                ));
            }
        }
    }
    Ok(())
}

fn is_safe_factor_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn baseline_values(
    manifest: &SensitivityManifest,
    baseline: &Scenario,
) -> Result<BTreeMap<String, f64>, SensitivityError> {
    manifest
        .factors
        .iter()
        .map(|factor| Ok((factor.id.clone(), factor_value(baseline, factor)?)))
        .collect()
}

fn factor_value_sets(manifest: &SensitivityManifest) -> Vec<(&SensitivityFactor, Vec<f64>)> {
    manifest
        .factors
        .iter()
        .map(|factor| (factor, factor.values.clone()))
        .collect()
}

fn planned_condition_count(
    design: SensitivityDesign,
    factors: &[(&SensitivityFactor, Vec<f64>)],
    baseline_values: &BTreeMap<String, f64>,
) -> u64 {
    match design {
        SensitivityDesign::OneAtATime => factors
            .iter()
            .map(|(factor, values)| {
                let baseline_value = baseline_values[&factor.id];
                u64::try_from(
                    values
                        .iter()
                        .filter(|value| value.total_cmp(&baseline_value).is_ne())
                        .count(),
                )
                .expect("usize fits u64")
            })
            .fold(0, u64::saturating_add),
        SensitivityDesign::FullFactorial => {
            let condition_count = factors
                .iter()
                .map(|(_, values)| u64::try_from(values.len()).expect("usize fits u64"))
                .try_fold(1_u64, u64::checked_mul)
                .unwrap_or(u64::MAX);
            let includes_baseline_combination = factors.iter().all(|(factor, values)| {
                let baseline_value = baseline_values[&factor.id];
                values
                    .iter()
                    .any(|value| value.total_cmp(&baseline_value).is_eq())
            });
            if includes_baseline_combination {
                condition_count.saturating_sub(1)
            } else {
                condition_count
            }
        }
    }
}

fn one_at_a_time_assignments(
    factors: &[(&SensitivityFactor, Vec<f64>)],
    baseline_values: &BTreeMap<String, f64>,
) -> Vec<BTreeMap<String, f64>> {
    let mut assignments = Vec::new();
    for (factor, values) in factors {
        let baseline_value = baseline_values[&factor.id];
        for value in values {
            if value.total_cmp(&baseline_value).is_ne() {
                assignments.push(BTreeMap::from([(factor.id.clone(), *value)]));
            }
        }
    }
    assignments
}

fn full_factorial_assignments(
    factors: &[(&SensitivityFactor, Vec<f64>)],
    baseline_values: &BTreeMap<String, f64>,
) -> Vec<BTreeMap<String, f64>> {
    let mut assignments = Vec::new();
    expand_factorial(factors, 0, BTreeMap::new(), &mut assignments);
    assignments.retain(|assignment| {
        assignment
            .iter()
            .any(|(factor_id, value)| value.total_cmp(&baseline_values[factor_id]).is_ne())
    });
    assignments
}

fn expand_factorial(
    factors: &[(&SensitivityFactor, Vec<f64>)],
    index: usize,
    current: BTreeMap<String, f64>,
    output: &mut Vec<BTreeMap<String, f64>>,
) {
    if index == factors.len() {
        output.push(current);
        return;
    }
    let (factor, values) = &factors[index];
    for value in values {
        let mut next = current.clone();
        next.insert(factor.id.clone(), *value);
        expand_factorial(factors, index + 1, next, output);
    }
}

fn factor_value(scenario: &Scenario, factor: &SensitivityFactor) -> Result<f64, SensitivityError> {
    match factor.target {
        SensitivityTarget::AgentCount => Ok(f64::from(agent(scenario, factor)?.count)),
        SensitivityTarget::AgentSpeedMps => Ok(agent(scenario, factor)?.speed_mps),
        SensitivityTarget::AgentRadiusM => Ok(agent(scenario, factor)?.radius_m),
        SensitivityTarget::AgentHeightM => Ok(agent(scenario, factor)?.height_m),
        SensitivityTarget::AgentReleaseAtS => Ok(agent(scenario, factor)?.release_at_s),
        SensitivityTarget::AgentReleaseIntervalS => {
            agent(scenario, factor)?.release_interval_s.ok_or_else(|| {
                unsupported(
                    factor,
                    "an agent group without an authored release interval",
                )
            })
        }
        SensitivityTarget::AgentReleaseBatchSize => {
            let group = agent(scenario, factor)?;
            if group.release_interval_s.is_none() {
                return Err(unsupported(
                    factor,
                    "an agent group without an authored release interval",
                ));
            }
            Ok(f64::from(group.release_batch_size.unwrap_or(1)))
        }
        SensitivityTarget::ExitCapacityPerS => exit(scenario, factor)?
            .capacity_per_s
            .ok_or_else(|| unsupported(factor, "an exit without an authored capacity")),
        SensitivityTarget::ConnectorCapacityPerS => connector(scenario, factor)?
            .service_rate_per_s()
            .ok_or_else(|| unsupported(factor, "a lift or connector without an authored capacity")),
        SensitivityTarget::EscalatorBeltSpeedMps => match connector(scenario, factor)? {
            Connector::Escalator { belt_speed_mps, .. } => Ok(*belt_speed_mps),
            _ => Err(unsupported(factor, "a non-escalator connector")),
        },
        SensitivityTarget::GateServiceRatePerS => Ok(gate(scenario, factor)?.service_rate_per_s),
        SensitivityTarget::MessageTrust => Ok(message(scenario, factor)?.trust),
        SensitivityTarget::MessageReachM => Ok(message(scenario, factor)?.reach_m),
        SensitivityTarget::CountermeasureTrust => Ok(countermeasure(scenario, factor)?.trust),
        SensitivityTarget::CountermeasureReachM => Ok(countermeasure(scenario, factor)?.reach_m),
    }
}

fn apply_factor(
    scenario: &mut Scenario,
    factor: &SensitivityFactor,
    value: f64,
) -> Result<(), SensitivityError> {
    match factor.target {
        SensitivityTarget::AgentCount => {
            agent_mut(scenario, factor)?.count = agent_count(value, factor)?;
        }
        SensitivityTarget::AgentSpeedMps => agent_mut(scenario, factor)?.speed_mps = value,
        SensitivityTarget::AgentRadiusM => agent_mut(scenario, factor)?.radius_m = value,
        SensitivityTarget::AgentHeightM => agent_mut(scenario, factor)?.height_m = value,
        SensitivityTarget::AgentReleaseAtS => agent_mut(scenario, factor)?.release_at_s = value,
        SensitivityTarget::AgentReleaseIntervalS => {
            agent_mut(scenario, factor)?.release_interval_s = Some(value);
        }
        SensitivityTarget::AgentReleaseBatchSize => {
            let batch_size = agent_count(value, factor)?;
            let group = agent_mut(scenario, factor)?;
            if group.release_interval_s.is_none() {
                return Err(unsupported(
                    factor,
                    "an agent group without an authored release interval",
                ));
            }
            group.release_batch_size = Some(batch_size);
        }
        SensitivityTarget::ExitCapacityPerS => {
            exit_mut(scenario, factor)?.capacity_per_s = Some(value);
        }
        SensitivityTarget::ConnectorCapacityPerS => match connector_mut(scenario, factor)? {
            Connector::Stair { capacity_per_s, .. }
            | Connector::Ramp { capacity_per_s, .. }
            | Connector::Escalator { capacity_per_s, .. } => *capacity_per_s = Some(value),
            Connector::Lift { .. } => return Err(unsupported(factor, "a lift connector")),
        },
        SensitivityTarget::EscalatorBeltSpeedMps => match connector_mut(scenario, factor)? {
            Connector::Escalator { belt_speed_mps, .. } => *belt_speed_mps = value,
            _ => return Err(unsupported(factor, "a non-escalator connector")),
        },
        SensitivityTarget::GateServiceRatePerS => {
            gate_mut(scenario, factor)?.service_rate_per_s = value;
        }
        SensitivityTarget::MessageTrust => message_mut(scenario, factor)?.trust = value,
        SensitivityTarget::MessageReachM => message_mut(scenario, factor)?.reach_m = value,
        SensitivityTarget::CountermeasureTrust => {
            countermeasure_mut(scenario, factor)?.trust = value;
        }
        SensitivityTarget::CountermeasureReachM => {
            countermeasure_mut(scenario, factor)?.reach_m = value;
        }
    }
    Ok(())
}

fn agent_count(value: f64, factor: &SensitivityFactor) -> Result<u32, SensitivityError> {
    factor.target.validate_value(value, &factor.id)?;
    value
        .to_string()
        .parse()
        .map_err(|_| SensitivityError::InvalidValue {
            factor_id: factor.id.clone(),
            message: "must be representable as a u32 agent count".to_owned(),
        })
}

fn agent<'a>(
    scenario: &'a Scenario,
    factor: &SensitivityFactor,
) -> Result<&'a crate::model::AgentGroup, SensitivityError> {
    scenario
        .agents
        .iter()
        .find(|item| item.id == factor.subject)
        .ok_or_else(|| unknown(factor, "agent group"))
}

fn agent_mut<'a>(
    scenario: &'a mut Scenario,
    factor: &SensitivityFactor,
) -> Result<&'a mut crate::model::AgentGroup, SensitivityError> {
    scenario
        .agents
        .iter_mut()
        .find(|item| item.id == factor.subject)
        .ok_or_else(|| unknown(factor, "agent group"))
}

fn exit<'a>(
    scenario: &'a Scenario,
    factor: &SensitivityFactor,
) -> Result<&'a crate::model::Exit, SensitivityError> {
    scenario
        .exits
        .iter()
        .find(|item| item.id == factor.subject)
        .ok_or_else(|| unknown(factor, "exit"))
}

fn exit_mut<'a>(
    scenario: &'a mut Scenario,
    factor: &SensitivityFactor,
) -> Result<&'a mut crate::model::Exit, SensitivityError> {
    scenario
        .exits
        .iter_mut()
        .find(|item| item.id == factor.subject)
        .ok_or_else(|| unknown(factor, "exit"))
}

fn connector<'a>(
    scenario: &'a Scenario,
    factor: &SensitivityFactor,
) -> Result<&'a Connector, SensitivityError> {
    scenario
        .connectors
        .iter()
        .find(|item| item.id() == factor.subject)
        .ok_or_else(|| unknown(factor, "connector"))
}

fn connector_mut<'a>(
    scenario: &'a mut Scenario,
    factor: &SensitivityFactor,
) -> Result<&'a mut Connector, SensitivityError> {
    scenario
        .connectors
        .iter_mut()
        .find(|item| item.id() == factor.subject)
        .ok_or_else(|| unknown(factor, "connector"))
}

fn gate<'a>(
    scenario: &'a Scenario,
    factor: &SensitivityFactor,
) -> Result<&'a crate::model::Gate, SensitivityError> {
    scenario
        .gates
        .iter()
        .find(|item| item.id == factor.subject)
        .ok_or_else(|| unknown(factor, "gate"))
}

fn gate_mut<'a>(
    scenario: &'a mut Scenario,
    factor: &SensitivityFactor,
) -> Result<&'a mut crate::model::Gate, SensitivityError> {
    scenario
        .gates
        .iter_mut()
        .find(|item| item.id == factor.subject)
        .ok_or_else(|| unknown(factor, "gate"))
}

fn message<'a>(
    scenario: &'a Scenario,
    factor: &SensitivityFactor,
) -> Result<&'a crate::model::Message, SensitivityError> {
    scenario
        .messages
        .iter()
        .find(|item| item.id == factor.subject)
        .ok_or_else(|| unknown(factor, "message"))
}

fn message_mut<'a>(
    scenario: &'a mut Scenario,
    factor: &SensitivityFactor,
) -> Result<&'a mut crate::model::Message, SensitivityError> {
    scenario
        .messages
        .iter_mut()
        .find(|item| item.id == factor.subject)
        .ok_or_else(|| unknown(factor, "message"))
}

fn countermeasure<'a>(
    scenario: &'a Scenario,
    factor: &SensitivityFactor,
) -> Result<&'a crate::model::Countermeasure, SensitivityError> {
    scenario
        .countermeasures
        .iter()
        .find(|item| item.id == factor.subject)
        .ok_or_else(|| unknown(factor, "countermeasure"))
}

fn countermeasure_mut<'a>(
    scenario: &'a mut Scenario,
    factor: &SensitivityFactor,
) -> Result<&'a mut crate::model::Countermeasure, SensitivityError> {
    scenario
        .countermeasures
        .iter_mut()
        .find(|item| item.id == factor.subject)
        .ok_or_else(|| unknown(factor, "countermeasure"))
}

fn unknown(factor: &SensitivityFactor, kind: &'static str) -> SensitivityError {
    SensitivityError::UnknownSubject {
        factor_id: factor.id.clone(),
        kind,
        subject: factor.subject.clone(),
    }
}

fn unsupported(factor: &SensitivityFactor, message: &str) -> SensitivityError {
    SensitivityError::UnsupportedSubject {
        factor_id: factor.id.clone(),
        message: message.to_owned(),
    }
}
