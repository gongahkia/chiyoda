# Executable semantics 0.19

The Rust `chiyoda-core` runtime is the reference interpreter for language
version 0.19. This document is normative where it describes public behavior;
the source and conformance tests make that behavior executable.

## State and step order

The interpreter initializes a deterministic row-major spawn layout for each
agent group. Static validation checks every generated starting coordinate
against the declared surface and the same radius-expanded obstacles used by the
navigator before the interpreter starts. A simulation step at time `t` performs,
in chronological event order. Events with equal timestamps run in this order:
connector state, exit state, message, countermeasure; declaration order breaks
any remaining tie within an event class.

1. Apply each `connector-state` and `exit-state` event whose declared time falls
   in `(t - timestep, t]` to physical availability. State events at `0s` are
   applied before the initial trace; same-time events apply in declaration
   order within their event class. A state change immediately recomputes each
   on-surface agent's route. It does not interrupt an agent already in connector
   transit or already evacuated through an exit; an arriving agent recomputes
   before attempting a now-closed final exit.
2. Deliver each message and countermeasure whose declared time falls in
   `(t - timestep, t]` to active agents on the same surface within the
   declared reach radius. A not-yet-released group is not a recipient.
3. Update accepted connector- or exit-availability beliefs. Each eligible recipient
   accepts an intervention when its deterministic seed-derived sample is below
   the declared trust probability. A qualifying countermeasure sets the
   corrected resource belief to its current physical state and recomputes a
   route. Physical closures are excluded regardless of belief. If the active
   constraints leave no route, an on-surface agent waits in the traceable
   `waiting_for_route` state until a later state or information event produces
   one. The route cost includes obstacle-aware Euclidean walking from the
   agent's current point to each connector and exit plus connector transit time.
   Each rectangular obstacle is expanded by the agent radius and the shortest
   path over visible clearance corners is used. A connector with an authored
   height clearance is excluded when the agent's authored height exceeds it,
   as is a connector class explicitly excluded by the agent group. The route
   does not forecast queues, density, or later messages. When a final stage
   offers alternative exits, the runtime compares feasible nominal route costs
   and chooses the shortest; ties resolve by final-exit declaration order and
   then connector declaration order. It re-evaluates that choice only at
   initial routing or an existing reroute trigger. A controlled exit's cost
   includes the walk to its selected gate and the subsequent walk to the exit,
   but not its gate-service delay.
   The run bundle records every declared message and countermeasure's reached
   and accepted-agent counts, including zeroes. These are deterministic runtime
   observations under the authored trust rule, not measured communication uptake.
   An optional explicit sampling key replaces the intervention identifier in
   that draw. Keys are globally unique within one scenario; matching a key
   across separately authored comparison arms deliberately aligns draw streams.
4. Accrue gate, declared connector, and declared exit service tokens at their authored
   people-per-second rates. Each resource begins empty and stores at most one
   authored rate's worth of credit, with a one-person minimum for rates below
   `1/s`; an idle resource therefore retains no more than one second of
   throughput (or one discrete person). Only whole tokens may be consumed.
5. Release groups whose authored release time is at or before `t`, then
   advance in-transit agents and on-surface agents in declaration order using
   a fixed Euler step, radius-based local separation, and surface bounds
   clamping. Release occurs after information delivery in the same step.
6. Board an available connector, process a gate token, process an exit token,
   advance from a reached next required waypoint or final exit stage, or mark
   the agent evacuated. A
   waypoint with dwell holds the agent until its authored expiry time.
   Stairs and ramps use endpoint distance divided by agent speed;
   escalators add their declared belt speed; lifts enforce cabin capacity
   during their declared cycle. A non-lift connector with an authored service
   rate consumes one deterministic service token before boarding; a connector
   without that declaration remains unlimited. An exit with an authored service
   rate likewise consumes one token after any gate processing; an undeclared
   exit capacity remains unlimited. The trace distinguishes waiting for a lift,
   an authored non-lift connector capacity, and an authored exit capacity.

Ties resolve by declaration and generated-agent order. This is intentional:
the complete order is recorded in canonical source and must not be replaced by
unordered map iteration.

## What this does not mean

The `0.19` local-separation law, nominal routing cost and alternative-exit
selection, scheduled-release semantics, operational-state transitions,
escalator walking-rider assumption, and seeded information acceptance law are
reference semantics, not calibrated behavioral claims. A valid source program
or deterministic trace does not
demonstrate crowd-flow accuracy, accessible egress fidelity, message
effectiveness, operational-state fidelity, or operational safety.

`clearance_time_s` is emitted only if every released agent has evacuated by the
configured end time. If at least one agent evacuated but one or more agents
remain, `last_exit_time_s` records the final observed evacuation instead. The
two measurements must not be substituted for each other: a partial run has no
system clearance time.

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
