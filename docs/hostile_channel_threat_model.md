# Hostile-channel threat model

## Scope

Chiyoda hostile-channel experiments model misinformation as bounded messages
that alter agent beliefs during evacuation. The attacker can inject false or
misleading exit and hazard claims through configured channels, but cannot change
physical layout, hazard physics, agent speed, or simulator ground truth.

## Attacker objectives

| Objective | Effect |
|:--|:--|
| `decoy-exit` | add a plausible false exit claim |
| `panic-induce` | add a plausible false hazard claim and raise danger belief |
| `responder-spoof` | send an exit claim under a spoofed responder source id |
| `gossip-poison` | seed high-credibility peer-like false exit claims |

## Scenario schema

```yaml
hostile_channels:
  - id: decoy
    channel_type: gossip
    objective: decoy-exit
    budget: 4
    start_step: 0
    interval_steps: 5
    plausibility: 0.7
    radius: 6.0
    source_id: attacker
    target_cohort: baseline
    claimed_exit:
      floor: "0"
      x: 12
      y: 4
```

`budget` is the maximum number of recipient injections. `plausibility` scales
the initial belief credibility, then each agent's `BeliefRevisionModel` updates
source credibility from observed outcomes.

## Belief revision

Each agent stores per-source credibility as Beta parameters. A supported claim
increments alpha; a contradicted claim increments beta. Before each update, past
evidence is discounted toward the configured prior by `forgetting_factor`.

Message provenance is persisted per agent with source id, time, channel,
objective, claimed exit or hazard, observed outcome, and post-update
credibility. Direct observation resolves pending claims when the claimed
position enters the agent's vision radius.

## Metrics

`harmful_convergence_index_accidental` preserves the non-adversarial HCI
component. `harmful_convergence_index_induced` adds pressure from hostile
recipients and hostile source credibility. `information_safety_efficiency_adversarial`
penalizes ordinary information-safety efficiency by hostile convergence
pressure.

## Red-team CLI

```console
$ python -m chiyoda.cli red-team scenarios/station_baseline.yaml --budget 8 --objective decoy-exit
```

The command injects or overrides the first hostile channel, runs the scenario,
and emits JSON metrics.

## External anchors

The route-choice calibration TODO item should use the 2025 Scientific Data paper
`s41597-025-04440-y` as the dataset source:
<https://www.nature.com/articles/s41597-025-04440-y>.

Connector-flow calibration should cite NIST Technical Note 1839, which reports
fire-drill stair movement data from 14 buildings and more than 22,000
individual measurements:
<https://nvlpubs.nist.gov/nistpubs/TechnicalNotes/NIST.TN.1839.pdf>.
