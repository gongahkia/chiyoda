# Executable semantics 0.42

The Rust `chiyoda-core` runtime is the reference interpreter for language 0.27
under runtime contract 0.42. This document is normative where it describes public
behavior;
the source and conformance tests make that behavior executable.

## State and step order

The interpreter initializes a deterministic row-major spawn layout for each
agent group. Static validation checks every generated starting coordinate
against the declared surface and the same radius-expanded obstacles used by the
navigator, and requires every generated disc on one surface to clear every
other generated disc by their combined radii plus the navigation epsilon,
regardless of release time. A simulation step at time `t` performs,
in chronological event order. Events with equal timestamps run in this order:
connector availability, exit availability, gate availability, connector
capacity, exit capacity, gate capacity, message, countermeasure. Declaration
order breaks ties within one event kind.

1. Apply each availability and capacity-state event whose declared time falls
   in `(previous step time, t]`. The final step is shortened when necessary so
   that `t` is exactly the authored duration. `connector-state`, `exit-state`,
   and `gate-state`
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
   `(previous step time, t]` to active agents on the same surface within the
   declared reach radius. A not-yet-released group is not a recipient.
3. Update accepted connector-, exit-, or gate-availability beliefs. Each eligible recipient
   accepts an intervention when its deterministic seed-derived sample is below
   the declared trust probability. An accepted gate belief excludes only that
   gate from a selected final-exit plan. A qualifying countermeasure sets the
   corrected connector, exit, or gate belief to its current physical state and
   recomputes a route. Physical closures are excluded regardless of belief. If the active
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
   effective authored people-per-second rates for this step's actual elapsed
   time. A capacity state
   in the step therefore takes effect before accrual. Each resource begins
   empty and stores at most one effective rate's worth of credit, with a
   one-person minimum for rates below `1/s`; an idle resource therefore retains
   no more than one second of throughput (or one discrete person). Only whole
   tokens may be consumed.
5. Treat each agent whose authored release time is at or before `t` as eligible
   for admission. Its generated spawn disc must be statically clear and
   dynamically clear of the current same-surface occupants. An uncleared agent
   remains outside the modeled surface in `waiting_to_depart`, emits one
   `agent_release_deferred_for_clearance` event, and is retried at later steps;
   `agent_released` records the actual admission time. Admitted agents then
   enter the on-surface snapshot before selecting any on-surface velocity.
   The visibility-graph path supplies each agent's preferred Euler velocity.
   For every same-surface candidate whose current separation is at most the
   sum of their radii plus both authored speed bounds over the fixed 2.5 s
   horizon, the runtime constructs a deterministic `f64` ORCA half-plane,
   except where the explicit queue-grid right-of-way rule below removes the
   later ticket's constraint from the earlier ticket's program. This
   conservative neighbour screen does not predict from a candidate's previous
   velocity, because either agent may choose a new bounded preferred velocity
   at this step. It chooses the
   speed-bounded velocity closest to the preference from their intersection.
   Two members of the same preallocated `queue-grid` use their monotonic FIFO
   tickets as an explicit local right-of-way rule: the earlier ticket does not
   yield an ORCA constraint to the later ticket, while the later ticket bears
   the full inside-obstacle correction. This applies only within one authored
   grid; line footprints and all other pairs retain reciprocal ORCA treatment.
   A later grid ticket receives an earlier same-grid ticket's selected,
   statically clipped same-step velocity when forming its full-responsibility
   constraint. All positions, all non-grid velocities, and every other pair
   remain from the immutable snapshot. Exact co-location uses a stable
   identifier-derived direction rather than a random draw. The selected segment
   is clipped to the static surface/obstacle-free region; a static conflict can
   shorten the selected velocity. If the speed-bounded half-planes are
   infeasible, the runtime emits a `local_avoidance_constraint_fallback` event
   and chooses a deterministic least-penetrating velocity. The horizon and
   interaction screen are uncalibrated reference assumptions; they do not
   establish a full-system physical-collision guarantee, especially for
   unmodelled geometry, connector transit, or fallback states.
   A `waiting_at_waypoint` agent is an immobile on-surface neighbour for this
   snapshot and for later landing-clearance tests. A connector with
   `portal-lanes` uses the agent's stable identifier-selected exact lane at
   both portals. At destination arrival, it remains in transit until that lane
   is statically and dynamically clear; it is not relocated to another lane or
   endpoint. A connector without `portal-lanes` retains the separate legacy
   endpoint-clearance placement rule.
   For an eligible line `queue-footprint`, capacity denial records the normal
   queue-entry audit and gives the agent a monotonic deterministic FIFO ticket.
   An immutable per-step queue snapshot ranks tickets, using the agent
   identifier only as a stable tie-breaker, and each entrant walks to its
   authored standing slot. A `queue-grid` instead gives each eligible
   on-surface request a monotonic ticket and distinct standing-slot target
   before local motion, recording `queue_slot_preallocated` with the exact
   resource and ticket. Physical arrival records its queue-entry audit and
   turns that ticket into a modeled wait. An unarrived grid ticket does neither.
   A grid follows its validated serpentine FIFO path, beginning at its exact
   authored front position. Only an agent that reaches the head slot can reserve service;
   it consumes an existing token for a non-lift connector, gate, or exit, or
   reserves one lift cabin place. It emits `queue_service_reserved`, leaves the
   footprint, and then physically approaches its resource without a second
   token or capacity check. A lift place remains occupied from reservation
   through transit; an on-surface reroute that changes the next resource
   relinquishes it before another agent may reserve the cabin. Static validation
   requires enough slots for every declared agent, so this contract has no
   unlocated overflow. Resources without a footprint retain declaration-ordered
   abstract service actions: an agent's clearance disc may contact a
   zero-geometry service target rather than requiring centre-point arrival.
   This avoids numerical deadlock without relaxing clearance during the
   approach. For `release T
   every I batch N`, ordinal agents share one release time by integer batch:
   `T + I * floor(ordinal / N)`. Omitted `batch` is one; omitted `every`
   releases the whole group at `T`. Each time is eligibility rather than an
   unconditional surface materialization: dynamic spawn clearance may delay
   individual admission. Release occurs after information delivery in the same
   step.
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
   exit capacity. At the completed integration boundary, the runtime audits
   every unordered pair of agents that simultaneously occupies one surface. A
   pair contributes when its horizontal centre distance is smaller than the
   sum of its authored reference-disc radii by more than the navigation
   epsilon. Separately, it analytically minimizes the horizontal separation of
   each eligible pair's linearly interpolated start/end paths over the completed
   interval. A swept pair is eligible only when both agents occupied the same
   surface at both boundaries, so release, portal, transit, surface-change, and
   evacuation transitions are excluded. Both audits exclude transit,
   unreleased, and evacuated states; neither reports physical contact.

State-event and service ties resolve by declaration and generated-agent order.
This is intentional: the complete order is recorded in canonical source and
must not be replaced by unordered map iteration. Local-motion neighbor lines
sort by stable generated agent identifier rather than mutable spatial-index
cell order.

## What this does not mean

The `0.42` local-motion law, nominal routing cost and alternative-exit
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

Current bundles also retain `movement_metrics`, an audit record for the
local-motion layer. `agents_with_local_clearance_adjustments` counts distinct
agents whose planned ordinary on-surface movement or legacy connector endpoint
placement was changed or deferred by that layer.
`local_clearance_adjustment_steps` counts those individual attempted-position
adjustments larger than the navigation clearance epsilon;
`cumulative_local_clearance_adjustment_m` sums the Euclidean
distance from planned to resolved position, and
`maximum_local_clearance_adjustment_m` is the largest one-attempt distance.
Zero-geometry service admission remains excluded for resources without a
footprint. Ordinary on-surface motion uses snapshot ORCA, including an
authored queue slot's approach and advancement. Where `portal-lanes` is
declared, exact connector lane arrivals are retained in transit until clear
rather than adjusted; gate and exit lanes are target locations only. This
telemetry is not a count of contacts or collisions, a density measure, a
physical crowd model, an observed delay, or calibration evidence.

Runtime contract `0.31`, retained in `0.42`, additionally records
`local_avoidance_constraint_fallback_steps`: each step in which the
speed-bounded ORCA half-planes were infeasible and the documented deterministic
least-penetrating fallback was selected. Every such count is cross-checked
against one `local_avoidance_constraint_fallback` event whose subject is an
agent present in the initial trace and whose detail fixes the reason. The
counter and its event audit are omitted from earlier bundles rather than
backfilled as zeroes. They report solver infeasibility, not contacts,
collisions, physical safety, or calibration evidence. A non-zero counter can
arise from dense competing motion under a finite speed bound; it does not by
itself identify an implementation fault or an initial overlap.

Runtime contract `0.36` additionally records
`on_surface_clearance_audit`. `agents_with_disc_overlaps` counts distinct
agents that appeared in at least one overlapping pair at a completed
integration boundary; `disc_overlap_pair_steps` counts the unordered pairs at
those boundaries, so the same pair may contribute repeatedly; and
`maximum_disc_overlap_m` is the largest positive sum-of-radii minus horizontal
centre-distance. The independent CLI and Python bundle readers require all
three fields, zero them together when no pair-step exists, and bound them by
the declared agent population, radii, and integration schedule. Current-runtime
bundle verification also deterministically reconstructs the run. The audit is
reference-runtime provenance, not swept-time collision detection, observed
contact, a density measurement, or a physical-safety claim. It is omitted from
earlier bundles rather than backfilled as zero.

Runtime contract `0.37` additionally records
`swept_on_surface_clearance_audit`. Its
`agents_with_swept_disc_overlaps` counts distinct agents that participated in
at least one positive-overlap pair along an eligible same-surface interval;
`swept_disc_overlap_pair_steps` counts those unordered pairs once per interval;
and `maximum_swept_disc_overlap_m` is the greatest positive sum-of-radii minus
analytic minimum horizontal centre-distance. The audit calculates closest
approach from the two linear paths rather than sampling intermediate times. It
is an audit of the discrete reference trajectory, not continuous physical
collision detection: it does not model motion within a step beyond linear
interpolation, transitions between surfaces, contact, density, physical safety,
or calibration. The independent CLI and Python readers require complete,
population-, radius-, and step-schedule-bounded telemetry, and earlier bundles
omit it rather than backfill zeroes.

`chiyoda verify-reference-clearance BUNDLE` is an optional engineering
acceptance gate for a current, reconstructible bundle. It requires zero
pair-steps and zero maximum overlap in both reference-disc audits. It rejects
missing, legacy, or non-reconstructible telemetry. This gate is deliberately
stricter than ordinary bundle verification, but it remains a condition on the
discrete reference model, not evidence of physical contact avoidance or
operational safety.

Current bundles also retain discrete `queue_metrics` for lift, non-lift
connector, gate, and exit capacity queues. `ever_queued_agents` is the number
of agents that entered a queue at least once; it mirrors the older top-level
exposure count. `cumulative_wait_agent_seconds` adds the actual reference-step
duration for each agent that was in that queue during the preceding reference
step, and
`peak_waiting_agents` is the largest count seen at a step boundary. These are
internally consistent telemetry for this discrete runtime, not observed queue
lengths, delays, throughput measurements, or calibrated flow values. A
`queue-footprint` makes the waiter's simulated standing position explicit; it
does not make that structural model an observation of a real queue.

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
all resources of that mechanism at a step boundary. The breakdown does not by
itself identify a real-world bottleneck or a measured service rate; an authored
footprint is separately visible in canonical source.

Runtime contract `0.31`, retained in `0.42`, also records an event when an agent first enters each
such modeled resource queue. Its event kind is one of `queue_entered_lift`,
`queue_entered_connector`, `queue_entered_gate`, or `queue_entered_exit`; its
subject is the generated agent identifier and its detail is the exact authored
resource identifier. For a grid, the event time is physical arrival at the
preallocated standing slot; for a line footprint or a resource without geometry
it remains the capacity-denial step. Repeated queue attempts by the same agent
at the same resource do not add events, while a later queue entry at a
different resource does. Bundle verification requires each
resource's events and each mechanism's unique-agent union to equal the
corresponding `ever_queued_agents` telemetry. A footprint-enabled queue also
emits one `queue_service_reserved` event when each head agent reserves a
service token or lift cabin place. For a lift footprint its detail remains
`connector:ID`, but its required prior entry audit is `queue_entered_lift`.
These are runtime audit trails, not observations of a physical queue.

Runtime contract `0.41`, retained in `0.42`, additionally records `queue_slot_preallocated` for
each grid ticket before local motion. Its subject is the generated agent
identifier and detail is `KIND:RESOURCE:TICKET`. It is not queue entry, a wait,
or service admission; its purpose is to make the distinction between an
assigned future slot and physical queue arrival inspectable from a run bundle.

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
