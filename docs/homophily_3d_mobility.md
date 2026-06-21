# Homophily, 3D Height, and Mobility

Chiyoda now models three opt-in extensions for evacuation heterogeneity.

## Group Attachment

Cohorts can declare:

| Field | Meaning |
| --- | --- |
| `family_id` | Shared attachment identifier. If omitted, grouped cohorts get generated family IDs. |
| `role_in_group` | `solo`, `leader`, `member`, `helper`, or `dependent`. |
| `separation_anxiety_threshold` | Distance before a follower biases movement back toward its leader/helper. |

`group_size` still assigns `group_id` and `leader_id`; the new fields make the
attachment semantics explicit in telemetry and study exports.

## Homophily Destination Choice

Scenario-level `destination_profiles` attach attributes to exit cells:

```yaml
destination_profiles:
  - cell: {floor: "0", x: 10, y: 4}
    profile: {community: "east"}
```

Cohorts can set `homophily_profile` and `homophily_weight`. During target-exit
selection, agents combine exit belief quality, weak distance affinity, family
target inertia, and profile similarity.

Reference: Raipat and Yabe's Marshall Fire study reports that evacuees selected
destinations with higher social similarity and social connectedness than their
spatial exposure alone would predict. See
https://doi.org/10.1057/s41599-026-07237-5.

## Height-Aware Cells And Hazards

Floors accept `cell_height_m`, `height_grid`, or `cell_heights` to add local
height offsets to cells. Connectors persist `height_delta_m` and queue traversal
uses that vertical distance when `travel_s` is not explicitly set.

Smoke/gas hazards remain legacy-compatible unless `height_aware: true` is set.
When enabled, hazard exposure and visibility are sampled at each agent's
`breathing_height_m`:

```yaml
hazards:
  - type: SMOKE
    location: [12.5, 5.5, 0.0]
    radius: 6.0
    severity: 1.0
    height_aware: true
    layer_base_m: 1.8
    layer_top_m: 3.0
    vertical_decay_m: 0.4
```

This is a lightweight surrogate, not an FDS/CFAST replacement. NIST FDS and
CFAST documents model smoke layer height and gas layers with more detailed fire
physics: https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication1019-5.pdf
and https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.1026r1.pdf.

## Mobility And Equity

`mobility_class` is opt-in per cohort:

| Class | Effect |
| --- | --- |
| `standard` | Default speed, vision, and breathing height. |
| `wheelchair` | Lower base speed, lower breathing height. |
| `walker` | Moderate speed reduction. |
| `visual-impairment` | Reduced base vision radius. |

Summary metrics now include:

| Metric | Meaning |
| --- | --- |
| `left_behind_index` | Max minus min cohort non-evacuation rate. |
| `exposure_by_group` | JSON mean hazard exposure by cohort. |
| `exposure_by_mobility_class` | JSON mean hazard exposure by mobility class. |
| `percentile_gap_time_to_safety_s` | P95 minus P50 completed travel time. |

These metrics expose disparate outcomes; they do not identify protected-class
causality or policy compliance.
