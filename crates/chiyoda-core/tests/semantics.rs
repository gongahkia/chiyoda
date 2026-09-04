use chiyoda_core::{
    BenchmarkManifest, RunOptions, benchmark::DatasetEvidence, benchmark::DatasetRole,
    benchmark::GeneratorRound, generator, parse, run, validate, validate_manifest,
};

#[test]
fn generated_source_is_parseable_and_valid() {
    let scenario = generator::scenario(73).expect("generator must preserve the language contract");
    assert_eq!(scenario.surfaces.len(), 2);
    assert_eq!(scenario.connectors.len(), 2);
    assert_eq!(scenario.messages.len(), 1);
    assert_eq!(scenario.countermeasures.len(), 1);
}

#[test]
fn runtime_is_reproducible_and_information_recomputes_routes() {
    let scenario = generator::scenario(73).expect("valid generated scenario");
    let first = run(
        scenario.clone(),
        RunOptions {
            trace_every_steps: 20,
        },
    )
    .expect("run one");
    let second = run(
        scenario,
        RunOptions {
            trace_every_steps: 20,
        },
    )
    .expect("run two");

    assert_eq!(first.bundle_hash, second.bundle_hash);
    assert!(first.verifies_hash());
    assert!(
        first
            .events
            .iter()
            .any(|event| event.kind == "route_recomputed")
    );
}

#[test]
fn lift_capacity_serializes_boarding() {
    let source = r#"
scenario "lift-capacity"
seed 1
duration 20s
timestep 1s
surface platform at (0m, 0m, 6m) size (10m, 10m)
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (9m, 1m, 0m) width 2m
lift lift_a from platform at (2m, 1m, 6m) to concourse at (2m, 1m, 0m) cabin 2m 2m capacity 1 cycle 4s
agents passengers count 2 on platform at (1m, 1m, 6m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("source validates");
    let bundle = run(
        scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    let boardings: Vec<_> = bundle
        .events
        .iter()
        .filter(|event| event.kind == "connector_boarding")
        .collect();
    assert_eq!(boardings.len(), 2);
    assert!(boardings[1].time_s > boardings[0].time_s);
    assert_eq!(bundle.metrics.queued_for_lift_agents, 1);
}

#[test]
fn validation_rejects_unreachable_agents() {
    let source = r#"
scenario "disconnected"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 6m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (9m, 1m, 0m) width 2m
agents passengers count 1 on upper at (1m, 1m, 6m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("topology must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("unreachable"))
    );
}

#[test]
fn benchmark_requires_open_calibration_and_holdout_evidence() {
    let digest = "a".repeat(64);
    let manifest = BenchmarkManifest {
        schema_version: "0.1".to_owned(),
        round_id: "alpha-1".to_owned(),
        generator: GeneratorRound {
            version: "0.1".to_owned(),
            public_fixture_seeds: vec![1, 2],
            evaluation_seed_commitment: digest.clone(),
            release_after_round: true,
        },
        datasets: vec![
            DatasetEvidence {
                id: "calibration".to_owned(),
                role: DatasetRole::Calibration,
                source_url: "https://example.invalid/calibration".to_owned(),
                license: "CC-BY-4.0".to_owned(),
                sha256: digest.clone(),
                redistributable: true,
                transformation: "documented projection".to_owned(),
            },
            DatasetEvidence {
                id: "holdout".to_owned(),
                role: DatasetRole::HeldOut,
                source_url: "https://example.invalid/holdout".to_owned(),
                license: "CC-BY-4.0".to_owned(),
                sha256: digest,
                redistributable: true,
                transformation: "documented projection".to_owned(),
            },
        ],
        claim_boundary: "Only the declared primitives and populations are supported.".to_owned(),
    };
    validate_manifest(&manifest).expect("public two-way evidence validates");
    let mut private = manifest;
    private.datasets[1].redistributable = false;
    assert!(validate_manifest(&private).is_err());
}
