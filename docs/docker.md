# Docker

The `Dockerfile` at the repo root is a two-stage build on `python:3.12-slim`
that pins dependencies from `requirements-lock.txt`.

## Build

```sh
docker build -t chiyoda .
```

## Run

The image's entrypoint is `python -m chiyoda.cli`. Show help:

```sh
docker run --rm chiyoda --help
```

Submit a benchmark policy run and mount an output directory:

```sh
mkdir -p out/benchmark_submission_docker
docker run --rm \
  -v "$(pwd)/out/benchmark_submission_docker:/app/out/benchmark_submission" \
  chiyoda benchmark submit --suite v1
```

Other suites:

```sh
docker run --rm chiyoda benchmark submit --suite v2
docker run --rm chiyoda benchmark submit --suite v3
```

## Notes

- The image installs from `requirements-lock.txt`, not `requirements.txt`,
  so behavior matches the verified `.venv`.
- The runtime stage does not include `pytest` or developer tooling. Use
  the local virtual environment for tests.
- Mount the working directory if you want to author or edit scenarios:
  `docker run --rm -v "$(pwd):/app" chiyoda validate-scenario scenarios/station_baseline.yaml`.
