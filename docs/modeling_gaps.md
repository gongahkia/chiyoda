# Modeling Gaps

Chiyoda is a research simulator for replayable evacuation and information-control studies. It should not be presented as an operational evacuation predictor without scenario-specific validation.

## Current Gaps

| Area | Gap | Current handling |
|:--|:--|:--|
| Real-station prediction | No station case is validated against drill, incident, trajectory, or expert-coded operational evidence. | `validation-evidence-audit` makes external evidence explicit; benchmark claim tiers downgrade when no operational evidence is recorded. |
| Pedestrian dynamics | Social-force and bottleneck behavior are grid-scale approximations. The Wuppertal proxy still differs from the lab reference flow. | `calibration-audit` exposes SFM profile/provenance and cohort bounds; Juelich/Wuppertal checks remain diagnostic, not certification. Local smoke visibility and flood depth now slow movement explicitly. |
| Hazard physics | Built-in gas, smoke, fire, flood, quake, shooter, wildfire, and ember fields are stylized. | `hazard-audit` labels imported vs stylized hazards; benchmark claim tiers downgrade stylized hazards. Flood depth affects routing cost and speed, but hydrology remains stylized. |
| Smoke/FDS agreement | The FDS check preserves imported scalar concentration and visibility values only. | It does not validate transient CFD transport or two-way coupling. |
| Scenario validation | Static validation checks topology, starts, exits, and reachability. | Runtime assertions now cover behavioral, hazard, vertical-transport, and hostile/LLM metrics; they remain regression checks, not external validation. |
| Geometry import | OSM/GTFS/GeoJSON conversion is pragmatic and raster-oriented. | It is not a standards-complete indoor mapper and cannot prove imported station topology is complete. |
| Vertical transport | Stairs, ramps, and escalators are weighted graph edges; elevators are capacity/dwell/travel-time holds. | Geometry audit flags under-specified elevators; runtime assertions cover aggregate elevator usage/queue/capacity metrics. |
| Viewer preview | Browser-side authoring/preview is not the reference simulation engine. | Viewer exports include a QA block plus opt-in Playwright canvas QA; exported scenarios should still be validated and rerun through `chiyoda.cli`. |
| Information-safety metrics | HCI and static frontier checks are internal diagnostics. | They flag plausible harmful-convergence regimes; they do not predict exact field outcomes. |
| Causal comparison | Matched-seed deltas inherit bias from priors and stylized hazards. | Bundles expose metadata, bootstrap intervals, and sensitivity outputs, but do not prove causal transport. |
| Hostile channels and LLMs | Attacker objectives, plausibility, credibility decay, and generated messages are abstract controls. | Taxonomy validation, runtime hostile/LLM assertions, replay caches, and LLM audit chains make behavior reproducible; empirical calibration remains scenario-specific. |

## Promotion Checklist

Before calling a scenario externally validated, add evidence for the specific claim:

- trajectory/drill/incident/expert-coded reference data for the geometry and population;
- `metadata.external_validation_evidence` records with source, scope, validation use, and optional file-backed artifacts;
- calibrated pedestrian-flow parameters for the density and bottleneck regime;
- imported or cross-checked hazard fields when hazard physics matters;
- documented source topology, manual edits, exits, connectors, spawns, and missing data;
- sensitivity runs over seeds, priors, hazards, and intervention timing;
- replayable exported bundle with hashable inputs and validation notes.

Useful checks:

```sh
.venv/bin/python scripts/audit_scenarios.py
.venv/bin/python -m chiyoda.cli validation-evidence-audit scenarios/station_sarin.yaml --json
```
