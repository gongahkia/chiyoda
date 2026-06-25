# Modeling Gaps

Chiyoda is a research simulator for replayable evacuation and information-control studies. It should not be presented as an operational evacuation predictor without scenario-specific validation.

## Current Gaps

| Area | Gap | Current handling |
|:--|:--|:--|
| Real-station prediction | No station case is validated against drill, incident, or expert-coded operational evidence. | Report-facing station cases must record provenance and validation-use limits. |
| Pedestrian dynamics | Social-force and bottleneck behavior are grid-scale approximations. The Wuppertal proxy still differs from the lab reference flow. | Juelich width checks and Wuppertal comparison are regression/diagnostic workflows, not certification. |
| Hazard physics | Built-in gas, smoke, fire, flood, quake, shooter, wildfire, and ember fields are stylized. | External scalar hazard fields can be imported; default hazards remain stylized unless a scenario includes a reference field. |
| Smoke/FDS agreement | The FDS check preserves imported scalar concentration and visibility values only. | It does not validate transient CFD transport or two-way coupling. |
| Scenario validation | Static validation checks topology, starts, exits, and reachability. | It does not prove behavioral plausibility, calibration quality, hazard realism, or source-data completeness. |
| Geometry import | OSM/GTFS/GeoJSON conversion is pragmatic and raster-oriented. | It is not a standards-complete indoor mapper and cannot prove imported station topology is complete. |
| Vertical transport | Stairs, ramps, and escalators are weighted graph edges; elevators are capacity/dwell/travel-time holds. | There is no physical elevator dispatch, door state, car position, or detailed queue discipline. |
| Viewer preview | Browser-side authoring/preview is not the reference simulation engine. | Exported scenarios should be validated and rerun through `chiyoda.cli`. |
| Information-safety metrics | HCI and static frontier checks are internal diagnostics. | They flag plausible harmful-convergence regimes; they do not predict exact field outcomes. |
| Causal comparison | Matched-seed deltas inherit bias from priors and stylized hazards. | Bundles expose metadata, bootstrap intervals, and sensitivity outputs, but do not prove causal transport. |
| Hostile channels and LLMs | Attacker objectives, plausibility, credibility decay, and generated messages are abstract controls. | Replay caches and audits make runs reproducible; empirical calibration remains scenario-specific. |

## Promotion Checklist

Before calling a scenario externally validated, add evidence for the specific claim:

- trajectory/drill/incident/expert-coded reference data for the geometry and population;
- calibrated pedestrian-flow parameters for the density and bottleneck regime;
- imported or cross-checked hazard fields when hazard physics matters;
- documented source topology, manual edits, exits, connectors, spawns, and missing data;
- sensitivity runs over seeds, priors, hazards, and intervention timing;
- replayable exported bundle with hashable inputs and validation notes.
