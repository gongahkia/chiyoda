# Benchmark materials

`public-fixtures.json` declares structural fixture seeds for the bundled
generator. They are deliberately not presented as an empirical benchmark.

Create a candidate fixture with:

```console
$ chiyoda generate --seed 73 -o fixture.chy
```

An empirical round is added only with a validated manifest and public data as
defined in [the benchmark protocol](../docs/benchmark.md).

`evidence/` also contains source-only, `uncalibrated_reference` catalogs. They
content-lock openly licensed inputs for transparent structural work, but are not
benchmark datasets and cannot be used by an empirical calibration adapter.
The current VRU source has a matching descriptive adapter:

```console
$ chiyoda reference vru-trajectory benchmarks/evidence/vru-trajectory-2022.json \
    -o out/vru-trajectory-reference.json
$ chiyoda reference crowd-queue benchmarks/evidence/wuppertal-crowdqueue-2018.json \
    -o out/wuppertal-crowdqueue-reference.json
```
