use chiyoda_core::{
    BenchmarkManifest, EvidenceCatalog, RunBundle, RunOptions, benchmark::DatasetEvidence,
    benchmark::DatasetRole, benchmark::GeneratorRound, bundle_hash, format_scenario, generator,
    parse, run, validate, validate_catalog, validate_manifest,
};
use std::path::PathBuf;

#[test]
fn generated_source_is_parseable_and_valid() {
    let scenario = generator::scenario(73).expect("generator must preserve the language contract");
    assert_eq!(scenario.surfaces.len(), 2);
    assert_eq!(scenario.connectors.len(), 2);
    assert_eq!(scenario.exits.len(), 2);
    assert_eq!(scenario.exit_states.len(), 1);
    assert_eq!(
        scenario.agents[0].alternative_destinations,
        vec!["plaza".to_owned()]
    );
    assert_eq!(scenario.messages.len(), 1);
    assert_eq!(scenario.countermeasures.len(), 1);
}

#[test]
fn parser_rejects_trailing_tokens_and_duplicate_zero_release() {
    let trailing = parse("scenario \"unexpected\" trailing")
        .expect_err("fixed-arity declarations must reject trailing tokens");
    assert_eq!(trailing.line, 1);

    let duplicate_release =
        generator::source(73).replacen("release 0s", "release 0s release 0s", 1);
    let duplicate = parse(&duplicate_release).expect_err("duplicate release must be rejected");
    assert!(duplicate.message.contains("duplicate `release` clause"));

    let duplicate_alternative =
        generator::source(73).replacen("release 0s", "alternative street release 0s", 1);
    let duplicate = parse(&duplicate_alternative)
        .expect_err("an alternative cannot duplicate the primary exit");
    assert!(
        duplicate
            .message
            .contains("duplicate alternative exit `street`")
    );
}

#[test]
fn parser_rejects_trailing_waypoint_dwell_tokens() {
    let source = r#"
scenario "trailing-waypoint-option"
seed 1
duration 10s
timestep 1s
surface platform at (0m, 0m, 0m) size (10m, 10m)
waypoint hall on platform at (2m, 2m, 0m) dwell 1s trailing
"#;
    let error = parse(source).expect_err("waypoint options must consume the full declaration");
    assert!(error.message.contains("waypoint dwell"));
}

#[test]
fn formatter_round_trips_to_the_same_typed_scenario() {
    let scenario = generator::scenario(73).expect("valid generator output");
    let formatted = format_scenario(&scenario);
    let reparsed = parse(&formatted).expect("formatted source parses");
    assert_eq!(scenario, reparsed);
    assert_eq!(formatted, format_scenario(&reparsed));
}

#[test]
fn formatter_round_trips_ramps_and_escalators() {
    let source = r#"
scenario "vertical-connectors"
seed 1
duration 30s
timestep 1s
surface platform at (0m, 0m, 6m) size (10m, 10m)
surface mezzanine at (0m, 0m, 3m) size (10m, 10m)
surface concourse at (0m, 0m, 0m) size (10m, 10m)
obstacle column on platform at (6m, 6m, 6m) size (1m, 1m)
exit street on concourse at (9m, 1m, 0m) width 2m capacity 2/s
exit plaza on concourse at (8m, 1m, 0m) width 2m
ramp accessible_ramp from platform at (2m, 1m, 6m) to mezzanine at (2m, 1m, 3m) width 1.5m capacity 1.2/s clearance 1.8m
escalator down_escalator from mezzanine at (2m, 1m, 3m) to concourse at (2m, 1m, 0m) width 1m belt 0.6m/s clearance 1.9m capacity 0.8/s
lift accessible_lift from platform at (4m, 1m, 6m) to concourse at (4m, 1m, 0m) cabin 2m 2m capacity 4 cycle 5s clearance 2m
connector-state escalator_closure connector down_escalator closed time 10s
exit-state plaza_closure exit plaza closed time 15s
agents passengers count 1 on platform at (1m, 1m, 6m) to street speed 1m/s radius 0.3m height 1.7m alternative plaza exclude lift exclude stair
message plaza_notice source signage on concourse at (1m, 1m, 0m) claim exit plaza closed truth true time 15s reach 2m trust 1
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("source validates");
    let formatted = format_scenario(&scenario);
    assert_eq!(
        parse(&formatted).expect("formatted source parses"),
        scenario
    );
}

#[test]
fn capacity_state_changes_round_trip_log_and_limit_the_effective_rate() {
    let source = r#"
scenario "scheduled-capacity"
seed 1
duration 6s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (5m, 1m, 0m) width 2m capacity 2/s
exit side on concourse at (8m, 1m, 0m) width 2m capacity 1/s
stair down from upper at (2m, 1m, 3m) to concourse at (2m, 1m, 0m) width 2m capacity 1/s
connector-state down_closed connector down closed time 0s
connector-capacity-state down_slow connector down capacity 0.5/s time 0s
connector-capacity-state down_restore connector down capacity 1/s time 3s
exit-capacity-state street_slow exit street capacity 0.1/s time 2s
gate side_gate on concourse at (7m, 1m, 0m) width 1m capacity 1/s to side
gate-state side_gate_closed gate side_gate closed time 1s
gate-capacity-state side_gate_slow gate side_gate capacity 0.5/s time 1s
agents passengers count 3 on concourse at (1m, 1m, 0m) to street speed 10m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("capacity-state source validates");
    assert_eq!(scenario.connector_service_rate_at("down", 0.0), Some(0.5));
    assert_eq!(scenario.connector_service_rate_at("down", 3.0), Some(1.0));
    assert_eq!(scenario.exit_capacity_at("street", 1.0), Some(2.0));
    assert_eq!(scenario.exit_capacity_at("street", 2.0), Some(0.1));
    assert_eq!(scenario.gate_service_rate_at("side_gate", 0.0), Some(1.0));
    assert_eq!(scenario.gate_service_rate_at("side_gate", 1.0), Some(0.5));
    let formatted = format_scenario(&scenario);
    assert_eq!(
        parse(&formatted).expect("formatted source parses"),
        scenario
    );

    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    assert_eq!(bundle.metrics.evacuated_agents, 2);
    let initial_physical_events: Vec<_> = bundle
        .events
        .iter()
        .filter(|event| event.time_s == 0.0 && event.subject == "down")
        .map(|event| event.kind.as_str())
        .collect();
    assert_eq!(
        initial_physical_events,
        vec!["connector_state_changed", "connector_capacity_changed"]
    );
    let gate_events_at_one_second: Vec<_> = bundle
        .events
        .iter()
        .filter(|event| {
            (event.time_s - 1.0).abs() < 1e-9
                && matches!(
                    event.kind.as_str(),
                    "gate_state_changed" | "gate_capacity_changed"
                )
        })
        .map(|event| event.kind.as_str())
        .collect();
    assert_eq!(
        gate_events_at_one_second,
        vec!["gate_state_changed", "gate_capacity_changed"]
    );
    assert!(bundle.events.iter().any(|event| {
        event.kind == "connector_capacity_changed" && event.subject == "down" && event.time_s == 0.0
    }));
    assert!(bundle.events.iter().any(|event| {
        event.kind == "gate_capacity_changed"
            && event.subject == "side_gate"
            && (event.time_s - 1.0).abs() < 1e-9
    }));
    assert!(bundle.events.iter().any(|event| {
        event.kind == "exit_capacity_changed"
            && event.subject == "street"
            && (event.time_s - 2.0).abs() < 1e-9
    }));
    assert!(bundle.events.iter().any(|event| {
        event.kind == "connector_capacity_changed"
            && event.subject == "down"
            && (event.time_s - 3.0).abs() < 1e-9
    }));
}

#[test]
fn validation_rejects_capacity_changes_without_a_constrained_resource() {
    let source = r#"
scenario "invalid-capacity-state"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (5m, 1m, 0m) width 2m
stair down from upper at (2m, 1m, 3m) to concourse at (2m, 1m, 0m) width 2m
connector-capacity-state down_limit connector down capacity 1/s time 1s
exit-capacity-state street_limit exit street capacity 1/s time 1s
gate-capacity-state unknown_gate gate absent capacity 1/s time 1s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses before semantic validation");
    let errors = validate(&scenario).expect_err("invalid capacity-state resources are rejected");
    assert!(errors.iter().any(|error| {
        error.path == "connector_capacity_states[0].connector"
            && error.message.contains("authored baseline capacity")
    }));
    assert!(errors.iter().any(|error| {
        error.path == "exit_capacity_states[0].exit"
            && error.message.contains("authored baseline capacity")
    }));
    assert!(
        errors
            .iter()
            .any(|error| error.path == "gate_capacity_states[0].gate")
    );
}

#[test]
fn gate_closure_reroutes_to_an_open_alternative_exit() {
    let source = r#"
scenario "gate-reroute"
seed 1
duration 40s
timestep 1s
surface concourse at (0m, 0m, 0m) size (20m, 10m)
exit east on concourse at (18m, 1m, 0m) width 2m
exit west on concourse at (18m, 8m, 0m) width 2m
gate east_gate on concourse at (12m, 1m, 0m) width 1m capacity 2/s to east
gate west_gate on concourse at (12m, 8m, 0m) width 1m capacity 2/s to west
gate-state east_closure gate east_gate closed time 2s
agents passengers count 1 on concourse at (1m, 1m, 0m) to east speed 1m/s radius 0.3m height 1.7m alternative west
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("gate-state source validates");
    assert!(!scenario.gate_open_at("east_gate", 2.0));
    assert!(scenario.gate_open_at("west_gate", 2.0));
    assert_eq!(
        parse(&format_scenario(&scenario)).expect("formatted source parses"),
        scenario
    );

    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    assert_eq!(bundle.metrics.evacuated_by_exit.get("west"), Some(&1));
    assert!(bundle.events.iter().any(|event| {
        event.kind == "gate_state_changed"
            && event.subject == "east_gate"
            && (event.time_s - 2.0).abs() < 1e-9
    }));
    assert!(
        bundle
            .events
            .iter()
            .any(|event| { event.kind == "gate_processed" && event.detail == "west_gate" })
    );
    assert!(
        !bundle
            .events
            .iter()
            .any(|event| { event.kind == "gate_processed" && event.detail == "east_gate" })
    );
}

#[test]
fn an_initially_closed_gate_recovers_when_it_reopens() {
    let source = r#"
scenario "gate-recovery"
seed 1
duration 12s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (8m, 1m, 0m) width 2m
gate fare_gate on concourse at (5m, 1m, 0m) width 1m capacity 2/s to street
gate-state initially_closed gate fare_gate closed time 0s
gate-state reopened gate fare_gate open time 2s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("gate-state source validates");
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    let before_reopening = bundle
        .trace
        .iter()
        .find(|frame| (frame.time_s - 1.0).abs() < 1e-9)
        .expect("trace contains one-second frame");
    assert_eq!(
        before_reopening.agents[0].state,
        chiyoda_core::AgentState::WaitingForRoute
    );
    assert!(bundle.events.iter().any(|event| {
        event.kind == "gate_state_changed"
            && event.subject == "fare_gate"
            && (event.time_s - 2.0).abs() < 1e-9
    }));
    assert_eq!(bundle.metrics.evacuated_agents, 1);
}

#[test]
fn gate_closure_does_not_revoke_a_completed_gate_passage() {
    let source = r#"
scenario "passed-gate"
seed 1
duration 3s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (3m, 1m, 0m) width 2m
gate fare_gate on concourse at (2m, 1m, 0m) width 1m capacity 2/s to street
gate-state late_closure gate fare_gate closed time 2s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 10m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("gate-state source validates");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    assert!(bundle.events.iter().any(|event| {
        event.kind == "gate_processed" && event.detail == "fare_gate" && event.time_s < 2.0
    }));
    assert_eq!(bundle.metrics.evacuated_agents, 1);
}

#[test]
fn validation_rejects_a_gate_state_for_an_unknown_gate() {
    let source = r#"
scenario "unknown-gate-state"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (8m, 1m, 0m) width 2m
gate-state invalid gate absent closed time 1s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses before semantic validation");
    let errors = validate(&scenario).expect_err("unknown gate state must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "gate_states[0].gate")
    );
}

#[test]
fn formatter_preserves_high_precision_authored_values() {
    let source = r#"
scenario "high-precision-capacity"
seed 1
duration 10s
timestep 100ms
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (8m, 1m, 0m) width 1m capacity 1.1477832456710213/s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1.2m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let formatted = format_scenario(&scenario);

    assert!(formatted.contains("1.1477832456710213/s"));
    assert_eq!(
        parse(&formatted).expect("formatted source parses"),
        scenario
    );
}

#[test]
fn connector_exclusion_routes_agents_to_an_eligible_connector() {
    let source = r#"
scenario "connector-eligibility"
seed 1
duration 20s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair direct_stair from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m
ramp accessible_ramp from upper at (2m, 1m, 3m) to lower at (2m, 1m, 0m) width 2m
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m exclude stair
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("eligible route validates");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    assert!(
        bundle.events.iter().any(|event| {
            event.kind == "connector_boarding" && event.detail == "accessible_ramp"
        })
    );
}

#[test]
fn alternative_exit_is_selected_when_the_primary_route_is_closed() {
    let source = r#"
scenario "alternative-exit"
seed 1
duration 20s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit primary on lower at (1m, 1m, 0m) width 2m
exit fallback on upper at (5m, 1m, 3m) width 2m
stair down from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m
connector-state initial_closure connector down closed time 0s
agents passengers count 1 on upper at (1m, 1m, 3m) to primary speed 1m/s radius 0.3m height 1.7m alternative fallback
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("all declared final exits are statically reachable");
    let bundle = run(&scenario, RunOptions::default()).expect("fallback route runs");
    assert!(
        bundle
            .events
            .iter()
            .any(|event| event.kind == "evacuated" && event.detail == "fallback")
    );
}

#[test]
fn alternative_exit_is_reselected_when_a_connector_closes() {
    let source = r#"
scenario "alternative-exit-reroute"
seed 1
duration 20s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit primary on lower at (1m, 1m, 0m) width 2m
exit fallback on upper at (5m, 1m, 3m) width 2m
stair down from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m
connector-state closure connector down closed time 1s
agents passengers count 1 on upper at (1m, 1m, 3m) to primary speed 1m/s radius 0.3m height 1.7m alternative fallback
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("rerouted fallback runs");
    assert!(bundle.events.iter().any(|event| {
        event.kind == "connector_state_changed"
            && event.subject == "down"
            && (event.time_s - 1.0).abs() < 1e-9
    }));
    assert!(
        bundle
            .events
            .iter()
            .any(|event| event.kind == "evacuated" && event.detail == "fallback")
    );
}

#[test]
fn exit_closure_reroutes_an_agent_to_an_alternative_exit() {
    let source = r#"
scenario "exit-closure-reroute"
seed 1
duration 12s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit primary on concourse at (2m, 1m, 0m) width 2m
exit fallback on concourse at (6m, 1m, 0m) width 2m
exit-state closure exit primary closed time 1s
agents passengers count 1 on concourse at (1m, 1m, 0m) to primary speed 1m/s radius 0.3m height 1.7m alternative fallback
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("rerouted fallback runs");
    let encoded = serde_json::to_value(&bundle).expect("bundle serializes to JSON");
    assert!(encoded["scenario"]["scenario"].get("exit_states").is_some());
    let round_trip: RunBundle = serde_json::from_value(encoded).expect("bundle deserializes");
    assert!(round_trip.verifies_hash());
    assert!(bundle.events.iter().any(|event| {
        event.kind == "exit_state_changed"
            && event.subject == "primary"
            && event.detail == "closure: closed"
    }));
    assert_eq!(
        bundle.metrics.evacuated_by_exit.get("fallback"),
        Some(&1),
        "the metric must attribute the rerouted evacuation to its final exit"
    );
    assert!(
        bundle
            .events
            .iter()
            .any(|event| event.kind == "evacuated" && event.detail == "fallback")
    );
}

#[test]
fn accepted_exit_misinformation_reroutes_and_a_correction_restores_the_physical_exit() {
    let source = r#"
scenario "exit-misinformation"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit primary on concourse at (2m, 1m, 0m) width 2m
exit fallback on concourse at (8m, 1m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to primary speed 1m/s radius 0.3m height 1.7m alternative fallback
message false_primary_closure source signage on concourse at (1m, 1m, 0m) claim exit primary closed truth false time 1s reach 10m trust 1
countermeasure correction corrects false_primary_closure source staff on concourse at (1m, 1m, 0m) time 2s reach 10m trust 1
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    assert!(bundle.events.iter().any(|event| {
        event.kind == "message_received"
            && event.detail.contains("exit `primary` closed")
            && event.detail.contains("accepted=true")
    }));
    assert!(bundle.events.iter().any(|event| {
        event.kind == "route_recomputed"
            && event.subject == "passengers:0"
            && event.detail.contains("exit:primary")
    }));
    assert!(bundle.events.iter().any(|event| {
        event.kind == "countermeasure_received"
            && event.subject == "passengers:0"
            && event.detail.contains("accepted=true")
    }));
    let message_delivery = bundle
        .metrics
        .information_delivery
        .get("false_primary_closure")
        .expect("message delivery is recorded");
    assert_eq!(message_delivery.received_agents, 1);
    assert_eq!(message_delivery.accepted_agents, 1);
    let correction_delivery = bundle
        .metrics
        .information_delivery
        .get("correction")
        .expect("countermeasure delivery is recorded");
    assert_eq!(correction_delivery.received_agents, 1);
    assert_eq!(correction_delivery.accepted_agents, 1);
    let mut legacy_value = serde_json::to_value(&bundle).expect("bundle serializes");
    legacy_value["metrics"]
        .as_object_mut()
        .expect("metrics is an object")
        .remove("information_delivery");
    legacy_value["bundle_hash"] = serde_json::Value::String(String::new());
    let legacy_unsigned: RunBundle =
        serde_json::from_value(legacy_value.clone()).expect("legacy bundle deserializes");
    legacy_value["bundle_hash"] = serde_json::Value::String(bundle_hash(&legacy_unsigned));
    let legacy: RunBundle = serde_json::from_value(legacy_value).expect("legacy bundle reloads");
    assert!(legacy.metrics.information_delivery.is_empty());
    assert!(legacy.verifies_hash());
    assert!(
        bundle
            .events
            .iter()
            .any(|event| { event.kind == "evacuated" && event.detail == "primary" })
    );
}

#[test]
fn an_exit_closure_during_transit_reroutes_on_arrival() {
    let source = r#"
scenario "transit-exit-reroute"
seed 1
duration 12s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit primary on lower at (4m, 1m, 0m) width 2m
exit fallback on lower at (8m, 1m, 0m) width 2m
stair down from upper at (2m, 1m, 3m) to lower at (2m, 1m, 0m) width 2m
exit-state primary_closure exit primary closed time 2s
agents passengers count 1 on upper at (2m, 1m, 3m) to primary speed 1m/s radius 0.3m height 1.7m alternative fallback
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    assert!(bundle.events.iter().any(|event| {
        event.kind == "route_recomputed"
            && event.subject == "passengers:0"
            && event.detail.contains("exit:primary")
    }));
    assert!(
        bundle
            .events
            .iter()
            .any(|event| { event.kind == "evacuated" && event.detail == "fallback" })
    );
}

#[test]
fn exit_metrics_attribute_each_completed_evacuation_to_its_final_exit() {
    let source = r#"
scenario "exit-metrics"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (20m, 10m)
exit west on concourse at (4m, 1m, 0m) width 2m
exit east on concourse at (16m, 1m, 0m) width 2m
agents westbound count 1 on concourse at (1m, 1m, 0m) to west speed 1m/s radius 0.3m height 1.7m
agents eastbound count 1 on concourse at (19m, 1m, 0m) to east speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("both groups evacuate");
    assert_eq!(bundle.metrics.evacuated_agents, 2);
    assert_eq!(bundle.metrics.evacuated_by_exit.get("west"), Some(&1));
    assert_eq!(bundle.metrics.evacuated_by_exit.get("east"), Some(&1));
    assert_eq!(
        bundle.metrics.clearance_time_s,
        bundle.metrics.last_exit_time_s
    );
    assert_eq!(
        bundle.metrics.evacuated_by_exit.values().sum::<u32>(),
        bundle.metrics.evacuated_agents
    );
}

#[test]
fn final_state_metrics_explain_agents_remaining_at_the_time_limit() {
    let source = r#"
scenario "remaining-state-metrics"
seed 1
duration 2s
timestep 1s
surface concourse at (0m, 0m, 0m) size (12m, 10m)
exit street on concourse at (10m, 1m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    assert_eq!(bundle.metrics.evacuated_agents, 0);
    assert_eq!(bundle.metrics.remaining_by_state.get("moving"), Some(&1));
    assert_eq!(bundle.metrics.clearance_time_s, None);
    assert_eq!(bundle.metrics.last_exit_time_s, None);
    let mut legacy_value = serde_json::to_value(&bundle).expect("bundle serializes");
    legacy_value["metrics"]
        .as_object_mut()
        .expect("metrics is an object")
        .remove("remaining_by_state");
    legacy_value["metrics"]
        .as_object_mut()
        .expect("metrics is an object")
        .remove("last_exit_time_s");
    legacy_value["bundle_hash"] = serde_json::Value::String(String::new());
    let legacy_unsigned: RunBundle =
        serde_json::from_value(legacy_value.clone()).expect("legacy bundle deserializes");
    legacy_value["bundle_hash"] = serde_json::Value::String(bundle_hash(&legacy_unsigned));
    let legacy: RunBundle = serde_json::from_value(legacy_value).expect("legacy bundle reloads");
    assert!(legacy.metrics.remaining_by_state.is_empty());
    assert_eq!(legacy.metrics.last_exit_time_s, None);
    assert!(legacy.verifies_hash());
}

#[test]
fn partial_evacuation_records_last_exit_but_not_system_clearance() {
    let source = r#"
scenario "partial-evacuation-metrics"
seed 1
duration 3s
timestep 1s
surface concourse at (0m, 0m, 0m) size (14m, 10m)
exit street on concourse at (4m, 1m, 0m) width 2m
agents quick count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
agents slow count 1 on concourse at (12m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");

    assert_eq!(bundle.metrics.total_agents, 2);
    assert_eq!(bundle.metrics.evacuated_agents, 1);
    assert_eq!(bundle.metrics.clearance_time_s, None);
    assert_eq!(bundle.metrics.last_exit_time_s, Some(3.0));
}

#[test]
fn exit_reopening_recovers_agents_waiting_for_a_route() {
    let source = r#"
scenario "exit-reopening"
seed 1
duration 12s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (4m, 1m, 0m) width 2m
exit-state initially_closed exit street closed time 0s
exit-state reopen exit street open time 3s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("waiting route recovers");
    assert!(bundle.trace.iter().any(|frame| {
        frame
            .agents
            .iter()
            .any(|agent| agent.state == chiyoda_core::AgentState::WaitingForRoute)
    }));
    assert!(bundle.events.iter().any(|event| {
        event.kind == "exit_state_changed"
            && event.subject == "street"
            && event.detail == "reopen: open"
    }));
    assert!(
        bundle
            .events
            .iter()
            .any(|event| event.kind == "evacuated" && event.detail == "street")
    );
}

#[test]
fn alternative_exit_selection_prices_the_gate_path_to_a_final_exit() {
    let source = r#"
scenario "gate-aware-exit-selection"
seed 1
duration 8s
timestep 1s
surface concourse at (0m, 0m, 0m) size (12m, 10m)
exit primary on concourse at (2m, 1m, 0m) width 2m
exit fallback on concourse at (5m, 1m, 0m) width 2m
gate distant_gate on concourse at (9m, 1m, 0m) width 2m capacity 10/s to primary
agents passengers count 1 on concourse at (1m, 1m, 0m) to primary speed 1m/s radius 0.3m height 1.7m alternative fallback
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    assert!(
        bundle
            .events
            .iter()
            .any(|event| event.kind == "evacuated" && event.detail == "fallback")
    );
}

#[test]
fn alternative_exit_ties_respect_declaration_order() {
    let source = r#"
scenario "alternative-exit-tie"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit first on concourse at (5m, 1m, 0m) width 2m
exit second on concourse at (5m, 1m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to first speed 1m/s radius 0.3m height 1.7m alternative second
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    assert!(
        bundle
            .events
            .iter()
            .any(|event| event.kind == "evacuated" && event.detail == "first")
    );
}

#[test]
fn validation_rejects_an_unknown_alternative_exit() {
    let source = r#"
scenario "unknown-alternative-exit"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (5m, 1m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m alternative missing
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("unknown alternative exits must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "agents[0].alternative_destinations[0]")
    );
}

#[test]
fn validation_rejects_an_exit_state_for_an_unknown_exit() {
    let source = r#"
scenario "unknown-exit-state"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (5m, 1m, 0m) width 2m
exit-state closure exit missing closed time 1s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("unknown exit states must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "exit_states[0].exit")
    );
}

#[test]
fn validation_rejects_a_route_when_all_connector_classes_are_excluded() {
    let source = r#"
scenario "ineligible-route"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair only_route from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m exclude stair
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("ineligible route must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "agents[0].destination")
    );
}

#[test]
fn runtime_is_reproducible_and_information_recomputes_routes() {
    let mut scenario = generator::scenario(73).expect("valid generated scenario");
    scenario.exit_states.clear();
    let first = run(
        &scenario,
        RunOptions {
            trace_every_steps: 20,
        },
    )
    .expect("run one");
    let second = run(
        &scenario,
        RunOptions {
            trace_every_steps: 20,
        },
    )
    .expect("run two");

    assert_eq!(first.bundle_hash, second.bundle_hash);
    assert!(first.verifies_hash());
    let encoded = serde_json::to_value(&first).expect("bundle serializes to JSON");
    assert!(
        encoded["scenario"]["scenario"].get("exit_states").is_none(),
        "an empty state collection must preserve the existing bundle encoding"
    );
    let serialized = serde_json::to_string(&first).expect("bundle serializes");
    let round_trip: RunBundle = serde_json::from_str(&serialized).expect("bundle deserializes");
    assert!(round_trip.verifies_hash());
    assert!(
        first
            .events
            .iter()
            .any(|event| event.kind == "route_recomputed")
    );
}

#[test]
fn route_selection_minimizes_nominal_travel_time_not_connector_hops() {
    let source = r#"
scenario "time-weighted-routing"
seed 1
duration 30s
timestep 1s
surface upper at (0m, 0m, 6m) size (10m, 10m)
surface mezzanine at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
lift slow_lift from upper at (1m, 1m, 6m) to lower at (1m, 1m, 0m) cabin 2m 2m capacity 4 cycle 20s
stair upper_stair from upper at (1m, 1m, 6m) to mezzanine at (1m, 1m, 3m) width 2m
stair lower_stair from mezzanine at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m
agents passengers count 1 on upper at (1m, 1m, 6m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    let boardings: Vec<_> = bundle
        .events
        .iter()
        .filter(|event| event.kind == "connector_boarding")
        .map(|event| event.detail.as_str())
        .collect();
    assert_eq!(boardings, ["upper_stair", "lower_stair"]);
}

#[test]
fn escalator_belt_speed_is_used_by_nominal_route_selection() {
    let source = r#"
scenario "escalator-routing"
seed 1
duration 20s
timestep 1s
surface upper at (0m, 0m, 6m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair down_stair from upper at (1m, 1m, 6m) to lower at (1m, 1m, 0m) width 2m
escalator down_escalator from upper at (1m, 1m, 6m) to lower at (1m, 1m, 0m) width 1m belt 1m/s
agents passengers count 1 on upper at (1m, 1m, 6m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    assert!(
        bundle.events.iter().any(|event| {
            event.kind == "connector_boarding" && event.detail == "down_escalator"
        })
    );
}

#[test]
fn validation_rejects_non_positive_escalator_belt_speed() {
    let source = r#"
scenario "invalid-escalator"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 6m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
escalator down from upper at (1m, 1m, 6m) to lower at (1m, 1m, 0m) width 1m belt 0m/s
agents passengers count 1 on upper at (1m, 1m, 6m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("zero belt speed must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "connectors[0].belt_speed_mps")
    );
}

#[test]
fn connector_clearance_routes_taller_agents_to_an_accessible_connector() {
    let source = r#"
scenario "height-aware-routing"
seed 1
duration 20s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair low_primary from upper at (3m, 1m, 3m) to lower at (3m, 1m, 0m) width 2m clearance 1.5m
ramp accessible_backup from upper at (1m, 3m, 3m) to lower at (1m, 3m, 0m) width 2m clearance 2.1m
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("height-aware route validates");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    assert!(bundle.events.iter().any(|event| {
        event.kind == "connector_boarding" && event.detail == "accessible_backup"
    }));
    assert!(
        !bundle
            .events
            .iter()
            .any(|event| event.kind == "connector_boarding" && event.detail == "low_primary")
    );
}

#[test]
fn validation_rejects_a_route_without_sufficient_connector_clearance() {
    let source = r#"
scenario "height-blocked"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair low_primary from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m clearance 1.5m
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("height-blocked route must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "agents[0].destination")
    );
}

#[test]
fn validation_rejects_non_positive_connector_clearance() {
    let source = r#"
scenario "invalid-clearance"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair primary from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m clearance 0m
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("zero clearance must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "connectors[0].clearance_height_m")
    );
}

#[test]
fn connector_state_change_closes_a_route_even_when_a_message_is_untrusted() {
    let source = r#"
scenario "physical-closure"
seed 1
duration 20s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair primary from upper at (3m, 1m, 3m) to lower at (3m, 1m, 0m) width 2m
stair backup from upper at (1m, 3m, 3m) to lower at (1m, 3m, 0m) width 2m
connector-state primary_closure connector primary closed time 1s
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m
message closure source signage on upper at (1m, 1m, 3m) claim connector primary closed truth true time 1s reach 2m trust 0
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("truthful closure validates");
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    assert!(bundle.events.iter().any(|event| {
        event.kind == "connector_state_changed"
            && event.subject == "primary"
            && event.detail == "primary_closure: closed"
    }));
    assert!(
        bundle
            .events
            .iter()
            .any(|event| event.kind == "connector_boarding" && event.detail == "backup")
    );
    assert!(
        !bundle
            .events
            .iter()
            .any(|event| event.kind == "connector_boarding" && event.detail == "primary")
    );
}

#[test]
fn physical_reopening_recovers_an_agent_waiting_for_a_route() {
    let source = r#"
scenario "closure-recovery"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair primary from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m
connector-state primary_closed connector primary closed time 1s
connector-state primary_opened connector primary open time 3s
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    assert_eq!(
        bundle.trace[1].agents[0].state,
        chiyoda_core::AgentState::WaitingForRoute
    );
    let boarding = bundle
        .events
        .iter()
        .find(|event| event.kind == "connector_boarding")
        .expect("agent boards after the reopening");
    assert_eq!(boarding.detail, "primary");
    assert!((boarding.time_s - 3.0).abs() < 1e-9);
}

#[test]
fn initial_physical_closure_waits_for_a_scheduled_reopening() {
    let source = r#"
scenario "initial-closure-recovery"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair primary from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m
connector-state primary_opened connector primary open time 3s
connector-state primary_initially_closed connector primary closed time 0s
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("future reopening makes the run executable");
    assert_eq!(
        bundle.trace[0].agents[0].state,
        chiyoda_core::AgentState::WaitingForRoute
    );
    assert!(bundle.events.iter().any(|event| {
        event.kind == "connector_state_changed"
            && event.time_s == 0.0
            && event.detail == "primary_initially_closed: closed"
    }));
    assert!(
        bundle.events.iter().any(|event| {
            event.kind == "connector_boarding" && (event.time_s - 3.0).abs() < 1e-9
        })
    );
}

#[test]
fn validation_rejects_a_truthful_message_that_disagrees_with_physical_state() {
    let source = r#"
scenario "invalid-message-truth"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair primary from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m
message false_truth source signage on upper at (1m, 1m, 3m) claim connector primary closed truth true time 1s reach 2m trust 1
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("incorrect truth label must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "messages[0].truthful")
    );
}

#[test]
fn validation_checks_exit_message_truth_and_references() {
    let source = r#"
scenario "invalid-exit-message"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (5m, 1m, 0m) width 2m
exit-state street_closed exit street closed time 1s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
message incorrect_truth source signage on concourse at (1m, 1m, 0m) claim exit street open truth true time 1s reach 2m trust 1
message unknown_exit source signage on concourse at (1m, 1m, 0m) claim exit missing closed truth false time 1s reach 2m trust 1
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("invalid exit claims must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "messages[0].truthful")
    );
    assert!(errors.iter().any(|error| {
        error.path == "messages[1].claim" && error.message.contains("unknown exit `missing`")
    }));
}

#[test]
fn validation_rejects_a_countermeasure_that_precedes_its_message() {
    let source = r#"
scenario "premature-countermeasure"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair primary from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m
message false_closure source signage on upper at (1m, 1m, 3m) claim connector primary closed truth false time 2s reach 2m trust 1
countermeasure correction corrects false_closure source staff on upper at (1m, 1m, 3m) time 1s reach 2m trust 1
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("a correction cannot precede its message");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "countermeasures[0].at_s")
    );
}

#[test]
fn parser_rejects_a_peer_countermeasure() {
    let source = r#"
scenario "peer-countermeasure"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair primary from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m
message false_closure source signage on upper at (1m, 1m, 3m) claim connector primary closed truth false time 1s reach 2m trust 1
countermeasure correction corrects false_closure source peer on upper at (1m, 1m, 3m) time 2s reach 2m trust 1
"#;
    let error = parse(source).expect_err("peer countermeasure must be rejected");
    assert!(error.message.contains("countermeasure source"));
}

#[test]
fn rectangular_obstacles_are_never_crossed_by_on_surface_motion() {
    let source = r#"
scenario "obstacle-routing"
seed 1
duration 30s
timestep 100ms
surface concourse at (0m, 0m, 0m) size (10m, 10m)
obstacle kiosk on concourse at (4m, 3m, 0m) size (2m, 4m)
exit street on concourse at (9m, 5m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 5m, 0m) to street speed 1m/s radius 0.2m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("source validates");
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    assert_eq!(bundle.metrics.evacuated_agents, 1);
    let obstacle = &scenario.obstacles[0];
    assert!(bundle.trace.iter().all(|frame| {
        frame.agents.iter().all(|agent| {
            !obstacle.contains(chiyoda_core::model::Point3 {
                x_m: agent.x_m,
                y_m: agent.y_m,
                z_m: agent.z_m,
            })
        })
    }));
}

#[test]
fn route_selection_includes_obstacle_aware_walking_time() {
    let source = r#"
scenario "obstacle-aware-routing"
seed 1
duration 40s
timestep 1s
surface upper at (0m, 0m, 6m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
obstacle wall on upper at (4m, 0m, 6m) size (2m, 7m)
exit street on lower at (1m, 8m, 0m) width 2m
stair blocked_near from upper at (7m, 1m, 6m) to lower at (7m, 1m, 0m) width 2m
stair clear_far from upper at (1m, 8m, 6m) to lower at (1m, 8m, 0m) width 2m
agents passengers count 1 on upper at (1m, 1m, 6m) to street speed 1m/s radius 0.2m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    assert!(
        bundle
            .events
            .iter()
            .any(|event| { event.kind == "connector_boarding" && event.detail == "clear_far" })
    );
    assert!(
        !bundle
            .events
            .iter()
            .any(|event| { event.kind == "connector_boarding" && event.detail == "blocked_near" })
    );
}

#[test]
fn validation_rejects_exit_inside_an_obstacle() {
    let source = r#"
scenario "blocked-exit"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
obstacle kiosk on concourse at (4m, 3m, 0m) size (2m, 4m)
exit street on concourse at (5m, 5m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("an occupied exit must be rejected");
    assert!(errors.iter().any(|error| error.path == "exits[0].at"));
}

#[test]
fn validation_rejects_an_intermediate_spawn_inside_an_obstacle() {
    let source = r#"
scenario "blocked-spawn"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
obstacle kiosk on concourse at (1.6m, 0.9m, 0m) size (0.1m, 0.2m)
exit street on concourse at (9m, 1m, 0m) width 2m
agents passengers count 3 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("intermediate spawn must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "agents[0].spawn[1]")
    );
}

#[test]
fn validation_rejects_spawn_without_navigation_clearance() {
    let source = r#"
scenario "spawn-clearance"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
obstacle kiosk on concourse at (1.2m, 0.9m, 0m) size (0.1m, 0.2m)
exit street on concourse at (9m, 1m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("spawn clearance must be rejected");
    assert!(errors.iter().any(|error| {
        error.path == "agents[0].at" && error.message.contains("does not clear obstacle")
    }));
}

#[test]
fn released_agents_become_active_after_their_declared_time() {
    let source = r#"
scenario "scheduled-release"
seed 1
duration 8s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (3m, 1m, 0m) width 2m
stair down from upper at (3m, 1m, 3m) to lower at (3m, 1m, 0m) width 2m
agents delayed count 1 on upper at (0m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m release 2s
message before_release source signage on upper at (0m, 1m, 3m) claim connector down closed truth false time 1s reach 2m trust 1
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    assert!((bundle.trace[1].time_s - 1.0).abs() < 1e-9);
    assert_eq!(
        bundle.trace[1].agents[0].state,
        chiyoda_core::AgentState::WaitingToDepart
    );
    assert!((bundle.trace[2].time_s - 2.0).abs() < 1e-9);
    assert_eq!(
        bundle.trace[2].agents[0].state,
        chiyoda_core::AgentState::Moving
    );
    assert!(
        bundle
            .events
            .iter()
            .any(|event| event.kind == "agent_released" && (event.time_s - 2.0).abs() < 1e-9)
    );
    assert!(
        !bundle
            .events
            .iter()
            .any(|event| event.kind == "message_received")
    );
    assert_eq!(bundle.metrics.evacuated_agents, 1);
}

#[test]
fn release_cadence_staggers_agents_deterministically_and_round_trips() {
    let source = r#"
scenario "scheduled-cadence"
seed 1
duration 6s
timestep 1s
surface concourse at (0m, 0m, 0m) size (20m, 10m)
exit street on concourse at (18m, 1m, 0m) width 2m
agents arrivals count 3 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m release 1s every 2s
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("cadence source validates");
    assert_eq!(scenario.agents[0].release_interval_s, Some(2.0));
    let formatted = format_scenario(&scenario);
    assert!(formatted.contains("release 1s every 2s"));
    assert_eq!(
        parse(&formatted).expect("formatted source parses"),
        scenario
    );

    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    let release_times: Vec<_> = bundle
        .events
        .iter()
        .filter(|event| event.kind == "agent_released")
        .map(|event| event.time_s)
        .collect();
    assert_eq!(release_times, vec![1.0, 3.0, 5.0]);
    let at_two_seconds = bundle
        .trace
        .iter()
        .find(|frame| (frame.time_s - 2.0).abs() < 1e-9)
        .expect("trace contains two-second frame");
    assert_eq!(
        at_two_seconds
            .agents
            .iter()
            .filter(|agent| agent.state == chiyoda_core::AgentState::WaitingToDepart)
            .count(),
        2
    );
}

#[test]
fn release_batches_activate_multiple_agents_per_cadence_instant() {
    let source = r#"
scenario "scheduled-pulses"
seed 1
duration 6s
timestep 1s
surface concourse at (0m, 0m, 0m) size (20m, 10m)
exit street on concourse at (18m, 1m, 0m) width 2m
agents arrivals count 5 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m release 1s every 2s batch 2
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("batched cadence source validates");
    assert_eq!(scenario.agents[0].release_batch_size, Some(2));
    let formatted = format_scenario(&scenario);
    assert!(formatted.contains("release 1s every 2s batch 2"));
    assert_eq!(
        parse(&formatted).expect("formatted source parses"),
        scenario
    );

    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    let release_times: Vec<_> = bundle
        .events
        .iter()
        .filter(|event| event.kind == "agent_released")
        .map(|event| event.time_s)
        .collect();
    assert_eq!(release_times, vec![1.0, 1.0, 3.0, 3.0, 5.0]);
}

#[test]
fn validation_rejects_a_zero_release_batch_and_parser_rejects_an_unpaired_batch() {
    let source = r#"
scenario "invalid-pulse"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (9m, 1m, 0m) width 2m
agents delayed count 3 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m release 1s every 2s batch 0
"#;
    let scenario = parse(source).expect("source parses before semantic validation");
    let errors = validate(&scenario).expect_err("zero batch must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "agents[0].release_batch_size")
    );

    let parse_error = parse(source.replace("every 2s batch 0", "batch 2").as_str())
        .expect_err("batch without cadence must not parse");
    assert!(
        parse_error
            .message
            .contains("requires `release DURATION every DURATION`")
    );
}

#[test]
fn validation_rejects_agent_release_after_the_scenario() {
    let source = r#"
scenario "late-release"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (9m, 1m, 0m) width 2m
agents delayed count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m release 11s
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("release outside duration must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "agents[0].release_at_s")
    );
}

#[test]
fn validation_rejects_a_release_cadence_that_extends_past_duration() {
    let source = r#"
scenario "late-cadence"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (9m, 1m, 0m) width 2m
agents delayed count 3 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m release 7s every 2s
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("final release outside duration must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "agents[0].final_release_at_s")
    );
}

#[test]
fn authored_connector_capacity_creates_a_deterministic_queue() {
    let source = r#"
scenario "stair-capacity"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair down from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m capacity 0.5/s
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    let boarding = bundle
        .events
        .iter()
        .find(|event| event.kind == "connector_boarding")
        .expect("agent boards once a token is accrued");
    assert!((boarding.time_s - 2.0).abs() < 1e-9);
    assert_eq!(
        bundle.trace[1].agents[0].state,
        chiyoda_core::AgentState::WaitingForConnector
    );
    assert_eq!(bundle.metrics.queued_for_connector_agents, 1);
}

#[test]
fn validation_rejects_non_positive_connector_capacity() {
    let source = r#"
scenario "invalid-stair-capacity"
seed 1
duration 10s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair down from upper at (1m, 1m, 3m) to lower at (1m, 1m, 0m) width 2m capacity 0/s
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("zero connector capacity must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "connectors[0].capacity_per_s")
    );
}

#[test]
fn authored_exit_capacity_creates_a_deterministic_queue() {
    let source = r#"
scenario "exit-capacity"
seed 1
duration 4s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (1m, 1m, 0m) width 2m capacity 0.5/s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    let evacuation = bundle
        .events
        .iter()
        .find(|event| event.kind == "evacuated")
        .expect("agent evacuates once a token is accrued");
    assert!((evacuation.time_s - 2.0).abs() < 1e-9);
    assert_eq!(
        bundle.trace[1].agents[0].state,
        chiyoda_core::AgentState::WaitingForExit
    );
    assert_eq!(bundle.metrics.queued_for_exit_agents, 1);
}

#[test]
fn validation_rejects_non_positive_exit_capacity() {
    let source = r#"
scenario "invalid-exit-capacity"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (9m, 1m, 0m) width 2m capacity 0/s
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("zero exit capacity must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.path == "exits[0].capacity_per_s")
    );
}

#[test]
fn information_trust_probability_respects_zero_and_one_extremes() {
    let source = r#"
scenario "trust-extremes"
seed 1
duration 20s
timestep 1s
surface upper at (0m, 0m, 3m) size (10m, 10m)
surface lower at (0m, 0m, 0m) size (10m, 10m)
exit street on lower at (1m, 1m, 0m) width 2m
stair primary from upper at (3m, 1m, 3m) to lower at (3m, 1m, 0m) width 2m
stair backup from upper at (1m, 3m, 3m) to lower at (1m, 3m, 0m) width 2m
agents passengers count 1 on upper at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m
message closure source signage on upper at (1m, 1m, 3m) claim connector primary closed truth false time 1s reach 2m trust 0
"#;
    let untrusted = run(
        &parse(source).expect("source parses"),
        RunOptions::default(),
    )
    .expect("run succeeds");
    assert!(
        untrusted
            .events
            .iter()
            .any(|event| { event.kind == "connector_boarding" && event.detail == "primary" })
    );
    assert!(
        !untrusted
            .events
            .iter()
            .any(|event| event.kind == "route_recomputed")
    );
    assert_eq!(
        untrusted.metrics.information_delivery["closure"].received_agents,
        1
    );
    assert_eq!(
        untrusted.metrics.information_delivery["closure"].accepted_agents,
        0
    );

    let trusted_source = source.replace("trust 0", "trust 1");
    let trusted = run(
        &parse(&trusted_source).expect("source parses"),
        RunOptions::default(),
    )
    .expect("run succeeds");
    assert!(
        trusted
            .events
            .iter()
            .any(|event| { event.kind == "connector_boarding" && event.detail == "backup" })
    );
    assert!(
        trusted
            .events
            .iter()
            .any(|event| event.kind == "route_recomputed")
    );
    assert_eq!(
        trusted.metrics.information_delivery["closure"].received_agents,
        1
    );
    assert_eq!(
        trusted.metrics.information_delivery["closure"].accepted_agents,
        1
    );
}

#[test]
fn explicit_sampling_key_preserves_trust_draws_across_renamed_interventions() {
    let source = r#"
scenario "matched-trust-draws"
seed 44
duration 2s
timestep 1s
surface concourse at (0m, 0m, 0m) size (30m, 10m)
exit street on concourse at (28m, 1m, 0m) width 2m
agents passengers count 6 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m
message baseline_notice source signage on concourse at (1m, 1m, 0m) claim exit street closed truth false time 1s reach 30m trust 0.5 sample matched_notice
"#;
    let candidate_source = source.replace("baseline_notice", "candidate_notice");
    let baseline = run(
        &parse(source).expect("baseline parses"),
        RunOptions::default(),
    )
    .expect("baseline runs");
    let candidate = run(
        &parse(&candidate_source).expect("candidate parses"),
        RunOptions::default(),
    )
    .expect("candidate runs");

    assert_eq!(
        baseline.metrics.information_delivery["baseline_notice"].accepted_agents,
        candidate.metrics.information_delivery["candidate_notice"].accepted_agents
    );
}

#[test]
fn required_waypoint_is_reached_before_the_final_exit() {
    let source = r#"
scenario "waypoint-journey"
seed 1
duration 12s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
waypoint checkpoint on concourse at (5m, 1m, 0m)
exit street on concourse at (9m, 1m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m via checkpoint
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("source validates");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    let waypoint = bundle
        .events
        .iter()
        .find(|event| event.kind == "waypoint_reached")
        .expect("journey reaches waypoint");
    let evacuation = bundle
        .events
        .iter()
        .find(|event| event.kind == "evacuated")
        .expect("journey reaches exit");
    assert_eq!(waypoint.detail, "checkpoint");
    assert!(waypoint.time_s < evacuation.time_s);
}

#[test]
fn validation_rejects_unknown_journey_waypoint() {
    let source = r#"
scenario "unknown-waypoint"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (9m, 1m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m via missing
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("unknown waypoint must be rejected");
    assert!(errors.iter().any(|error| error.path == "agents[0].via[0]"));
}

#[test]
fn validation_rejects_an_unreachable_required_waypoint() {
    let source = r#"
scenario "unreachable-waypoint"
seed 1
duration 10s
timestep 1s
surface platform at (0m, 0m, 3m) size (10m, 10m)
surface concourse at (0m, 0m, 0m) size (10m, 10m)
surface isolated_hall at (20m, 0m, 0m) size (10m, 10m)
waypoint checkpoint on isolated_hall at (21m, 1m, 0m)
exit street on concourse at (1m, 1m, 0m) width 2m
stair down from platform at (1m, 1m, 3m) to concourse at (1m, 1m, 0m) width 2m
agents passengers count 1 on platform at (1m, 1m, 3m) to street speed 1m/s radius 0.3m height 1.7m via checkpoint
"#;
    let scenario = parse(source).expect("source parses");
    let errors = validate(&scenario).expect_err("unreachable waypoint must be rejected");
    assert!(errors.iter().any(|error| error.path == "agents[0].via[0]"));
}

#[test]
fn ordered_waypoints_are_reached_in_source_order() {
    let source = r#"
scenario "multi-waypoint-journey"
seed 1
duration 16s
timestep 1s
surface concourse at (0m, 0m, 0m) size (12m, 10m)
waypoint first on concourse at (4m, 1m, 0m)
waypoint second on concourse at (7m, 1m, 0m)
exit street on concourse at (10m, 1m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m via first via second
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(&scenario, RunOptions::default()).expect("run succeeds");
    let reached: Vec<_> = bundle
        .events
        .iter()
        .filter(|event| event.kind == "waypoint_reached")
        .map(|event| event.detail.as_str())
        .collect();
    assert_eq!(reached, ["first", "second"]);
    assert_eq!(bundle.metrics.evacuated_agents, 1);
}

#[test]
fn waypoint_dwell_holds_an_agent_before_its_next_stage() {
    let source = r#"
scenario "waypoint-dwell"
seed 1
duration 10s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
waypoint checkpoint on concourse at (3m, 1m, 0m) dwell 2s
exit street on concourse at (6m, 1m, 0m) width 2m
agents passengers count 1 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.3m height 1.7m via checkpoint
"#;
    let scenario = parse(source).expect("source parses");
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    assert_eq!(
        bundle.trace[3].agents[0].state,
        chiyoda_core::AgentState::WaitingAtWaypoint
    );
    assert!(
        bundle
            .events
            .iter()
            .any(|event| event.kind == "waypoint_wait_started")
    );
    assert!(
        bundle
            .events
            .iter()
            .any(|event| event.kind == "waypoint_wait_ended")
    );
    assert_eq!(bundle.metrics.evacuated_agents, 1);
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
        &scenario,
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
fn gate_service_rate_creates_a_deterministic_queue() {
    let source = r#"
scenario "gate-capacity"
seed 1
duration 20s
timestep 1s
surface concourse at (0m, 0m, 0m) size (10m, 10m)
exit street on concourse at (9m, 1m, 0m) width 2m
gate fare_gate on concourse at (3m, 1m, 0m) width 1m capacity 0.5/s to street
agents passengers count 10 on concourse at (1m, 1m, 0m) to street speed 1m/s radius 0.1m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("source validates");
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    assert!(bundle.metrics.queued_for_gate_agents > 0);
    assert!(
        bundle
            .events
            .iter()
            .any(|event| event.kind == "gate_processed")
    );
}

#[test]
fn ten_thousand_agents_execute_a_spatially_indexed_step() {
    let source = r#"
scenario "ten-thousand"
seed 1
duration 100ms
timestep 100ms
surface concourse at (0m, 0m, 0m) size (100m, 100m)
exit street on concourse at (99m, 50m, 0m) width 4m
agents passengers count 10000 on concourse at (0m, 0m, 0m) to street speed 1m/s radius 0.3m height 1.7m
"#;
    let scenario = parse(source).expect("source parses");
    validate(&scenario).expect("spawn extent validates");
    let bundle = run(
        &scenario,
        RunOptions {
            trace_every_steps: 1,
        },
    )
    .expect("run succeeds");
    assert_eq!(bundle.metrics.total_agents, 10_000);
    assert_eq!(bundle.trace.len(), 2);
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

#[test]
fn checked_in_eindhoven_catalog_is_a_valid_pre_benchmark_source_lock() {
    let catalog_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/evidence/eindhoven-centraal-platform-2024.json");
    let catalog: EvidenceCatalog = serde_json::from_str(
        &std::fs::read_to_string(&catalog_path).expect("checked-in catalog is readable"),
    )
    .expect("checked-in catalog is JSON");
    validate_catalog(&catalog).expect("checked-in catalog follows the source-lock contract");
}
