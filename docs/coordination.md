# Coordinated queue-grid motion

## Status

The ordinary runtime remains a deterministic local-motion reference model. It
does not claim that an authored `queue-grid` is collision-free: its boundary
and swept reference-disc audits are deliberately retained as failure-finding
telemetry. In particular, the dense queue-grid stress source is expected to
fail the strict `verify-reference-clearance` acceptance command.

The first coordination primitive is now available in `chiyoda_core::coordination`.
It accepts declared, timed linear reference-disc trajectories and deterministically
reports every same-surface conflict, including the exact open unsafe interval
and maximum reference-disc overlap. It uses the runtime's clearance epsilon,
not a separate endpoint-only rule. It excludes portal, lift, and other transit
geometry unless a future model explicitly represents those intervals.

The same module now provides a bounded planner over an explicitly supplied
static roadmap. It first tries a shortest spatial route, then represents each
roadmap node as continuous safe time intervals rather than enumerating every
`(node, time)` wait state. Its every candidate wait and move is still checked
continuously against declared occupied trajectories. The original fully
time-expanded search remains a bounded fallback, so a search-bound exhaustion
remains distinct from a result with no plan in the declared finite model.

`CoordinationRoadmap::lattice` derives one such bounded roadmap from a declared
surface, the declared obstacles, a reference-disc radius, a spacing, and exact
anchors. It rejects an unsafe anchor or node-bound exhaustion and accepts an
edge only when its entire radius-expanded segment is statically clear. The
caller must choose the lattice spacing and node bound; neither is calibrated or
derived from a facility.

A bounded deterministic conflict-repair tree is now layered over that search.
It begins with individual plans and a deterministic sequential-formation seed,
then selects the earliest remaining continuous-time conflict. Each branch adds
the other participant's exact conflicting segment as a prohibition for one
agent and replans only that agent; it does not freeze every peer trajectory as
permanent occupancy. A result is returned only after the same exact conflict
kernel finds no remaining conflicts. The finite roadmap, time horizon,
low-level bound, and conflict-tree bound mean this is not a complete solver;
bound exhaustion is an explicit unknown result.

The repair request can also carry prior accepted trajectories. This is the
handoff contract for rolling windows: each new cohort plans against all earlier
accepted reservations, and the request rejects an already-conflicted handoff
instead of trying to hide it. It now accepts either a conventional permanent
goal or a sequence of queue-rank windows through one shared repair tree, so
the two objective forms cannot acquire different conflict semantics.

Queue-grid targets are not permanent per-ticket goals. When the FIFO head takes
service, all remaining active tickets advance one authored rank. The module now
derives this activation/departure timeline as `QueueGridSlotWindow` intervals;
it rejects a non-head departure and does not permit a service event to rewrite
FIFO. `plan_multi_stage` turns each ticket's contiguous window sequence into a
continuous trajectory: it holds at the released position until activation,
moves only within the active rank window, and reserves a reached rank through
the next handoff. Treating every initial slot as occupied until the end of a
run is intentionally rejected: it both disagrees with runtime queue semantics
and made the dense rolling stress case infeasible around ticket 96.

`queue_grid_timed_targets` is the explicit bridge between those FIFO windows
and a roadmap: each authored rank must be bound to one exact roadmap node. It
does not snap to a nearby vertex when a rank is missing, because that would
silently change the authored queue geometry.

`plan_queue_grid` composes the event timeline, rank-to-node binding,
multi-stage tasks, prior reservations, and bounded conflict repair into one
core API. Its service-departure inputs are declared scheduling facts; it does
not infer service capacity, demand, or human queueing behaviour. That keeps
the eventual runtime policy reviewable: a runtime caller must explicitly own
how it predicts departures, how often it replans, and what it does when this
bounded solver returns no plan or exhausts a bound.

`plan_queue_grid_rolling` processes those tasks in deterministic back-to-front
formation cohorts.
Each cohort is checked against every earlier accepted trajectory, so this is a
bounded decomposition rather than an unchecked heuristic. Cohort size is an
explicit computational policy: smaller cohorts limit each conflict tree but
can rule out solutions that require revising an earlier accepted trajectory.
The runtime must report that policy with any result that uses it.

The planner currently chooses a back-to-front *formation* order for rolling
cohorts; that never alters FIFO service eligibility. `assess_queue_grid_rolling`
returns the first cohort that cannot be added without reopening earlier paths,
instead of reducing that result to an unexplained `None`. Under the checked
exploratory policy—a 0.6 m roadmap, 0.5 s planning grid, 135 s first
completion, 4 s headway, and cohorts of eight—the 152-agent stress source
reports tickets 144 through 137 as the first cohort with no plan. This is
evidence about that finite roadmap, rolling decomposition, and assumed
schedule—not evidence that the authored geometry has no physical solution.

For exploratory work without qualifying service data,
`estimate_queue_grid_departures` derives those inputs from an explicit
`QueueGridServiceAssumption` (first completion and constant active-queue
headway). This is intentionally an uncalibrated scenario assumption, not a
capacity model or an observed service claim. If its schedule makes a physical
formation impossible, `plan_queue_grid` returns no plan rather than extending
or rewriting the assumed headway.

The explicit `chiyoda coordinate-queue-grid` command writes a self-contained
coordination artifact. It embeds the source and its SHA-256 hash, selected group
and queue-grid IDs, every planning bound, the deliberately uncalibrated service
assumption, and either exact trajectories or the first bounded no-plan/unresolved
cohort. `chiyoda verify-queue-grid-coordination` verifies the source hash,
reconstructs the same outcome, and, for a planned artifact, runs the exact
continuous conflict check again. For example:

```console
$ chiyoda coordinate-queue-grid examples/experiments/queue-grid-stress.chy \
    --group passengers --queue-grid fare_gate_queue \
    --first-departure-at-s 135 --headway-s 4 -o out/coordination.json
$ chiyoda verify-queue-grid-coordination out/coordination.json
```

This is deliberately separate from `chiyoda run`: the default local-motion
runtime and existing run-bundle format do not silently consume a coordination
artifact. Runtime execution must be a later explicit policy with a defined
replanning cutover. The artifact is replayable as a planning result, not a
physical-safety certificate.

## Chosen architecture

The coordinator will be a bounded, optional planning layer for an authored
queue grid, separate from generic ORCA:

1. Construct a statically clear roadmap for the affected surface, including
   release positions, authored grid slots, and obstacle-aware transitions.
2. Translate queue ticket activation and service-departure events into
   rank-window tasks, then use the spatial-route and safe-interval roadmap
   planner to produce a timed trajectory for each disc while respecting
   already declared trajectory constraints. Retain the fully time-expanded
   search only as a bounded fallback with the same conflict semantics.
3. Detect conflicts with the continuous-time kernel rather than comparing only
   waypoint or integration-boundary occupancy.
4. Resolve a conflict through a bounded conflict tree: branch on a prohibition
   for one participant or the other, then replan only the constrained path.
5. Accept a plan only when the exact kernel reports no conflicts. If the bound
   is exhausted or the roadmap has no plan, return an explicit unresolved
   result; do not fall back to a claim of clearance.
6. Execute only the accepted timed plan and retain the runtime audits as an
   independent check. A zero audit is an internal reference-model acceptance
   result, not a real-world safety or facility-fidelity claim.

The plan must preserve FIFO as a service and audit rule—ticket order controls
grid-slot and service eligibility—not as an assertion that a local priority
heuristic is complete. A planner may choose temporary path ordering necessary
to form the queue, but it may not let a later ticket consume service ahead of
an earlier physically eligible ticket.

## Why this shape

Classical conflict-based search (CBS) builds a constraint tree and replans the
individual participant at each conflict; it is suitable for a bounded exact
formation set but can scale poorly in large open spaces. Continuous-time CBS
adapts that approach to geometry-aware unsafe intervals and safe-interval
planning. Large-neighbourhood repair can later be used as a bounded,
non-certifying search accelerator, but its result still needs the same exact
conflict check before execution.

These are design inputs, not evidence that Chiyoda predicts human movement:

- [Continuous-time Conflict-Based Search (IJCAI 2019)](https://www.ijcai.org/Proceedings/2019/0006.pdf)
- [Improving Continuous-time Conflict-Based Search (AAAI 2021)](https://cdn.aaai.org/ojs/17338/17338-13-20832-1-2-20210518.pdf)
- [MAPF-LNS2 (AAAI 2022)](https://ojs.aaai.org/index.php/AAAI/article/download/21266/21015)
