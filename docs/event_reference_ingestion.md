# Event Reference Ingestion

Chiyoda treats evacuation drills, VR trials, incident reconstructions, and
expert-coded observations as external reference data. These records are loaded
for comparison and provenance review only; they are not consumed by simulation
execution and must not silently tune scenario behavior.

## Schema

Event references are YAML or JSON files with:

- `reference_id`: stable identifier for the reference artifact.
- `reference_type`: one of `drill`, `vr`, `incident`, or `expert_coded`.
- `provenance`: required source metadata.
- `observations`: timestamped or simulation-time-coded reference events.

The provenance block must include:

- `source`
- `license`
- `timestamp`
- `station`
- `scenario_assumptions`
- `known_missing_data`

It may also include `source_url`, `access_date`, `level`,
`coordinate_transform`, and `notes`.

## Example

```yaml
reference_id: example_drill_001
reference_type: drill
description: Small fixture showing the auditable reference schema.
provenance:
  source: "Example transit drill coding sheet"
  source_url: "https://example.invalid/drill"
  license: "CC-BY-4.0"
  timestamp: "2026-01-15T09:00:00Z"
  access_date: "2026-04-29"
  station: "Example Station"
  level: "concourse"
  coordinate_transform: "local fixture coordinates, meters"
  scenario_assumptions:
    - "Single-level evacuation comparison"
  known_missing_data:
    - "No individual identity or physiology observations"
observations:
  - event_id: first_queue
    label: queue_detected
    time_s: 42.0
    location: [12.5, 8.0]
    agent_count: 18
    confidence: 0.8
```

Use `chiyoda.references.load_event_reference(path)` to validate and load a
reference, then call `observations_frame()` and `provenance_frame()` to produce
tables for comparison notebooks or paper appendices.
