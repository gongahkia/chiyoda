# Public-data scouting register

This register documents the intake decision, rather than treating every public
dataset as interchangeable evidence. It was reviewed on 2026-09-04. A listed
source is not a validated Chiyoda model component unless it appears in a
content-locked catalog and passes the later protocol review.

| Source | License/access verified | Decision | Reason |
| --- | --- | --- | --- |
| [Eindhoven Centraal platform trajectories](https://zenodo.org/records/13784588) | CC BY 4.0; direct public files | acquired; cataloged | 60 days of anonymous 10 Hz platform trajectories at a transit station. Supports descriptive horizontal platform-walking analysis only. |
| [VRU Trajectory Dataset](https://zenodo.org/records/6303669) | CC BY 4.0; direct public files | acquired; excluded from current catalog | Its pedestrian data comes from an urban intersection, not a transit interchange. It may become an explicit out-of-domain robustness diagnostic, never transit/stair calibration. |
| [RAWPED](https://zenodo.org/records/3741742) | restricted; expressly non-commercial and no redistribution | rejected | It violates Chiyoda's public redistribution requirement. |
| [SiT](https://github.com/SPALaboratory/SiT-Dataset) | CC BY-NC-ND 4.0 | rejected | The non-commercial/no-derivatives terms are incompatible with a redistributable Apache-2.0 benchmark corpus. |
| [Pedestrian Dynamics Data Archive](https://ped.fz-juelich.de/da/) | archive-wide CC BY 4.0 notice | pending dataset-specific review | The archive is promising for controlled bottleneck experiments, but no individual dataset is admitted until its own files, protocol, schema, citation, and license statement are content-locked. |

## Selection rule

To enter `benchmarks/evidence/`, a source must have all of the following:

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
