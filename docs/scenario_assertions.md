# Scenario Assertions And Toy Checks

Runtime assertions live under the scenario-level `assertions` key. They run
after a scenario simulation and are intended for regression tests, not
statistical validation.

Run them with:

```sh
.venv/bin/python -m chiyoda.cli assert-scenario scenarios/validation_multifloor_connectors.yaml
```

Supported checks:

- `evacuated`: integer or `{eq,min,max}`.
- `remaining`: integer or `{eq,min,max}`.
- `travel_time_s`: `{min,max,mean_min,mean_max}` over evacuated agents.
- `agent_base_speed_mps`: `{min,max,mean_min,mean_max}` over non-responder agents.
- `agent_release_step`: `{min,max,mean_min,mean_max}` over non-responder agents.
- `connector_usage`: mapping from connector ID to integer or `{eq,min,max}`.
- `connector_flow`: latest-step finished transfers per connector.
- `connector_capacity`: configured queue capacity per connector.
- `connector_queue_length`: latest waiting count per connector.
- `exit_usage`: mapping from exit label or `floor:x,y` to integer or `{eq,min,max}`.
- `cohort_exit_usage`: nested cohort-to-exit mapping with integer or `{eq,min,max}`.
- `no_impossible_floor_jumps: true`.
- `impossible_floor_jumps`: integer or `{eq,min,max}`.
- `exit_floors`: list of floor IDs that must receive at least one evacuation.

Regression scenarios:

- `scenarios/validation_multifloor_connectors.yaml`: stairs, ramp, escalator,
  elevator, three floors, and a floor-1 hazard.
- `scenarios/validation_elevator_queue.yaml`: elevator capacity and
  simultaneous-arrival queue pressure.

Toy calibration script:

```sh
.venv/bin/python scripts/run_toy_calibrations.py -o out/toy_calibrations.json
```

Profiler script:

```sh
.venv/bin/python scripts/profile_large_scenario.py scenarios/station_sarin.yaml \
  --max-steps 100 \
  --population-total 250 \
  -o out/profile_station_sarin.json
```

The profiler records runtime, NetworkX graph size, density-update count,
telemetry row estimates, connector usage, memory peak, and top cumulative-time
functions. It is a local engineering profiler, not a benchmark suite.
