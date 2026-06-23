# Empirical Priors

Chiyoda stores empirically anchored milling-time and compliance priors in
`data/empirical/milling_distributions.json`.

Sources:

- Cell Broadcast tsunami dataset: https://data.mendeley.com/datasets/9wg3hb4w23/1
- Dataset DOI: https://doi.org/10.17632/9wg3hb4w23.1
- Dataset article: https://pmc.ncbi.nlm.nih.gov/articles/PMC11599998/
- FEMA WEA best practices: https://www.fema.gov/emergency-managers/practitioners/integrated-public-alert-warning-system/public-safety-officials/alerting-authorities/best-practices
- WEA 360-character study: https://pmc.ncbi.nlm.nih.gov/articles/PMC11424238/

## YAML

Use:

```yaml
behavior:
  milling_time_dist: cb_fr_2024
  compliance_dist: cb_fr_2024
```

Available priors:

| Prior | Milling-time basis | Compliance basis | Status |
|:---|:---|:---|:---|
| `cb_fr_2024` | Mendeley `Tsunami_test_En.csv`, SHA-256 `c30d98682bb6e24b9f0a0524be017b51eaf28687cc011f805fe5151a2f236581` | Same dataset; explicit non-evacuation answer | empirical |
| `wea_us_default` | FEMA/WEA guidance, not a fitted response-time dataset | FEMA/WEA guidance and PMC11424238 high-compliance finding | [Inference] |
| `synthetic_baseline` | immediate release | always compliant | legacy control |

The empirical prior file declares `cb_fr_2024` as its default. For backward
compatibility, existing scenarios that omit both YAML keys still use
`synthetic_baseline`.

## Cell Broadcast 2024 Derivation

`cb_fr_2024` uses the English CSV because it contains stable English column
names. The dataset page reports 9,446 completed answers, collected after Cell
Broadcast tsunami alerts along the French Mediterranean coast.

Milling-time buckets use `Evacuation_time` rows with a concrete time answer:

| Source bucket | Count | Seconds used |
|:---|---:|---:|
| `less than 1 minutes` | 1,401 | 30 |
| `2 to 5 minutes` | 3,041 | 210 |
| `6 to 10 minutes` | 1,502 | 480 |
| `11 to 20 minutes` | 706 | 930 |
| `21 to 30 minutes` | 330 | 1,530 |
| `more than 31 minutes` | 155 | 2,700 |

Stored moments:

- `n = 7135`
- mean `417.88086895585144` seconds
- variance `245066.7076017677` seconds^2
- stddev `495.04212709805586` seconds

Deviations:

- [Inference] Bucketed answers are represented by midpoints.
- [Inference] `more than 31 minutes` is represented as 45 minutes because the
  source bucket is open-ended.
- `I do not know`, blank, and `Question not asked` rows are excluded from
  milling-time moments.
- Compliance uses concrete time-answer rows as compliant and the explicit
  `I would not have evacuated so I will not answer this question` answer as
  noncompliant. Unknown answers are excluded.

Compliance moments:

- `n = 8011`
- probability `0.8906503557608288`
- variance `0.09739229954393785`

## Runtime Effect

When a scenario explicitly selects a non-`synthetic_baseline`
`milling_time_dist`, `ScenarioManager` samples a delay and adds it to each
non-responder, non-hostile agent's `release_step`.

When a scenario explicitly selects a non-`synthetic_baseline`
`compliance_dist`, `BehaviorModel` samples `will_comply_with_alert`. Agents
sampled as noncompliant remain in `FROZEN` state.
