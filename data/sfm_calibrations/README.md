# Social Force Calibrations

`generic_legacy.yaml` preserves Chiyoda's previous hardcoded parameters.

`yolov5_mdpi_2024.yaml` uses the four calibrated pedestrian SFM parameters
reported in Table 7 of:

<https://doi.org/10.3390/s24155011>

That article calibrates desired speed, relaxation time, pedestrian interaction
intensity, and pedestrian interaction range from YOLOv5-derived station density
maps. Chiyoda-specific wall, cutoff, clamp, and observation-radius parameters
remain explicit legacy defaults with their own provenance labels.

The counterflow reference tracked for later SFM extensions is:

<https://doi.org/10.1016/j.physa.2024.129762>

The visual-range terms use the limited-visual-range SFM reference:

<https://doi.org/10.1016/j.physa.2023.128461>
