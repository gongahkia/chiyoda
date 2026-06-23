# Benchmark Baselines

T2.3 ships two reproducible reference baselines for benchmark v1:

- `oracle`: rule-based scenario selector in `chiyoda/policies/oracle.py`.
- `ppo`: a Stable-Baselines3 PPO smoke policy trained through the Gymnasium-style wrapper in `chiyoda/policies/ppo_baseline.py`.

External interfaces:

- Stable-Baselines3 PPO API: <https://stable-baselines3.readthedocs.io/en/master/modules/ppo.html>
- Gymnasium Env API: <https://gymnasium.farama.org/api/env/>
- Gymnasium interface paper: <https://arxiv.org/abs/2407.17032>
- Safety-Gymnasium reference implementation context: <https://github.com/PKU-Alignment/safety-gymnasium>

## Artifacts

| file | purpose |
|:--|:--|
| `data/baselines/ppo_chiyoda_smoke.zip` | Stable-Baselines3 PPO weights trained for 16 timesteps on `scenarios/benchmark/transit_cbrn.yaml`. |
| `data/baselines/ppo_smoke_discrete_policy.json` | Metadata plus the trained model's cached deterministic action index for dependency-light CI eval. |

The checked-in PPO baseline is intentionally a smoke baseline, not an official leaderboard entry. It exists to make the RL baseline path concrete and reproducible without adding a large model artifact.

## Reproduction

Train the PPO artifact in an environment with Stable-Baselines3 and Gymnasium:

```bash
python -m pip install -e . stable-baselines3 gymnasium
python -m chiyoda.cli baseline train --kind ppo --scenario scenarios/benchmark/transit_cbrn.yaml --timesteps 16 --seed 42 -o data/baselines
```

Evaluate the shipped baselines:

```bash
python -m chiyoda.cli baseline eval --baseline oracle --suite v1 -o out/baselines/oracle_v1
python -m chiyoda.cli baseline eval --baseline ppo --suite v1 --weights data/baselines/ppo_smoke_discrete_policy.json -o out/baselines/ppo_v1
```

## Scores

| baseline | suite | tier | policy_hash | mean_score | score_ci_low | score_ci_high | seeds_used | run_count |
|:--|:--|:--|:--|--:|--:|--:|:--|--:|
| oracle | v1 | smoke | b0a3488299198abd | 52.51162191164837 | 52.26161148522841 | 52.76163233806833 | 42,137 | 12 |
| ppo | v1 | smoke | b89108b3ba529d21 | 52.50156909662613 | 52.227133966213735 | 52.77600422703853 | 42,137 | 12 |

`tests/test_baselines.py` reruns the eval path and checks these documented scores.
