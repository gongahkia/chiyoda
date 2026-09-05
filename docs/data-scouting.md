# Public-data scouting register

This register documents the intake decision, rather than treating every public
dataset as interchangeable evidence. It was reviewed on 2026-09-04. A listed
source is not a validated Chiyoda model component unless it appears in a
content-locked catalog and passes the later protocol review.

| Source | License/access verified | Decision | Reason |
| --- | --- | --- | --- |
| [Eindhoven Centraal platform trajectories](https://zenodo.org/records/13784588) | CC BY 4.0; direct public files | acquired; cataloged | 60 days of anonymous 10 Hz platform trajectories at a transit station. Supports descriptive horizontal platform-walking analysis only. |
| [VRU Trajectory Dataset](https://zenodo.org/records/6303669) | CC BY 4.0; direct public file | acquired; source-only cataloged | Its pedestrian data comes from an urban intersection, not a transit interchange. It is content-locked as `uncalibrated_reference` for disclosed structural exploration, never transit/stair calibration or benchmark evidence. |
| [Wuppertal controlled crowd-queue trajectories](https://ped.fz-juelich.de/da/crowdqueue) | CC BY 4.0 declared on the dataset page; direct public HTTPS ZIP | acquired; source-only cataloged | 24 controlled university entrance runs with published 25 Hz 2D trajectories through one fixed 0.5 m gate. The adapter reports disclosed gate-crossing alternatives only; it is not a station-capacity or queue-model validation source. |
| [OpenStreetMap Eindhoven Centraal station-area extract](https://api.openstreetmap.org/api/0.6/map?bbox=5.4785%2C51.4405%2C5.4810%2C51.4435) | ODbL 1.0; attribution required | acquired; source-only cataloged | The 2026-09-05 API XML snapshot is content-locked by [`eindhoven-centraal-layout-osm-2026.json`](../benchmarks/evidence/eindhoven-centraal-layout-osm-2026.json). The layout workflow preserves recognized geographic tags and selected way-node sequences; with an explicit local origin it produces a reproducible east/north authoring reference. It does not turn optional/incomplete map tags into facility geometry, capacity, accessibility, or calibrated claims. |
| [RAWPED](https://zenodo.org/records/3741742) | restricted; expressly non-commercial and no redistribution | rejected | It violates Chiyoda's public redistribution requirement. |
| [SiT](https://github.com/SPALaboratory/SiT-Dataset) | CC BY-NC-ND 4.0 | rejected | The non-commercial/no-derivatives terms are incompatible with a redistributable Apache-2.0 benchmark corpus. |
| [Pedestrian Dynamics Data Archive](https://ped.fz-juelich.de/da/) | archive-wide CC BY 4.0 notice | one dataset acquired; others pending dataset-specific review | The Wuppertal crowd-queue source now meets the full source-only intake contract. No other archive dataset is admitted until its own files, protocol, schema, citation, and license statement are separately reviewed and content-locked. |

## Selection rule

To enter an `empirical_evaluation` catalog under `benchmarks/evidence/`, a
source must have all of the following:

1. a direct, stable HTTPS file URL and a license allowing redistribution;
2. published measurement and coordinate semantics sufficient to state a narrow
   claim boundary;
3. a practical, documented split unit that prevents observation/trajectory
   leakage; and
4. a source-specific adapter with exact source hashes and a testable
   transformation.

Data which merely has an open landing page, is non-commercial, requires an
account or private request, lacks a per-dataset license, or cannot support the
declared primitive is not admitted. This is intentionally stricter than
"downloadable."

An `uncalibrated_reference` catalog has the same direct-file, licensing, content
lock, coordinate-semantics, and transformation requirements, but does not invent
a calibration/held-out split. It is appropriate for provenance-preserving
assumption discovery and structural experiments; it is not a back door into an
empirical claim.
