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
