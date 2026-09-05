# Chiyoda language reference 0.28

Each non-empty line is one declaration. Lines beginning with `#` are comments.
Quoted strings are supported only where explicitly shown. All lengths use
metres, all durations use seconds or milliseconds, and all speeds use metres
per second. The compiler rejects unitless values for physical quantities.

`chiyoda format SOURCE` renders canonical source to standard output;
`chiyoda format SOURCE --check` makes canonical formatting a CI-verifiable
source invariant.

## Grammar

```text
scenario "NAME"
seed UNSIGNED_INTEGER
duration DURATION
timestep DURATION

surface ID at (LENGTH, LENGTH, LENGTH) size (LENGTH, LENGTH)
obstacle ID on SURFACE at (LENGTH, LENGTH, LENGTH) size (LENGTH, LENGTH)
waypoint ID on SURFACE at (LENGTH, LENGTH, LENGTH) [dwell DURATION]
exit ID on SURFACE at (LENGTH, LENGTH, LENGTH) width LENGTH [capacity RATE]
stair ID from SURFACE at (LENGTH, LENGTH, LENGTH) to SURFACE at (LENGTH, LENGTH, LENGTH) width LENGTH [capacity RATE|clearance LENGTH]...
ramp ID from SURFACE at (LENGTH, LENGTH, LENGTH) to SURFACE at (LENGTH, LENGTH, LENGTH) width LENGTH [capacity RATE|clearance LENGTH]...
escalator ID from SURFACE at (LENGTH, LENGTH, LENGTH) to SURFACE at (LENGTH, LENGTH, LENGTH) width LENGTH belt SPEED [capacity RATE|clearance LENGTH]...
lift ID from SURFACE at (LENGTH, LENGTH, LENGTH) to SURFACE at (LENGTH, LENGTH, LENGTH) cabin LENGTH LENGTH capacity UNSIGNED_INTEGER cycle DURATION [clearance LENGTH]
portal-lanes ID (connector|exit|gate) RESOURCE axis (x|y) count UNSIGNED_INTEGER
queue-footprint ID (connector|exit|gate) RESOURCE on SURFACE from (LENGTH, LENGTH, LENGTH) to (LENGTH, LENGTH, LENGTH) slots UNSIGNED_INTEGER
queue-grid ID (connector|exit|gate) RESOURCE on SURFACE from (LENGTH, LENGTH, LENGTH) to (LENGTH, LENGTH, LENGTH) width LENGTH lanes UNSIGNED_INTEGER slots UNSIGNED_INTEGER
connector-state ID connector CONNECTOR (open|closed) time DURATION
exit-state ID exit EXIT (open|closed) time DURATION
connector-capacity-state ID connector CONNECTOR capacity RATE time DURATION
exit-capacity-state ID exit EXIT capacity RATE time DURATION
gate ID on SURFACE at (LENGTH, LENGTH, LENGTH) width LENGTH capacity RATE to EXIT
gate-state ID gate GATE (open|closed) time DURATION
gate-capacity-state ID gate GATE capacity RATE time DURATION

agents ID count UNSIGNED_INTEGER on SURFACE at (LENGTH, LENGTH, LENGTH) to EXIT speed SPEED radius LENGTH height LENGTH [via WAYPOINT]... [alternative EXIT]... [exclude (stair|ramp|escalator|lift)]... [release DURATION [every DURATION [batch UNSIGNED_INTEGER]]]

message ID source (peer|official|signage|staff) on SURFACE at (LENGTH, LENGTH, LENGTH) claim (connector CONNECTOR|exit EXIT|gate GATE) (open|closed) truth (true|false) time DURATION reach LENGTH trust PROBABILITY [sample ID]
countermeasure ID corrects MESSAGE source (official|signage|staff) on SURFACE at (LENGTH, LENGTH, LENGTH) time DURATION reach LENGTH trust PROBABILITY [sample ID]
```

`LENGTH` is a finite number with an `m` suffix. `DURATION` is a finite number
with an `s` or `ms` suffix. `SPEED` has an `m/s` suffix. `RATE` has a `/s`
suffix. `PROBABILITY` is a finite decimal in `[0, 1]`.

`release` is optional and defaults to `0s`. It schedules the earliest time when
a declared group may become active; it is an authored demand input rather than
an arrival-rate fit. At or after that time, the runtime admits an agent only
when its generated spawn disc is dynamically clear of already active discs on
the same surface. A blocked agent remains `waiting_to_depart`, emits one
`agent_release_deferred_for_clearance` audit event, and is retried at each later
integration boundary. Its later `agent_released` event records actual admission
time, not the authored eligibility time.
`release T every I` releases ordinal group agents deterministically at `T`,
`T + I`, and so on. `release T every I batch N` releases at most `N` agents at
each of those instants: agents 0 through `N - 1` release at `T`, the next `N`
at `T + I`, and so on. Omitting `batch` means one agent per instant; omitting
`every` retains simultaneous release of the whole group. A batch requires an
`every` clause and must be positive. The final batch release must fall within
scenario duration. This is a declared schedule, not an inferred arrival
process or stochastic demand fit.
Each `via` adds an ordered required waypoint stage before the final exit.
Waypoint `dwell` defaults to `0s`; when declared, it holds an agent after the
stage before it may begin the next one.
The `to` exit and every `alternative` exit are final-stage candidates. At route
creation and each existing reroute trigger, the runtime selects the candidate
with the shortest currently feasible nominal route; source order breaks exact
ties. An alternative must be a distinct declared exit and, like the primary
exit, must be statically reachable after every required waypoint. This is a
transparent routing rule, not an inferred exit-preference model. When an exit
has a gate, its nominal route includes the walk to the selected gate and from
that gate to the exit, but does not forecast gate service queues.
Each `exclude` is a hard route constraint on one connector class. Omission
permits all connector classes; it does not assign an inferred mobility,
disability, or accessibility profile. Duplicate exclusions are rejected and
canonical formatting writes them in connector-class order.
Exit and non-lift connector `capacity` are optional and default to unlimited
throughput. When declared, each is an authored people-per-second service limit;
width is never silently converted to a flow rate.
`portal-lanes` is optional. It partitions the named connector, exit, or gate's
already authored width into `count` equal centre-line target lanes on global
surface-coordinate `axis`. The declaration is spatial placement only: it does
not create a service rate, infer a queue discipline, reserve a queue footprint,
or calibrate a facility. The compiler requires one declaration at most for a
resource, a positive count, a known resource, and every lane centre to fit the
largest authored agent radius within its surface and clear of obstacles. A
connector declaration applies to both of that connector's portals. At runtime,
an agent's lane is selected deterministically from its generated identifier and
the lane declaration identifier. An agent travelling through a lane-authored
connector remains in transit at the destination portal until its exact lane is
clear; it is never projected into a different open position. Omitting the
declaration retains the legacy single-point target and endpoint-clearance rule.
`queue-footprint` is optional and independent of `portal-lanes`. It declares
FIFO standing-slot centres on the source surface of one lift, one non-lift
connector with an authored `capacity`, one gate, or one exit with an authored
`capacity`.
`from` is the front slot nearest service and `to` is the tail slot; `slots`
evenly spaces every intermediate centre on that authored spine. The compiler
requires one footprint at most for a resource, a positive slot count at least
as large as the scenario's total declared agents, source-surface matching,
radius-safe slot spacing, boundary and obstacle clearance for every slot, and a
spine that clears expanded obstacles. These strict bounds reject an unlocated
overflow rather than silently placing excess agents somewhere else.
For a line `queue-footprint`, service denial retains the established behavior:
the agent receives a deterministic ticket, records queue entry, and walks to
its ranked slot. A `queue-grid` instead preallocates a deterministic ticket and
assigned standing slot before its on-surface motion is planned. A grid records
queue entry only after physical arrival at that slot; an unarrived ticket is
neither a queue wait nor an admission event. The head slot alone may reserve
the resource: it consumes an existing authored service token for a non-lift
connector, gate, or exit, while for a lift it reserves one cabin place. The
resulting `queue_service_reserved` event precedes that agent's physical
approach to its portal. A lift reservation occupies capacity through boarding
and transit, and is relinquished if an on-surface reroute changes its next
resource before boarding. This adds no inferred rate, queue discipline, staff
model, or calibration claim. Omitting the declaration retains the legacy
abstract queue at that resource.
`queue-grid` is the multi-lane form of an authored queue footprint. `from` is
rank zero, the exact front standing position; `to` fixes the depth direction
of the first lane. `lanes` must be at least two. The runtime fills each lane
from front to back, then reverses direction in the next lane, creating one
serpentine FIFO path: every rank advances to its immediate predecessor's
previous standing position, including at a lane turn. `width` is the
front-lane-centre to final-lane-centre span on the deterministic perpendicular
side of the `from`-to-`to` direction. The compiler requires enough slots for
every declared agent, safe longitudinal and lateral spacing for the largest
authored radius, clearance of every grid position, and clearance of every
adjacent FIFO transition. A grid remains authored geometry and queue order;
it does not infer capacity, demand, staffing, observed behaviour, or a safety
claim. For local motion within the same explicit grid only, an earlier ticket
has right-of-way over later tickets; the later ticket accepts the full
inside-obstacle avoidance correction. This deterministic queue-formation rule
does not extend to line footprints or unrelated pedestrians, and it is not a
collision, queue-discipline, or behavioral-fidelity guarantee.
The run trace distinguishes capacity waits at lifts, non-lift connectors, gates,
and final exits. Current run bundles also report discrete per-mechanism queue
exposure, cumulative reference-step wait, and step-boundary peak telemetry;
none is an inferred physical queue or measured flow.
`connector-capacity-state` and `exit-capacity-state` change a resource's
authored rate at a declared time; `gate-capacity-state` does the same for a
gate. The referenced non-lift connector or exit must already have an authored
baseline capacity, and every scheduled rate must be positive. A capacity state
does not derive a rate from geometry or make a claim about staffing, demand,
queue discipline, or facility operations. Same-time capacity states apply in
declaration order.
Connector `clearance` is optional and defaults to unlimited height. When
declared, a connector may only be used by an agent whose authored `height` is
at most that clearance. A non-lift connector may declare one `capacity` and one
`clearance` in either order; canonical formatting writes capacity first.

`trust` is the per-recipient probability of accepting an information event. The
reference runtime derives a deterministic sample from the scenario seed, agent
identifier, and intervention identifier, so replay remains exact. `sample` may
replace that identifier with an explicit, globally unique draw-stream key. It
exists so two authored comparison arms can keep matched trust draws despite
renaming the intervention; it is not a model of repeated measurements or a
claim about paired human behavior. Omission preserves the identifier stream.

Every connector, exit, and gate is physically open by default.
`connector-state`, `exit-state`, and `gate-state` change physical availability;
capacity-state declarations change only service tokens, not availability or
route choice. A closed gate excludes that gate from final-exit planning. Agents
already processed by a gate continue to its exit; agents that have not passed a
closed gate reroute to an open gate or an alternative exit, or wait for a route.
Events at `0s` establish the initial state, and same-time declarations of one
kind apply in declaration order. A `message` records what a recipient may
believe about a connector, final exit, or gate, whereas an availability state
declaration records what can actually be boarded or used. The compiler checks
that `truth true` agrees with the claimed resource's authored physical state at
the message time and that `truth false` disagrees. A belief never overrides a
physical closure. An accepted gate-availability belief excludes that one gate
from the agent's selected final-exit plan; another available gate for the same
exit remains selectable.

An accepted exit-availability belief is also excluded from that recipient's
final-exit selection; a qualifying countermeasure resets the belief to the
current physical state. An exit closure recomputes on-surface routes just like
a connector state change. Exit-state changes do not interrupt an agent already
marked evacuated.

## Static checks

The compiler enforces globally unique identifiers; positive geometry,
durations, speeds, widths, rates, and capacities; in-surface coordinates;
obstacle extents; statically clear exit, connector, gate, message, queue
footprint, and every
deterministic agent spawn (including each spawn's navigation-radius clearance);
exit and connector references; gate destinations; availability- and
capacity-state times and resource contracts; message truth labels against the
claimed resource's authored physical state; an agent-height- and connector-
eligibility-aware directed surface path from every agent group through every
required waypoint stage and to every declared final exit candidate; message
timing; and countermeasure references and ordering.

`countermeasure` is a correction of a declared falsehood, so it may only
reference a `message` with `truth false` and may not precede that message. This
is a source-level consistency check, not a claim that staff behavior or
messaging effects are empirically validated.

## Canonical IR

Successful compilation emits a JSON `CanonicalScenario` with
`language_version: "0.27"`. Declaration order is preserved and forms part of
the deterministic execution contract. The canonical IR is the public boundary
between conforming compilers and runtimes; direct use of parser internals is
not a stable API.

## Current geometry boundary

Version 0.27 supports axis-aligned rectangular walkable surfaces with
axis-aligned rectangular no-go zones, joined by directed 3D stairs, ramps,
escalators, and lifts. The runtime expands no-go zones by each agent radius
and finds a deterministic Euclidean shortest path through the resulting
visibility graph. A ramp or stair has nominal transit time equal to its 3D
endpoint distance divided by the declared agent speed. An escalator adds its
declared directed belt speed to that walking speed; the reference model
therefore represents walking riders, not a standing queue. Lift transit time
is its declared cycle. These are explicit analytical assumptions, not
calibrated human-performance parameters.

Agent radius drives deterministic reciprocal local motion during ordinary
movement and waypoint arrival, as well as obstacle clearance. Before a run, all
generated spawn discs on one surface must clear one another by their combined
radii plus the navigation epsilon, even if their authored release times differ.
Each step derives preferred velocities from the visibility-graph path, selects
speed-bounded ORCA velocities from one immutable position snapshot, and clips
each resulting segment against static geometry. Within the same preallocated
queue grid, a later FIFO ticket receives an earlier ticket's selected same-step
velocity for its full-responsibility constraint; all other pair selection stays
snapshot based. Exact co-location has a stable identifier-based tie rule, not a
random sample. The 2.5-second interaction horizon and its candidate screen are
uncalibrated reference assumptions; they do not establish a physical-collision
guarantee or a crowd model. Optional `portal-lanes`
declarations give connector portals and gate/exit targets explicit lane-centre
geometry. Optional `queue-footprint` declarations additionally give eligible
service waits a finite, authored FIFO standing spine. They do not turn service
tokens into inferred physical throughput, staffing, or a calibrated packing
model. Resources without the declaration retain their legacy single-point
target and deterministic endpoint-clearance placement.
Agent height filters only explicitly declared connector clearances; the runtime
does not model ceilings, body posture, or general 3D collision volumes. General
meshes, non-rectangular or moving obstacles, BIM/IFC imports,
standing-on-escalator behavior, and articulated gait remain outside the language
boundary.
