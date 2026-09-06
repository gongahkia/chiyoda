# Evidence and claim boundary

Chiyoda is an open research alpha. Its current evidence consists of language
conformance, deterministic-runtime tests, and a source-locked descriptive
trajectory intake path. There are **no calibrated population profiles, facility
models, empirical benchmark scores, or operational claims** in this repository.

## Acquired-source boundary

An open dataset is not an empirical result. Before any measurement can be
described, Chiyoda records it in a versioned evidence catalog with the
publisher's license and any checksum it exposes, an exact local byte size and
SHA-256 content lock, and a transformation statement. A catalog has one of two explicit
purposes:

- `empirical_evaluation` is the strict historical contract: every file has a
  calibration or held-out role and the catalog explains the leakage boundary.
- `uncalibrated_reference` content-locks an open source without pretending it
  has an evaluation split. It cannot be passed to a calibration adapter or used
  as benchmark evidence.

Raw sources live under `data/raw/`, which is intentionally ignored by Git.
Derived reports name every source digest and must be reproducible from that
catalog.

The catalog accepts redistributable `CC-BY-4.0` and `ODbL-1.0` source metadata.
`ODbL-1.0` is limited to `uncalibrated_reference` and must state the required
attribution; it cannot become an empirical-evaluation catalog. This supports
source-linked map observation without weakening the public empirical evidence
contract. See [open-layout source observations](layout-sources.md).

For a one-off uncalibrated scenario informed by any acquired source, use an
[experiment artifact](experiments.md) to snapshot the scenario, assumptions,
source-report bytes, and claim boundary with the run bundle. This is provenance
control, not calibration or empirical evaluation.

The first catalog is
[`benchmarks/evidence/eindhoven-centraal-platform-2024.json`](../benchmarks/evidence/eindhoven-centraal-platform-2024.json).
It locks six CC BY 4.0 Parquet files published by Pouw, van der Vleuten,
Corbetta, and Toschi on Zenodo. They contain anonymous 10 Hz `(time, id, x, y)`
trajectories from one Eindhoven Centraal platform over 60 consecutive days.
Files covering days 01–30 are designated calibration and days 31–60 held-out.
This temporal, file-level partition prevents row and trajectory leakage between
the two roles, but it is not independent-site validation.

The catalog supports only descriptive analysis of horizontal platform walking.
It does **not** support claims about a whole station, stairs, lifts, gates,
bodies, routing, information effects, accessibility, or evacuation outcomes.
The full source and the source's CC BY 4.0 license are the authoritative record:
[Zenodo record 13784588](https://zenodo.org/records/13784588).

The second catalog,
[`benchmarks/evidence/vru-trajectory-2022.json`](../benchmarks/evidence/vru-trajectory-2022.json),
locks the publisher's 4.5 MB CC BY 4.0 archive of pedestrian and cyclist
intersection trajectories. It is deliberately `uncalibrated_reference`: its
urban-intersection setting is useful for disclosed structural exploration but
does not supply a station split or authorize calibration. Its publisher record
is [Zenodo record 6303669](https://zenodo.org/records/6303669).

Acquire and lock the source without putting it in a commit:

```console
$ PYTHONPATH=python/src python3 -m chiyoda_analysis.evidence_cli fetch \
    benchmarks/evidence/eindhoven-centraal-platform-2024.json
$ cargo run -p chiyoda -- evidence lock \
    benchmarks/evidence/eindhoven-centraal-platform-2024.json
$ cargo run -p chiyoda -- evidence lock \
    benchmarks/evidence/vru-trajectory-2022.json
```

The Rust command is the project-native independent content-lock check. The
Python fetcher writes a temporary sibling file and atomically installs it only
after the byte count and SHA-256 match. It will refuse to overwrite an existing
file whose content does not match the catalog.

`calibrate eindhoven-platform` rejects an `uncalibrated_reference` catalog even
when its files are content-locked. This preserves a hard line between acquiring
open data for transparent assumptions and claiming that an adapter can evaluate
the runtime.

## Uncalibrated VRU reference report

The VRU archive has a source-specific descriptive adapter because its publisher
format is a gzip-compressed tar archive of one CSV trajectory per file, rather
than Eindhoven's Parquet schema. Run it only after locking the catalog:

```console
$ cargo run -p chiyoda -- reference vru-trajectory \
    benchmarks/evidence/vru-trajectory-2022.json \
    -o out/vru-trajectory-reference.json
```

The adapter verifies the archive's byte size and SHA-256 before it reads it. It
then reads only archive entries under `VRU_dataset/pedestrians/`, validates the
published `timestamp`, `x`, and `y` CSV schema, and emits row counts, publisher
motion-directory counts, and a fixed-filter speed summary. A 100 ms gap limit,
4 m/s upper filter, and 0.01 m/s histogram quantiles are recorded in the
report; rejected intervals remain counted rather than silently removed.

The report status is `uncalibrated_reference_only`. Its values can help choose
and disclose broad structural sensitivity alternatives, but cannot become a
runtime default, a station calibration, a population estimate, a benchmark
score, or a predictive/safety claim.

The checked-in
[`vru-trajectory-reference.json`](../benchmarks/reports/vru-trajectory-reference.json)
is regenerated from the content-locked archive with the current adapter. The
source-linked walking-speed sensitivity example records this report's SHA-256
and snapshots its exact JSON inside each executed study.

## Uncalibrated Wuppertal bottleneck reference report

The third catalog,
[`benchmarks/evidence/wuppertal-crowdqueue-2018.json`](../benchmarks/evidence/wuppertal-crowdqueue-2018.json),
locks the publisher's 25 Hz text trajectories from 24 controlled university
entrance runs. The source page describes a fixed 0.5 m entry gate at `y = 0 m`;
corridor width, priming, motivation, and participant count vary across runs.
The site declares CC BY 4.0 and the public source is available from the
[Pedestrian Dynamics Data Archive](https://ped.fz-juelich.de/da/crowdqueue).

Run the source-specific adapter only after locking the catalog:

```console
$ cargo run -p chiyoda -- reference crowd-queue \
    benchmarks/evidence/wuppertal-crowdqueue-2018.json \
    -o out/wuppertal-crowdqueue-reference.json
```

The adapter verifies the ZIP size and SHA-256 before reading it. It accepts only
the publisher's root `RUN_PRIMING_WIDTH_MOTIVATION.txt` trajectory files,
requires the published five-column header, uses the declared 25 Hz frame rate,
and linearly interpolates each first positive-to-nonpositive `y = 0 m` crossing
within a 200 ms adjacent-frame limit. It reports each run and a direct,
descriptive per-run crossing-flow distribution. Gaps and speed-filtered steps
remain counted rather than disappearing.

The checked-in
[`wuppertal-crowdqueue-reference.json`](../benchmarks/reports/wuppertal-crowdqueue-reference.json)
contains 24 measured runs, 978 usable observed crossings, and a 1.1478 persons/s
per-run P50 under its fixed filter. These are descriptive source facts, not a
general exit-capacity law. The accompanying source-linked sensitivity example
uses its P05/P50/P95 values only as out-of-domain structural alternatives.

This source does not validate the reference runtime's service-token mechanism,
station exits, queues, route choice, information behavior, accessibility,
evacuation, population profile, or safety. It cannot become a calibration
source or an empirical benchmark by relabeling it.

## Descriptive intake report

After a source lock, the following command streams only the **calibration**
Parquet files in bounded batches and writes a report. It computes consecutive-observation horizontal
speeds after a declared 500 ms maximum inter-observation gap and 4 m/s upper
filter. Every rejected observation category is counted, and reported quantiles
are fixed 0.01 m/s histogram estimates rather than undocumented floating-point
sampling.

```console
$ cargo run -p chiyoda -- calibrate eindhoven-platform \
    benchmarks/evidence/eindhoven-centraal-platform-2024.json \
    -o out/eindhoven-platform-intake.json
```

The report has the hard-coded status `descriptive_only`: it does not change any
runtime parameter. A reviewable calibration protocol must first decide which
mechanism, parameterization, uncertainty model, and held-out prediction test a
future runtime change is allowed to use. Only after that protocol freezes a
candidate may the held-out role be read with `--partition held-out`; the default
command deliberately does not inspect it.

The exact pre-registration, leakage, and acceptance rules are in the
[calibration protocol](calibration-protocol.md).

## Opt-in free-walking speed profile

The descriptive intake remains separate from the runtime default. The only
implemented evidence-to-runtime bridge is an explicit three-stage
`free-walking-speed-profile` workflow defined in the
[calibration protocol](calibration-protocol.md): it fits a constant
`AgentGroup.speed_mps` input from days 01–30 after a strict endpoint screen
requiring exactly one tracked platform object per source frame and a fixed
0.5 m/s minimum walking speed, compares that scalar with the equivalently
screened locked days 31–60 P50, then emits a provenance-complete DSL
declaration. The result is opt-in, self-hashed, and embedded in canonical
scenario IR; a normal scenario continues to use its authored `speed SPEED`
literal.

The held-out status is intentionally
`held_out_comparison_complete_not_accepted`. It records a reproducible temporal
comparison, not an acceptance threshold or a claim of free-trajectory,
population, avoidance, queue, route-choice, connector, station, predictive,
operational, or safety validity.

The checked-in [profile artifact](../benchmarks/reports/eindhoven-free-walking-speed-profile.json)
stores 1.065 m/s from 750,141 retained calibration steps; the linked
[held-out artifact](../benchmarks/reports/eindhoven-free-walking-held-out.json)
reports a 1.075 m/s held-out P50 from 636,647 retained steps and a 0.010 m/s
absolute P50 difference. Those are disclosed measurements under the fixed
screen, not a pass threshold or a claim beyond this scalar input.

The checked-in [calibration descriptive report](../benchmarks/reports/eindhoven-platform-calibration-intake.json)
was regenerated from the locked days 01–30 files using adapter
`0.1.0-alpha.1`. It contains 470,779,630 retained consecutive observations;
its aggregate speed distribution is intentionally reported only as source
description, not a runtime default or validated human parameter.

## Requirements for an empirical benchmark round

Every public round must include a machine-readable manifest accepted by:

```console
$ chiyoda benchmark verify benchmarks/rounds/ROUND.json
```

The manifest requires:

- at least one calibration dataset and one held-out dataset;
- an openly redistributable license, stable source URL, SHA-256 digest, and
  documented transformation for every dataset;
- a versioned generator, public fixture seeds, a committed evaluation-seed
  hash, and a commitment to release the seeds after the round; and
- a plain-language statement of the supported population, facility primitives,
  metrics, uncertainty, and exclusions.

The validator deliberately rejects private or non-redistributable data. It
does not assess whether a dataset is scientifically adequate; that requires
peer review and the published calibration protocol.

## Population and accessibility boundary

No population profile is shipped until openly redistributable evidence supports
its parameters and held-out evaluation. This avoids presenting “illustrative”
mobility or disability behavior as empirical fidelity. The DSL has lift,
capacity, body-radius, and body-height primitives, but their presence is not a
claim of accessible-egress validation.

## Prohibited interpretation

Do not use Chiyoda outputs to certify buildings, direct evacuations, set
emergency procedures, assess a real facility’s vulnerability, or claim that a
countermeasure will improve safety. The project may support future research on
such questions only after scenario-specific empirical evidence and appropriate
governance.
