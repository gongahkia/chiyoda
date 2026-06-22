# Hazard Field Import

Chiyoda's built-in gas, smoke, fire, and crush models are stylized simulation
components. They should not be described as validated hazard physics unless a
scenario is explicitly cross-checked against an external FDS run or published
gas/smoke reference field.

Imported hazard fields provide that comparison path without changing the core
simulation loop. A scenario hazard can point to a JSON or CSV grid:

```yaml
hazards:
  - type: SMOKE
    field:
      file: hazards/reference_smoke_field.json
```

JSON format:

```json
{
  "kind": "SMOKE",
  "origin": [0.0, 0.0],
  "cell_size": 1.0,
  "intensity": [[0.0, 0.2], [0.0, 0.8]],
  "visibility": [[1.0, 0.9], [1.0, 0.4]],
  "source": {
    "name": "reference field name",
    "license": "source license",
    "url": "https://example.invalid/reference"
  }
}
```

CSV format requires `x`, `y`, and `intensity` columns, plus optional
`visibility`. Imported fields affect:

- agent hazard exposure and physiology,
- information visibility,
- route penalties through `hazard_penalty_at_cell`.

This is an import and regression-test path only. External summaries should
continue to describe the default hazards as stylized until a concrete reference
field and matching scenario are included as a validation artifact.

## Worked Example: FDS Smoke Slice → Imported Field

A tiny synthetic FDS-style smoke slice ships in
`tests/fixtures/fds_smoke_slice.csv`. The format mirrors what a slice file
preprocessor would emit: one row per `(x, y)` cell with `intensity` and
`visibility` columns. Origin is `(0, 0)` and cell size is 1.0 m.

```csv
x,y,intensity,visibility
0,0,0.05,0.95
1,0,0.10,0.90
...
```

Load it and inspect the resulting field directly:

```python
from chiyoda.environment.hazards import ImportedHazardField

field = ImportedHazardField.from_csv(
    "tests/fixtures/fds_smoke_slice.csv", kind="SMOKE"
)
print(field.severity, field.radius, field.intensity_grid.shape)
```

End-to-end CLI: reference the same CSV from a scenario YAML and run it
through the existing `run` command. A minimal scenario stanza:

```yaml
hazards:
  - type: SMOKE
    field:
      file: tests/fixtures/fds_smoke_slice.csv
      cell_size: 1.0
      origin: [0.0, 0.0]
```

```sh
.venv/bin/python -m chiyoda.cli run path/to/scenario.yaml -o out/fds_demo
```

The imported field is treated as a static hazard for the run; agent
hazard exposure, vision decay, and route hazard penalty all consult
`intensity_at` against the grid rather than the stylized spreading
`Hazard` model.
