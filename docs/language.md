# Chiyoda language reference 0.1

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
exit ID on SURFACE at (LENGTH, LENGTH, LENGTH) width LENGTH
stair ID from SURFACE at (LENGTH, LENGTH, LENGTH) to SURFACE at (LENGTH, LENGTH, LENGTH) width LENGTH
lift ID from SURFACE at (LENGTH, LENGTH, LENGTH) to SURFACE at (LENGTH, LENGTH, LENGTH) cabin LENGTH LENGTH capacity UNSIGNED_INTEGER cycle DURATION
gate ID on SURFACE at (LENGTH, LENGTH, LENGTH) width LENGTH capacity RATE to EXIT

agents ID count UNSIGNED_INTEGER on SURFACE at (LENGTH, LENGTH, LENGTH) to EXIT speed SPEED radius LENGTH height LENGTH

message ID source (peer|official|signage|staff) on SURFACE at (LENGTH, LENGTH, LENGTH) claim connector CONNECTOR (open|closed) truth (true|false) time DURATION reach LENGTH trust PROBABILITY
countermeasure ID corrects MESSAGE source (official|signage|staff) on SURFACE at (LENGTH, LENGTH, LENGTH) time DURATION reach LENGTH trust PROBABILITY
```

`LENGTH` is a finite number with an `m` suffix. `DURATION` is a finite number
with an `s` or `ms` suffix. `SPEED` has an `m/s` suffix. `RATE` has a `/s`
suffix. `PROBABILITY` is a finite decimal in `[0, 1]`.

## Static checks

The compiler enforces globally unique identifiers; positive geometry,
durations, speeds, widths, rates, and capacities; in-surface coordinates;
exit and connector references; a directed surface path from every agent group
to its declared exit; message timing; and countermeasure references.

`countermeasure` is a correction of a declared falsehood, so it may only
reference a `message` with `truth false`. This is a source-level consistency
check, not a claim that staff behavior or messaging effects are empirically
validated.

## Canonical IR

Successful compilation emits a JSON `CanonicalScenario` with
`language_version: "0.1"`. Declaration order is preserved and forms part of
the deterministic execution contract. The canonical IR is the public boundary
between conforming compilers and runtimes; direct use of parser internals is
not a stable API.

## Current geometry boundary

Version 0.1 supports axis-aligned rectangular walkable surfaces joined by
directed 3D stair or lift connectors. Agents have radius and height metadata
and move as volume-aware capsules on those surfaces. General meshes, ramps,
arbitrary floor plans, BIM/IFC imports, and articulated gait are intentionally
not supported yet; they require separate semantics and empirical validation.
