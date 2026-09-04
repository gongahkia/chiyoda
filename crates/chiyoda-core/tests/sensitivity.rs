use chiyoda_core::{
    AssumptionBasis, SensitivityDerivedReport, SensitivityDesign, SensitivityFactor,
    SensitivityManifest, SensitivityReference, SensitivityTarget, parse, plan_sensitivity,
};

const SOURCE: &str = r#"
scenario "sensitivity-fixture"
seed 7
duration 20s
timestep 1s
surface concourse at (0m, 0m, 0m) size (20m, 10m)
exit street on concourse at (18m, 2m, 0m) width 2m capacity 2/s
agents passengers count 2 on concourse at (1m, 2m, 0m) to street speed 1m/s radius 0.3m height 1.7m release 1s every 2s
message notice source signage on concourse at (2m, 2m, 0m) claim exit street open truth true time 1s reach 5m trust 0.5
"#;

fn manifest(design: SensitivityDesign, factors: Vec<SensitivityFactor>) -> SensitivityManifest {
    SensitivityManifest {
        schema_version: "0.1".to_owned(),
        name: "fixture".to_owned(),
        description: "structural sensitivity fixture".to_owned(),
        baseline_source: "baseline.chy".to_owned(),
        first_seed: 10,
        count: 2,
        trace_every_steps: 1,
        design,
        max_conditions: 16,
        factors,
        claim_boundary: "This is an uncalibrated structural experiment.".to_owned(),
    }
}

fn factor(id: &str, target: SensitivityTarget, subject: &str, values: &[f64]) -> SensitivityFactor {
    SensitivityFactor {
        id: id.to_owned(),
        target,
        subject: subject.to_owned(),
        values: values.to_vec(),
        basis: AssumptionBasis::BestGuess,
        rationale: "disclosed best-guess alternatives".to_owned(),
        references: Vec::new(),
    }
}

fn same_number(left: f64, right: f64) -> bool {
    left.total_cmp(&right).is_eq()
}

#[test]
fn one_at_a_time_changes_one_declared_input_per_condition() {
    let baseline = parse(SOURCE).expect("fixture parses");
    let study = plan_sensitivity(
        &manifest(
            SensitivityDesign::OneAtATime,
            vec![
                factor(
                    "walking_speed",
                    SensitivityTarget::AgentSpeedMps,
                    "passengers",
                    &[1.0, 1.5],
                ),
                factor(
                    "exit_capacity",
                    SensitivityTarget::ExitCapacityPerS,
                    "street",
                    &[2.0, 3.0],
                ),
            ],
        ),
        &baseline,
    )
    .expect("declared alternatives plan");

    assert!(same_number(study.baseline_values["walking_speed"], 1.0));
    assert!(same_number(study.baseline_values["exit_capacity"], 2.0));
    assert_eq!(study.conditions.len(), 2);
    assert_eq!(study.conditions[0].factor_values.len(), 1);
    assert!(same_number(
        study.conditions[0].scenario.agents[0].speed_mps,
        1.5
    ));
    assert_eq!(study.conditions[1].factor_values.len(), 1);
    assert_eq!(
        study.conditions[1].scenario.exits[0].capacity_per_s,
        Some(3.0)
    );
    assert!(same_number(baseline.agents[0].speed_mps, 1.0));
    assert_eq!(baseline.exits[0].capacity_per_s, Some(2.0));
}

#[test]
fn full_factorial_includes_interactions_but_not_the_duplicate_baseline() {
    let baseline = parse(SOURCE).expect("fixture parses");
    let study = plan_sensitivity(
        &manifest(
            SensitivityDesign::FullFactorial,
            vec![
                factor(
                    "walking_speed",
                    SensitivityTarget::AgentSpeedMps,
                    "passengers",
                    &[1.0, 1.5],
                ),
                factor(
                    "message_trust",
                    SensitivityTarget::MessageTrust,
                    "notice",
                    &[0.5, 1.0],
                ),
            ],
        ),
        &baseline,
    )
    .expect("factorial alternatives plan");

    assert_eq!(study.conditions.len(), 3);
    assert!(study.conditions.iter().any(|condition| {
        same_number(condition.factor_values["walking_speed"], 1.5)
            && same_number(condition.factor_values["message_trust"], 1.0)
            && same_number(condition.scenario.agents[0].speed_mps, 1.5)
            && same_number(condition.scenario.messages[0].trust, 1.0)
    }));
    assert!(
        study
            .conditions
            .iter()
            .all(|condition| !condition.factor_values.is_empty())
    );
}

#[test]
fn planner_rejects_invalid_agent_count_alternatives_before_execution() {
    let baseline = parse(SOURCE).expect("fixture parses");
    let error = plan_sensitivity(
        &manifest(
            SensitivityDesign::OneAtATime,
            vec![factor(
                "passenger_count",
                SensitivityTarget::AgentCount,
                "passengers",
                &[2.0, 2.5],
            )],
        ),
        &baseline,
    )
    .expect_err("fractional people are invalid");

    assert!(error.to_string().contains("whole number"));
}

#[test]
fn planner_varies_an_authored_agent_release_interval() {
    let baseline = parse(SOURCE).expect("fixture parses");
    let study = plan_sensitivity(
        &manifest(
            SensitivityDesign::OneAtATime,
            vec![factor(
                "arrival_cadence",
                SensitivityTarget::AgentReleaseIntervalS,
                "passengers",
                &[2.0, 3.0],
            )],
        ),
        &baseline,
    )
    .expect("cadence alternatives plan");

    assert!(same_number(study.baseline_values["arrival_cadence"], 2.0));
    assert_eq!(study.conditions.len(), 1);
    assert_eq!(
        study.conditions[0].scenario.agents[0].release_interval_s,
        Some(3.0)
    );
}

#[test]
fn planner_varies_a_release_batch_from_the_default_of_one() {
    let baseline = parse(SOURCE).expect("fixture parses");
    let study = plan_sensitivity(
        &manifest(
            SensitivityDesign::OneAtATime,
            vec![factor(
                "arrival_pulse",
                SensitivityTarget::AgentReleaseBatchSize,
                "passengers",
                &[1.0, 2.0],
            )],
        ),
        &baseline,
    )
    .expect("batch alternatives plan");

    assert!(same_number(study.baseline_values["arrival_pulse"], 1.0));
    assert_eq!(study.conditions.len(), 1);
    assert_eq!(
        study.conditions[0].scenario.agents[0].release_batch_size,
        Some(2)
    );
}

#[test]
fn planner_varies_a_declared_gate_capacity_state() {
    let baseline = parse(
        r#"
scenario "capacity-state-sensitivity"
seed 7
duration 20s
timestep 1s
surface concourse at (0m, 0m, 0m) size (20m, 10m)
exit street on concourse at (18m, 2m, 0m) width 2m capacity 2/s
gate fare_gate on concourse at (16m, 2m, 0m) width 1m capacity 2/s to street
gate-capacity-state fare_gate_reduced gate fare_gate capacity 1/s time 10s
agents passengers count 2 on concourse at (1m, 2m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#,
    )
    .expect("fixture parses");
    let study = plan_sensitivity(
        &manifest(
            SensitivityDesign::OneAtATime,
            vec![factor(
                "fare_gate_reduced_capacity",
                SensitivityTarget::GateCapacityStatePerS,
                "fare_gate_reduced",
                &[1.0, 0.5],
            )],
        ),
        &baseline,
    )
    .expect("capacity-state alternatives plan");

    assert!(same_number(
        study.baseline_values["fare_gate_reduced_capacity"],
        1.0
    ));
    assert_eq!(study.conditions.len(), 1);
    assert!(same_number(
        study.conditions[0].scenario.gate_capacity_states[0].capacity_per_s,
        0.5
    ));
}

#[test]
fn planner_varies_declared_schedule_times() {
    let baseline = parse(
        r#"
scenario "state-time-sensitivity"
seed 7
duration 20s
timestep 1s
surface concourse at (0m, 0m, 0m) size (20m, 10m)
exit street on concourse at (18m, 2m, 0m) width 2m capacity 2/s
gate fare_gate on concourse at (16m, 2m, 0m) width 1m capacity 2/s to street
gate-state fare_gate_closed gate fare_gate closed time 15s
gate-capacity-state fare_gate_reduced gate fare_gate capacity 1/s time 10s
agents passengers count 2 on concourse at (1m, 2m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#,
    )
    .expect("fixture parses");
    let study = plan_sensitivity(
        &manifest(
            SensitivityDesign::OneAtATime,
            vec![
                factor(
                    "fare_gate_closure_time",
                    SensitivityTarget::GateStateAtS,
                    "fare_gate_closed",
                    &[15.0, 12.0],
                ),
                factor(
                    "fare_gate_reduced_time",
                    SensitivityTarget::GateCapacityStateAtS,
                    "fare_gate_reduced",
                    &[10.0, 5.0],
                ),
            ],
        ),
        &baseline,
    )
    .expect("schedule-time alternatives plan");

    assert_eq!(study.conditions.len(), 2);
    assert!(study.conditions.iter().any(|condition| {
        condition
            .factor_values
            .get("fare_gate_closure_time")
            .is_some_and(|value| same_number(*value, 12.0))
            && same_number(condition.scenario.gate_states[0].at_s, 12.0)
    }));
    assert!(study.conditions.iter().any(|condition| {
        condition
            .factor_values
            .get("fare_gate_reduced_time")
            .is_some_and(|value| same_number(*value, 5.0))
            && same_number(condition.scenario.gate_capacity_states[0].at_s, 5.0)
    }));
}

#[test]
fn planner_rejects_unbounded_capacity_as_a_numeric_baseline() {
    let baseline = parse(SOURCE.replace("capacity 2/s", "").as_str())
        .expect("fixture without capacity parses");
    let error = plan_sensitivity(
        &manifest(
            SensitivityDesign::OneAtATime,
            vec![factor(
                "exit_capacity",
                SensitivityTarget::ExitCapacityPerS,
                "street",
                &[1.0, 2.0],
            )],
        ),
        &baseline,
    )
    .expect_err("unbounded capacity does not provide a numeric baseline");

    assert!(error.to_string().contains("without an authored capacity"));
}

#[test]
fn planner_rejects_a_factorial_design_over_its_declared_condition_limit() {
    let baseline = parse(SOURCE).expect("fixture parses");
    let mut study_manifest = manifest(
        SensitivityDesign::FullFactorial,
        vec![
            factor(
                "walking_speed",
                SensitivityTarget::AgentSpeedMps,
                "passengers",
                &[1.0, 1.5],
            ),
            factor(
                "message_trust",
                SensitivityTarget::MessageTrust,
                "notice",
                &[0.5, 1.0],
            ),
        ],
    );
    study_manifest.max_conditions = 2;

    let error = plan_sensitivity(&study_manifest, &baseline)
        .expect_err("three non-baseline factorial conditions exceed the declared limit");

    assert!(error.to_string().contains("exceeding max_conditions 2"));
}

#[test]
fn factorial_limit_counts_every_condition_when_the_baseline_is_not_listed() {
    let baseline = parse(SOURCE).expect("fixture parses");
    let mut study_manifest = manifest(
        SensitivityDesign::FullFactorial,
        vec![
            factor(
                "walking_speed",
                SensitivityTarget::AgentSpeedMps,
                "passengers",
                &[1.25, 1.5],
            ),
            factor(
                "message_trust",
                SensitivityTarget::MessageTrust,
                "notice",
                &[0.75, 1.0],
            ),
        ],
    );
    study_manifest.max_conditions = 3;

    let error = plan_sensitivity(&study_manifest, &baseline)
        .expect_err("all four alternatives must count against the execution limit");

    assert!(error.to_string().contains("create 4 conditions"));
}

#[test]
fn documented_estimates_require_a_retained_source_and_limit() {
    let baseline = parse(SOURCE).expect("fixture parses");
    let mut documented = factor(
        "walking_speed",
        SensitivityTarget::AgentSpeedMps,
        "passengers",
        &[1.0, 1.5],
    );
    documented.basis = AssumptionBasis::DocumentedEstimate;
    let missing = plan_sensitivity(
        &manifest(SensitivityDesign::OneAtATime, vec![documented.clone()]),
        &baseline,
    )
    .expect_err("documented estimate without source must be rejected");
    assert!(
        missing
            .to_string()
            .contains("requires at least one reference")
    );

    documented.references.push(SensitivityReference {
        id: "source".to_owned(),
        citation: "Fixture source (2026)".to_owned(),
        url: "https://example.test/source".to_owned(),
        applicability: "illustrative alternative range".to_owned(),
        limitation: "not a runtime calibration".to_owned(),
        source_sha256: Some("a".repeat(64)),
        derived_report: Some(SensitivityDerivedReport {
            path: "report.json".to_owned(),
            sha256: "b".repeat(64),
        }),
    });
    plan_sensitivity(
        &manifest(SensitivityDesign::OneAtATime, vec![documented]),
        &baseline,
    )
    .expect("documented source is retained with the factor");
}
