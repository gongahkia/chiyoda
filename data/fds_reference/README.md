# FDS Reference Data

This directory stores the static scalar profile used by
`scenarios/validation_fds_room_corridor.yaml`.

Source files:
- FDS case input: https://raw.githubusercontent.com/firemodels/fds/master/Verification/Detectors/smoke_detector.fds
- FDS detector CSV: https://raw.githubusercontent.com/firemodels/fds/master/Verification/Detectors/smoke_detector.csv
- Current FDS manuals index: https://pages.nist.gov/fds-smv/manuals.html
- FDS User Guide source: https://raw.githubusercontent.com/firemodels/fds/master/Manuals/FDS_User_Guide/FDS_User_Guide.tex
- FDS Verification Guide source: https://raw.githubusercontent.com/firemodels/fds/master/Manuals/FDS_Verification_Guide/FDS_Verification_Guide.tex

`smoke_detector_reference.csv` maps the first 10 detector time samples to a
one-row static profile. `gas_concentration_kg_kg` is the FDS `Soot Mass
Fraction` column. `visibility_m` is derived as `S = C/K`, `K = K_m * rho * Y_s`,
with `C=3`, `K_m=8700 m2/kg`, `rho=1.195 kg/m3`, and capped at the FDS default
`MAXIMUM_VISIBILITY=30 m`.
