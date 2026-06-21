# Route Choice Calibration 2025

This calibration ingests Snopkova et al.'s Scientific Data route-choice
dataset and fits a small held-out model for Chiyoda route-choice priors.

## Source

| Field | Value |
| --- | --- |
| Article | Snopkova, Tancos, Herman, Jurik. "Predictors of evacuation behavior: dataset on respondents' route choice and web interaction." Scientific Data 12, 116 (2025). |
| Article DOI | https://doi.org/10.1038/s41597-025-04440-y |
| Dataset DOI | https://doi.org/10.6084/m9.figshare.27705402.v1 |
| Local archive | `data/calibration/route_choice_2025/Snopkova_Isovists.zip` |
| Archive MD5 | `2d53020e8aac54ed270205534623742d` |
| Dataset license | CC BY 4.0 in Figshare metadata |

The raw archive contains `responses.csv`, `participants.csv`,
`interaction.csv`, and `confidence.csv`. The article describes a web-based
T-intersection experiment where participants chose left or right corridors
under an 8 second timer, with task IDs encoding corridor width, length, and
stair presence.

## Procedure

Run:

```sh
.venv/bin/python -m chiyoda.cli calibrate-route-choice
```

The command writes:

| Artifact | Purpose |
| --- | --- |
| `figshare_article_27705402.json` | Figshare metadata and file checksum. |
| `Snopkova_Isovists.zip` | Immutable raw source archive. |
| `normalized_route_choice_records.csv` | One row per valid non-training left/right choice. |
| `fit_parameters.json` | Model coefficients, held-out metrics, and Chiyoda prior values. |

The normalizer excludes training tasks, timeout/background/no-AOI rows, and
rows without a valid timer. It decodes `taskId_L-widthlengthstairs_R-widthlengthstairs`
into right-minus-left width, right-shorter length, and right-minus-left stair
features. It also adds a participant-local previous-side feature as a weak
inertia proxy.

The fitter uses deterministic participant-held-out L2 logistic regression.
The held-out test compares model log loss against an intercept-only baseline.
The current local fit uses 4,045 observations from 208 participants:

| Metric | Value |
| --- | ---: |
| Test log loss | `0.4182` |
| Baseline log loss | `0.6932` |
| Log-loss improvement | `0.2750` |
| Test accuracy | `0.845` |

## Prior Mapping

`fit_parameters.json` exposes three Chiyoda-facing priors:

| Prior | Mapping |
| --- | --- |
| `exit_affinity` | Logistic transform of the fitted geometry-coefficient norm for width, length, and stairs. |
| `herding` | Logistic transform of the previous-side coefficient. This is an inertia proxy, not an observed crowd-following measurement. |
| `familiarity` | Confidence and response-speed summary from valid choices, scaled to `[0, 1]`. |

Do not treat these priors as station-specific evacuation truth. The source is
a controlled T-intersection web/VR task, not a Chiyoda station drill.
