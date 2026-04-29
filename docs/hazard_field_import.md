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

This is an import and regression-test path only. The paper should continue to
describe the default hazards as stylized until a concrete reference field and
matching scenario are included as a validation artifact.
