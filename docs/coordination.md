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

The same module now also provides a bounded time-expanded search over an
explicitly supplied static roadmap. Its every wait and move action is checked
continuously against declared occupied trajectories, and it distinguishes an
infeasible result within that roadmap/time horizon from a search-bound
exhaustion. It is a low-level planning primitive, not yet an automatic
queue-grid executor. The initial state enumeration is intentionally explicit;
safe-interval compression remains a future optimisation.

`CoordinationRoadmap::lattice` derives one such bounded roadmap from a declared
surface, the declared obstacles, a reference-disc radius, a spacing, and exact
anchors. It rejects an unsafe anchor or node-bound exhaustion and accepts an
edge only when its entire radius-expanded segment is statically clear. The
caller must choose the lattice spacing and node bound; neither is calibrated or
derived from a facility.

A bounded deterministic conflict-repair tree is now layered over that search.
It begins with individual roadmap plans, selects the earliest remaining
continuous-time conflict, and branches by replanning either participant against
the other currently declared trajectories. A result is returned only after the
same exact conflict kernel finds no remaining conflicts. The finite roadmap,
time horizon, low-level bound, and conflict-tree bound mean this is not a
complete solver; bound exhaustion is an explicit unknown result.

The repair request can also carry prior accepted trajectories. This is the
handoff contract for rolling windows: each new cohort plans against all earlier
accepted reservations, and the request rejects an already-conflicted handoff
instead of trying to hide it. It now accepts either a conventional permanent
goal or a sequence of queue-rank windows through one shared repair tree, so
the two objective forms cannot acquire different conflict semantics. Runtime
integration still has to define cohort size and replanning cadence.

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

`plan_queue_grid_rolling` processes those tasks in deterministic FIFO cohorts.
Each cohort is checked against every earlier accepted trajectory, so this is a
bounded decomposition rather than an unchecked heuristic. Cohort size is an
explicit computational policy: smaller cohorts limit each conflict tree but
can rule out solutions that require revising an earlier accepted trajectory.
The runtime must report that policy with any result that uses it.

For exploratory work without qualifying service data,
`estimate_queue_grid_departures` derives those inputs from an explicit
`QueueGridServiceAssumption` (first completion and constant active-queue
headway). This is intentionally an uncalibrated scenario assumption, not a
capacity model or an observed service claim. If its schedule makes a physical
formation impossible, `plan_queue_grid` returns no plan rather than extending
or rewriting the assumed headway.

These primitives are not yet an execution policy or a safety certificate. They
must not silently turn an audit failure into a movement clamp: once two planned
segments are already incompatible, clamping only one after the other has
committed can create a deadlock or preserve an existing overlap.

## Chosen architecture

The coordinator will be a bounded, optional planning layer for an authored
queue grid, separate from generic ORCA:

1. Construct a statically clear roadmap for the affected surface, including
   release positions, authored grid slots, and obstacle-aware transitions.
2. Translate queue ticket activation and service-departure events into
   rank-window tasks, then use the time-expanded roadmap planner to produce a
   timed trajectory for each disc while respecting already declared trajectory
   constraints. Replace its explicit wait-state enumeration with safe-interval
   compression only after preserving the same window and conflict semantics.
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
