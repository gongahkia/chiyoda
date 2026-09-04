# Benchmark materials

`public-fixtures.json` declares structural fixture seeds for the bundled
generator. They are deliberately not presented as an empirical benchmark.

Create a candidate fixture with:

```console
$ chiyoda generate --seed 73 -o fixture.chy
```

An empirical round is added only with a validated manifest and public data as
defined in [the benchmark protocol](../docs/benchmark.md).

