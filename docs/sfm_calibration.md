# Social Force Calibration

Scenarios can select a social-force profile with:

```yaml
social_force_calibration: yolov5_mdpi_2024
```

or with explicit overrides:

```yaml
social_force_calibration:
  profile: yolov5_mdpi_2024
  parameters:
    base_vision_radius_m: 4.0
    visual_range_m: 3.5
    counter_flow_avoidance_strength: 0.9
```

Available profiles live under `data/sfm_calibrations/`:

- `generic_legacy`: previous Chiyoda hardcoded defaults.
- `yolov5_mdpi_2024`: desired speed, relaxation time, pedestrian interaction
  intensity, and pedestrian interaction range from Sensors 2024 Table 7:
  <https://doi.org/10.3390/s24155011>

The Physica A 2024 counterflow reference is tracked for the existing
counterflow friction and lateral avoidance terms:
<https://doi.org/10.1016/j.physa.2024.129762>

Limited visual-range parameters (`visual_range_m`, `visual_field_degrees`, and
`rear_repulsion_weight`) are opt-in overrides so legacy benchmark scores remain
stable. They follow the asymmetric visual-range SFM reference:
<https://doi.org/10.1016/j.physa.2023.128461>

Sensitivity evidence for a fixed one-step baseline is stored in
`data/sfm_calibrations/sensitivity_baseline.json`. It records the displacement
delta for each changed `yolov5_mdpi_2024` parameter against `generic_legacy`.
