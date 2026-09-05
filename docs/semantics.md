# Executable semantics 0.24

The Rust `chiyoda-core` runtime is the reference interpreter for language 0.21
under runtime contract 0.24. This document is normative where it describes public
behavior;
the source and conformance tests make that behavior executable.

## State and step order

The interpreter initializes a deterministic row-major spawn layout for each
agent group. Static validation checks every generated starting coordinate
against the declared surface and the same radius-expanded obstacles used by the
navigator before the interpreter starts. A simulation step at time `t` performs,
in chronological event order. Events with equal timestamps run in this order:
connector availability, exit availability, gate availability, connector
capacity, exit capacity, gate capacity, message, countermeasure. Declaration
order breaks ties within one event kind.

1. Apply each availability and capacity-state event whose declared time falls
   in `(t - timestep, t]`. `connector-state`, `exit-state`, and `gate-state`
   alter physical availability and immediately recompute every on-surface route.
   `connector-capacity-state`, `exit-capacity-state`, and
   `gate-capacity-state` alter only the effective service rate and emit an
   event; route selection does not forecast queues. State events at `0s` are
   applied before the initial trace; same-time declarations of one kind apply
   in declaration order. An availability change does not interrupt an agent
   already in connector transit or already evacuated through an exit; an
   arriving agent recomputes before attempting a now-closed final exit. A gate
   closure excludes that gate from future final-exit plans; it does not revoke
   passage already processed through the gate.
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
4. Accrue gate, declared connector, and declared exit service tokens at their
   effective authored people-per-second rates for this step. A capacity state
   in the step therefore takes effect before accrual. Each resource begins
   empty and stores at most one effective rate's worth of credit, with a
   one-person minimum for rates below `1/s`; an idle resource therefore retains
   no more than one second of throughput (or one discrete person). Only whole
   tokens may be consumed.
5. Release each agent whose authored release time is at or before `t`, then
   advance in-transit agents and on-surface agents in declaration order using
   a fixed Euler step, radius-based local separation, and surface bounds
   clamping. For `release T every I batch N`, ordinal agents share one release
   time by integer batch: `T + I * floor(ordinal / N)`. Omitted `batch` is
   one; omitted `every` releases the whole group at `T`. Release occurs after
   information delivery in the same step.
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
   an authored non-lift connector capacity, a gate capacity, and an authored
   exit capacity.

Ties resolve by declaration and generated-agent order. This is intentional:
the complete order is recorded in canonical source and must not be replaced by
unordered map iteration.

## What this does not mean

The `0.24` local-separation law, nominal routing cost and alternative-exit
selection, scheduled-release and service-capacity semantics, operational-state
transitions, escalator walking-rider assumption, and seeded information
acceptance law are reference semantics, not calibrated behavioral claims. A
valid source program or deterministic trace does not
demonstrate crowd-flow accuracy, accessible egress fidelity, message
effectiveness, operational-state fidelity, or operational safety.

`clearance_time_s` is emitted only if every released agent has evacuated by the
configured end time. If at least one agent evacuated but one or more agents
remain, `last_exit_time_s` records the final observed evacuation instead. The
two measurements must not be substituted for each other: a partial run has no
system clearance time.

Current bundles also retain discrete `queue_metrics` for lift, non-lift
connector, gate, and exit capacity queues. `ever_queued_agents` is the number
of agents that entered a queue at least once; it mirrors the older top-level
exposure count. `cumulative_wait_agent_seconds` adds one authored timestep for
each agent that was in that queue during the preceding reference step, and
`peak_waiting_agents` is the largest count seen at a step boundary. These are
internally consistent telemetry for this discrete runtime, not physical queue
lengths, observed delays, throughput measurements, or calibrated flow values.

Current bundles additionally retain `queue_metrics.by_resource`. Its `lifts`,
`connectors`, `gates`, and `exits` maps contain every authored resource whose
semantics can create the corresponding wait state: every lift and gate, plus
only non-lift connectors and exits with an authored capacity. An unreached
constrained resource is retained with zero values. These entries attribute the
same discrete wait observations to the resource that denied the agent at that
step. Aggregate cumulative wait equals the sum of its resource entries, but an
aggregate mechanism exposure may be lower than the resource-entry total when
one agent queued at multiple resources. An aggregate mechanism peak is not
reconstructed from resource peaks: it is the maximum simultaneous count across
all resources of that mechanism at a step boundary. The breakdown does not
identify a geometric queue, a real-world bottleneck, or a measured service rate.

Runtime contract `0.24` also records an event when an agent first enters each
such modeled resource queue. Its event kind is one of `queue_entered_lift`,
`queue_entered_connector`, `queue_entered_gate`, or `queue_entered_exit`; its
subject is the generated agent identifier and its detail is the exact authored
resource identifier. The event time is the capacity-denial step. Repeated
denials of the same agent by the same resource do not add events, while a later
queue entry at a different resource does. Bundle verification requires each
resource's events and each mechanism's unique-agent union to equal the
corresponding `ever_queued_agents` telemetry. This is a runtime audit trail,
not an observation of a physical queue.

## Reproducibility contract

`chiyoda run` writes `scenario.chy` and `run.json`. A run bundle contains its
canonical scenario, runtime version, options, trace, event log, metrics,
scenario hash, and SHA-256 bundle hash. The bundle hash is computed from the
entire bundle with the hash field blank. For a bundle compatible with the
installed runtime contract, `chiyoda replay` and `chiyoda-replay` rebuild the
complete run from the embedded scenario and options before interpreting a
trace. An incompatible legacy bundle is rejected by default; an explicit
hash-only inspection mode is available for archival review.

Given identical source, language/runtime version, options, and supported Linux
environment, the reference runtime is expected to emit an identical bundle
hash. This is tested for the current implementation; it does not establish
cross-version or cross-architecture numerical equivalence.
