# Calibration and held-out protocol

This protocol governs the Eindhoven Centraal platform catalog. It is deliberately
narrow: the catalog measures anonymous 2D motion on one platform, not evacuation
or whole-interchange behavior.

Only a catalog declared as `empirical_evaluation` can enter this protocol.
Content-locked `uncalibrated_reference` sources are useful for transparent
assumption discovery but have no calibration/held-out role and are rejected by
the calibration adapter.

## Pre-registration gate

Before reading a held-out report, publish a protocol record that fixes:

1. the runtime mechanism being tested and its exact version/hash;
2. the parameter(s) which calibration may change, with their allowed ranges;
3. the calibration objective and uncertainty estimate;
4. trajectory inclusion, gap, and outlier filters (the current descriptive
   intake defaults are a maximum 500 ms gap and 4 m/s speed);
5. the held-out metric, aggregation unit, pass/fail interpretation, and all
   planned subgroup analyses; and
6. an explicit statement that the resulting claim is limited to the measured
   horizontal platform primitive.

The protocol record must be committed before `--partition held-out` is run. A
subsequent parameter change, metric change, source reprocessing, or inspection
of held-out results invalidates that protocol and requires a new split or a new
round.

## Implemented narrow protocol: horizontal free-walking speed

The repository now implements one fixed protocol record, rather than allowing
callers to tune a model after inspecting results. It applies only to an opt-in
`walking-profile` DSL declaration of kind `horizontal-free-walking`:

1. The tested mechanism is the existing scalar `AgentGroup.speed_mps` preferred
   speed, with no change to local avoidance, route planning, queues, connectors,
   releases, or any default runtime parameter.
2. The profile is fitted only on days 01–30. A retained step must have exactly
   one tracked object in the full platform frame at both endpoints. This strict,
   zero-neighbour screen avoids relabeling the publisher's complex platform
   observations as unconditionally free walking; it does not prove the absence
   of environmental effects. It also requires speed at least 0.5 m/s, a fixed
   prospective screen for stationary tracks and tracker jitter rather than a
   value tuned against either partition. The one parameter is the resulting
   fixed 0.01 m/s histogram P50, the minimizer of the stated absolute-error
   objective.
3. Days 31–60 are processed only by the evaluation command using the same
   content-locked catalog and unchanged 500 ms / 4 m/s filter. The held-out
   report gives signed and absolute error between the fitted scalar and the
   held-out P50, while retaining the full held-out descriptive summary.
4. There is deliberately no invented pass threshold. A completed temporal
   comparison receives `held_out_comparison_complete_not_accepted`; it is not a
   product-acceptance, population, trajectory, or predictive-validity result.

Run the three stages explicitly:

```console
$ cargo run -p chiyoda -- calibrate free-walking-speed-profile \
    benchmarks/evidence/eindhoven-centraal-platform-2024.json \
    -o out/eindhoven-free-walking-profile.json
$ cargo run -p chiyoda -- calibrate evaluate-free-walking-speed-profile \
    benchmarks/evidence/eindhoven-centraal-platform-2024.json \
    out/eindhoven-free-walking-profile.json \
    -o out/eindhoven-free-walking-held-out.json
$ cargo run -p chiyoda -- calibrate emit-free-walking-speed-dsl \
    out/eindhoven-free-walking-profile.json \
    out/eindhoven-free-walking-held-out.json \
    -o out/eindhoven-free-walking-profile.chy
```

Copy the one emitted `walking-profile` line near the top of an authored source,
then use `speed profile eindhoven_platform_free_walking_p50` for a specifically
scoped group. The generated profile and evaluation are self-hashed; the emitted
DSL line carries both artifact hashes and the catalog hash. This makes a bundle
replayable without a raw-data dependency while leaving the source trail
inspectable.

## Split and leakage control

The catalog uses immutable ten-day Parquet source files as split units:

- days 01–30: calibration;
- days 31–60: held-out.

No row, file, or trajectory identifier crosses the partitions. This prevents
the common error of randomly splitting adjacent observations from the same
pedestrian. It does not provide independence across sites, stations, seasons,
or measurement systems; those require later external datasets.

## What the current adapter measures

For every object identifier, the adapter compares consecutive samples in the
same source file, converts millimetres to metres, and records horizontal speed.
It reports retained observations and each exclusion category: first observation,
non-positive time interval, interval over the gap limit, and speed over the
limit. Mean and population standard deviation are streaming statistics;
P05/P50/P95 use a documented 0.01 m/s histogram. Every report names every
source SHA-256 digest.

The adapter is therefore auditable, but it is not a model fit. In particular it
does not infer body radius, height, avoidance force, path planning, route choice,
gate service, lift behavior, stair behavior, trust, or countermeasure effects.

## Acceptance rule

No value may be copied from a descriptive report into a runtime default,
population profile, benchmark score, or product claim without a reviewed
protocol and a held-out prediction result. Even a successful result supports
only the declared measured primitive and uncertainty; it does not validate
Chiyoda for operational, regulatory, or life-safety use.
