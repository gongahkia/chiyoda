# FDS Scalar Field Agreement

This is a static scalar import check, not a CFD solver validation. It imports a
profile derived from the FDS `smoke_detector` verification case and confirms
that Chiyoda preserves the gas concentration and visibility values used by a
room-corridor scenario.

Primary sources:
- NIST FDS-SMV describes FDS as an LES code for smoke and heat transport:
  https://pages.nist.gov/fds-smv/
- Current FDS manuals index, release FDS 6.11.0 / SMV 6.11.0:
  https://pages.nist.gov/fds-smv/manuals.html
- FDS User Guide source for `K = K_m rho Y_s`, `S = C/K`, and
  `MAXIMUM_VISIBILITY=30 m`:
  https://raw.githubusercontent.com/firemodels/fds/master/Manuals/FDS_User_Guide/FDS_User_Guide.tex
- FDS Verification Guide source for the `smoke_detector` case:
  https://raw.githubusercontent.com/firemodels/fds/master/Manuals/FDS_Verification_Guide/FDS_Verification_Guide.tex
- FDS case input and detector output:
  https://raw.githubusercontent.com/firemodels/fds/master/Verification/Detectors/smoke_detector.fds
  https://raw.githubusercontent.com/firemodels/fds/master/Verification/Detectors/smoke_detector.csv

## Reference Conversion

`data/fds_reference/smoke_detector_reference.csv` uses the first 10 rows of the
FDS detector CSV. `gas_concentration_kg_kg` is the FDS `Soot Mass Fraction`
column. `visibility_m` is derived with the FDS visibility relation:

`K = K_m * rho * Y_s`

`S = C / K`

Constants used: `K_m=8700 m2/kg`, `rho=1.195 kg/m3`, `C=3`, capped at
`MAXIMUM_VISIBILITY=30 m`.

The CSV row index is mapped onto a 1 m room-corridor profile. This keeps the
agreement test deterministic and avoids claiming transient smoke transport
parity.

## Metrics

Executed by `tests/test_fds_agreement.py`.

| Metric | RMSE | Max abs error | Pass criteria | Result |
| --- | ---: | ---: | ---: | --- |
| Gas concentration, kg/kg | 0.0 | 0.0 | RMSE <= 1e-12 and max <= 1e-12 | pass |
| Visibility, m | 0.0 | 0.0 | RMSE <= 1e-9 and max <= 1e-9 | pass |

Remaining mismatches: none measured inside this static import harness.
[Inference] Dynamic mismatches, if measured by running FDS and Chiyoda as
coupled transient solvers, would come from transport physics and time
integration because this harness imports a fixed scalar field.
