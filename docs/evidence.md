# Evidence and claim boundary

Chiyoda is an open research alpha. Its current evidence consists of language
conformance, deterministic-runtime tests, and a source-locked descriptive
trajectory intake path. There are **no calibrated population profiles, facility
models, empirical benchmark scores, or operational claims** in this repository.

## Acquired-source boundary

An open dataset is not an empirical result. Before any measurement can be
described, Chiyoda records it in a versioned evidence catalog with the
publisher's license and checksum, an exact local byte size and SHA-256 content
lock, an explicit calibration/held-out role, and a transformation statement.
Raw sources live under `data/raw/`, which is intentionally ignored by Git.
Derived reports name every source digest and must be reproducible from that
catalog.

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

Acquire and lock the source without putting it in a commit:

```console
$ PYTHONPATH=python/src python3 -m chiyoda_analysis.evidence_cli fetch \
    benchmarks/evidence/eindhoven-centraal-platform-2024.json
$ cargo run -p chiyoda -- evidence lock \
    benchmarks/evidence/eindhoven-centraal-platform-2024.json
```

The Rust command is the project-native independent content-lock check. The
Python fetcher writes a temporary sibling file and atomically installs it only
after the byte count and SHA-256 match. It will refuse to overwrite an existing
file whose content does not match the catalog.

## Descriptive intake report

After a source lock, the following command streams the Parquet files in bounded
batches and writes a report. It computes consecutive-observation horizontal
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
future runtime change is allowed to use.

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
