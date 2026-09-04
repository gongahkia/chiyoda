# Chiyoda language reference 0.14

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
connector-state ID connector CONNECTOR (open|closed) time DURATION
exit-state ID exit EXIT (open|closed) time DURATION
gate ID on SURFACE at (LENGTH, LENGTH, LENGTH) width LENGTH capacity RATE to EXIT

agents ID count UNSIGNED_INTEGER on SURFACE at (LENGTH, LENGTH, LENGTH) to EXIT speed SPEED radius LENGTH height LENGTH [via WAYPOINT]... [alternative EXIT]... [exclude (stair|ramp|escalator|lift)]... [release DURATION]

message ID source (peer|official|signage|staff) on SURFACE at (LENGTH, LENGTH, LENGTH) claim connector CONNECTOR (open|closed) truth (true|false) time DURATION reach LENGTH trust PROBABILITY
countermeasure ID corrects MESSAGE source (official|signage|staff) on SURFACE at (LENGTH, LENGTH, LENGTH) time DURATION reach LENGTH trust PROBABILITY
```

`LENGTH` is a finite number with an `m` suffix. `DURATION` is a finite number
with an `s` or `ms` suffix. `SPEED` has an `m/s` suffix. `RATE` has a `/s`
suffix. `PROBABILITY` is a finite decimal in `[0, 1]`.

`release` is optional and defaults to `0s`. It schedules when a declared group
becomes active; it is an authored demand input rather than an arrival-rate fit.
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
Connector `clearance` is optional and defaults to unlimited height. When
declared, a connector may only be used by an agent whose authored `height` is
at most that clearance. A non-lift connector may declare one `capacity` and one
`clearance` in either order; canonical formatting writes capacity first.

`trust` is the per-recipient probability of accepting an information event. The
reference runtime derives a deterministic sample from the scenario seed, agent
identifier, and intervention identifier, so replay remains exact.

Every connector is physically open by default. `connector-state` changes that
physical state; events at `0s` establish the initial state, and same-time
events apply in declaration order. A `message` records what a recipient may
believe, whereas a connector state records what can actually be boarded. The
compiler checks that `truth true` agrees with the connector's declared physical
state at the message time and that `truth false` disagrees. A belief never
overrides a physical closure.

Every exit is also physically open by default. `exit-state` changes its final
stage availability; a closed exit is excluded from final-exit selection, and
its closure recomputes on-surface routes just like a connector state change.
Exit-state changes do not interrupt an agent already marked evacuated.

## Static checks

The compiler enforces globally unique identifiers; positive geometry,
durations, speeds, widths, rates, and capacities; in-surface coordinates;
obstacle extents; unoccupied exit, connector, gate, every deterministic agent
spawn (including the navigation radius clearance), and message coordinate; exit
and connector references; connector-state and exit-state times; message truth labels against
the authored physical state; an agent-height- and connector-eligibility-aware
directed surface path from every agent group through every required waypoint
stage and to every declared final exit candidate; message timing; and
countermeasure references and ordering.

`countermeasure` is a correction of a declared falsehood, so it may only
reference a `message` with `truth false` and may not precede that message. This
is a source-level consistency check, not a claim that staff behavior or
messaging effects are empirically validated.

## Canonical IR

Successful compilation emits a JSON `CanonicalScenario` with
`language_version: "0.14"`. Declaration order is preserved and forms part of
the deterministic execution contract. The canonical IR is the public boundary
between conforming compilers and runtimes; direct use of parser internals is
not a stable API.

## Current geometry boundary

Version 0.14 supports axis-aligned rectangular walkable surfaces with
axis-aligned rectangular no-go zones, joined by directed 3D stairs, ramps,
escalators, and lifts. The runtime expands no-go zones by each agent radius
and finds a deterministic Euclidean shortest path through the resulting
visibility graph. A ramp or stair has nominal transit time equal to its 3D
endpoint distance divided by the declared agent speed. An escalator adds its
declared directed belt speed to that walking speed; the reference model
therefore represents walking riders, not a standing queue. Lift transit time
is its declared cycle. These are explicit analytical assumptions, not
calibrated human-performance parameters.

Agent radius drives local separation and obstacle clearance. Agent height
filters only explicitly declared connector clearances; the runtime does not
model ceilings, body posture, or general 3D collision volumes. General meshes,
non-rectangular or moving obstacles, BIM/IFC imports, standing-on-escalator
behavior, and articulated gait remain outside the language boundary.
