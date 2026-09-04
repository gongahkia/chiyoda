# Executable semantics 0.1

The Rust `chiyoda-core` runtime is the reference interpreter for language
version 0.1. This document is normative where it describes public behavior;
the source and conformance tests make that behavior executable.

## State and step order

The interpreter initializes a deterministic row-major spawn layout for each
agent group. A simulation step at time `t` performs, in order:

1. Deliver messages and countermeasures whose declared time falls in
   `(t - timestep, t]` to agents on the same surface within the declared
   reach radius.
2. Update accepted connector-availability beliefs. A trust value of at least
   `0.5` is accepted by this reference policy; a closed connector is excluded
   from a new shortest directed surface route. A qualifying countermeasure
   removes the false connector closure and recomputes a route.
3. Accrue gate service tokens at their declared people-per-second rate.
4. Advance in-transit agents, then advance on-surface agents in declaration
   order using a fixed Euler step, radius-based local separation, and surface
   bounds clamping.
5. Board an available connector, process a gate token, or mark the agent
   evacuated. Lifts enforce cabin capacity during their declared cycle.

Ties resolve by declaration and generated-agent order. This is intentional:
the complete order is recorded in canonical source and must not be replaced by
unordered map iteration.

## What this does not mean

The `0.1` local-separation law and `0.5` information acceptance threshold are
reference semantics, not calibrated behavioral claims. A valid source program
or deterministic trace does not demonstrate crowd-flow accuracy, accessible
egress fidelity, message effectiveness, or operational safety.

## Reproducibility contract

`chiyoda run` writes `scenario.chy` and `run.json`. A run bundle contains its
canonical scenario, runtime version, options, trace, event log, metrics,
scenario hash, and SHA-256 bundle hash. The bundle hash is computed from the
entire bundle with the hash field blank. `chiyoda replay` and
`chiyoda-replay` reject an invalid bundle hash before interpreting a trace.

Given identical source, language/runtime version, options, and supported Linux
environment, the reference runtime is expected to emit an identical bundle
hash. This is tested for the current implementation; it does not establish
cross-version or cross-architecture numerical equivalence.

