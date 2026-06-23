# Reinforcement Learning Interface

`chiyoda.acceleration.rl_env.ChiyodaRLEnv` exposes a Gymnasium-style wrapper
around the existing `Simulation.step()` loop. It is intended for policy search
over information interventions, not for replacing benchmark submission runs.

## API

```python
from chiyoda.acceleration.rl_env import ChiyodaRLEnv

env = ChiyodaRLEnv("scenarios/benchmark/transit_cbrn.yaml")
observation, info = env.reset(seed=42)
observation, reward, terminated, truncated, info = env.step({"policy": "none"})
```

`reset()` returns `(observation, info)`.

`step(action)` returns `(observation, reward, terminated, truncated, info)`.
`terminated` means all evacuees are done. `truncated` means the scenario or
episode step cap was reached.

## Observation

Observation shape is `(n_slots, 4)`, one row per active intervention slot:

| Column | Meaning |
|:--|:--|
| `belief_entropy` | Latest global belief entropy from step telemetry. |
| `exposure` | Mean cumulative hazard exposure over non-responder evacuees. |
| `density` | Latest mean local density from step telemetry. |
| `hci` | `harmful_convergence_index_induced` from `SimulationAnalytics`. |

The wrapper repeats the same global state row for each slot because the current
runtime has one shared intervention surface. Multi-slot actions are executed
through a small composite intervention policy.

## Action

Action keys mirror the scenario `interventions` policy mapping. Examples:

```python
{"policy": "none"}

{
    "policy": "density_aware",
    "start_step": 0,
    "interval_steps": 1,
    "budget_per_interval": 1,
    "message_radius": 5.0,
}
```

Multiple slots can be passed as:

```python
{"interventions": [{"policy": "global_broadcast"}, {"policy": "entropy_targeted"}]}
```

Any policy accepted by `chiyoda.information.interventions.create_intervention_policy`
can be used. Unsupported policies fail fast through the existing intervention
factory.

## Reward

The default reward is:

```text
evacuated_delta - exposure_delta - 0.01 * remaining_evacuees - hci
```

This keeps reward finite and aligned with Chiyoda's benchmark direction:
evacuate agents while penalizing extra hazard exposure and induced harmful
convergence.

## Stable-Baselines3 PPO Sketch

The action is a Python dictionary, so production training usually wraps
`ChiyodaRLEnv` with a small discrete-action adapter:

```python
import gymnasium as gym
from gymnasium import spaces
from stable_baselines3 import PPO

from chiyoda.acceleration.rl_env import ChiyodaRLEnv


class DiscreteInterventionEnv(gym.Env):
    def __init__(self):
        self.env = ChiyodaRLEnv("scenarios/benchmark/transit_cbrn.yaml")
        self.actions = [
            {"policy": "none"},
            {"policy": "global_broadcast", "start_step": 0, "interval_steps": 5},
            {"policy": "density_aware", "start_step": 0, "interval_steps": 5},
        ]
        self.action_space = spaces.Discrete(len(self.actions))
        self.observation_space = spaces.Box(
            low=0.0, high=float("inf"), shape=(1, 4), dtype="float32"
        )

    def reset(self, *, seed=None, options=None):
        obs, info = self.env.reset(seed=seed, options=options)
        return obs.astype("float32"), info

    def step(self, action):
        obs, reward, terminated, truncated, info = self.env.step(
            self.actions[int(action)]
        )
        return obs.astype("float32"), reward, terminated, truncated, info


model = PPO("MlpPolicy", DiscreteInterventionEnv(), verbose=1)
model.learn(total_timesteps=10_000)
```

## Two-Player Parallel API

YAML opt-in selects the PettingZoo-style parallel wrapper:

```yaml
scenario:
  rl:
    mode: two_player
    defender_actions:
      - {policy: none}
      - {policy: global_broadcast, start_step: 0, interval_steps: 1}
    adversary_actions:
      - {policy: none}
      - {objective: false-protective-action, budget: 1}
      - {objective: threat-amplification, budget: 1}
```

```python
from chiyoda.acceleration.rl_env import create_rl_env

env = create_rl_env("scenario.yaml")
observations, infos = env.reset(seed=42)
actions = {agent: env.action_space(agent).sample() for agent in env.agents}
observations, rewards, terminations, truncations, infos = env.step(actions)
```

`create_rl_env()` returns `ChiyodaRLEnv` unless `rl.mode` is `two_player`; then
it returns `ChiyodaParallelRLEnv`.

Parallel agents:

| Agent | Control surface | Reward |
|:--|:--|:--|
| `defender` | One intervention action template from `rl.defender_actions`. | Single-agent reward: evacuation gain minus exposure, remaining evacuees, and induced HCI. |
| `adversary` | One hostile-channel action template from `rl.adversary_actions`. | Negative defender reward plus `0.1 * hostile_recipient_delta`. |

Both agents receive the same 6-column global observation:

| Column | Meaning |
|:--|:--|
| `belief_entropy` | Latest global belief entropy. |
| `exposure` | Mean cumulative hazard exposure. |
| `density` | Latest mean local density. |
| `hci` | `harmful_convergence_index_induced`. |
| `hostile_events` | Cumulative hostile-channel event count. |
| `hostile_recipients` | Cumulative hostile-channel recipient count. |

The wrapper implements the PettingZoo ParallelEnv call shape documented by
Farama: `reset()` returns observation and info dictionaries, and `step()`
returns observation, reward, termination, truncation, and info dictionaries.
Reference: <https://pettingzoo.farama.org/api/parallel/>.

The adversarial framing follows the multi-agent threat-model motivation in
TAMAS, which benchmarks adversarial risks in multi-agent LLM systems:
<https://arxiv.org/abs/2511.05269>.

## Prior Art

- EvacuAI / ExitMatrix uses deep reinforcement learning for real-time indoor
  escape routing under fire conditions: <https://pmc.ncbi.nlm.nih.gov/articles/PMC10648289/>.
- Klepach et al. combine RL with artificial forces for leader-guided evacuation
  of active particles: <https://arxiv.org/abs/2509.19972>.
