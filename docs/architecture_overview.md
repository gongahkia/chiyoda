# Architecture Overview

## Runtime Stack

Chiyoda is organized as a strict scenario-to-simulation-to-study pipeline:

```text
scenario YAML
  -> ScenarioManager
  -> Layout / hazards / agents / responders / hostile channels
  -> Simulation runtime
  -> telemetry tables
  -> StudyBundle / benchmark outputs
```

## ITED Runtime

The ITED runtime couples physical movement and information state.

| Layer | Main modules | Responsibility |
|:--|:--|:--|
| Scenario loading | `chiyoda/scenarios/manager.py` | Build strict `layout.floors`, hazards, agents, responders, policies, and calibration metadata |
| Physical runtime | `chiyoda/core/simulation.py` | Advance steps, hazards, movement, connector queues, telemetry, interventions |
| Agents | `chiyoda/agents/` | Cognitive commuters, responders, hostile agents, physiology, route intent |
| Navigation | `chiyoda/navigation/` | Belief-weighted A*, social-force movement, floor-aware connectors |
| Environment | `chiyoda/environment/` | Layout parsing, exits, hazards, imported fields, station provenance |
| Information | `chiyoda/information/` | Beliefs, entropy, gossip, interventions, hostile channels, LLM controls |
| Analysis | `chiyoda/analysis/` | Metrics, reports, figures, static viewer, comparison |
| Studies | `chiyoda/studies/` | Study configs, bundle export/load, benchmark submit/scoring |

## Step Loop

At each simulation step:

1. unreleased agents become active when release time is reached,
2. hazards evolve,
3. agents observe exits and hazards within vision,
4. gossip and hostile-channel updates alter beliefs,
5. information interventions and optional LLM decisions run on schedule,
6. agents update intentions and paths from believed state,
7. social-force movement and connector queues advance positions,
8. telemetry snapshots are recorded.

## Information-Warfare Extension

Hostile channels are bounded misinformation emitters. They can alter agent beliefs but cannot mutate ground truth, layout, hazard physics, or movement speeds.

| Objective | Modeled effect |
|:--|:--|
| `false-protective-action` | false exit claim |
| `threat-amplification` | false or exaggerated hazard claim |
| `authority-confusion` | exit claim under responder-like source id |
| `social-proof-poisoning` | peer-like false claim seeded through local communication |

Agents maintain source credibility through belief revision. Provenance records claim source, channel, time, objective, claim target, and observed outcome when available.

## Selective LLM Layer

The LLM layer is deliberately narrow:

- generator providers: `template`, `replay`, `local_replay`, `openai`, `anthropic`,
- cache keys include request state,
- validation rejects invented exits/hazards, vague guidance, unsafe confidence, and unsafe radius/credibility,
- budget guards block live cache misses by call count, estimated tokens, or configured estimated USD,
- study bundles export `llm_calls` for replay verification.

LLM output is never treated as ground truth. Rejected generations fall back to deterministic bounded behavior where the runtime requires an action.

## Benchmark Layer

Benchmark suites live in `chiyoda/studies/benchmark.py` and `docs/benchmark/`.

Spec common to all suites:

- seeds: `42`, `137`,
- scoring rule: `composite_v1`,
- allowed knobs: `interventions`, `information`, `behavior`, `hostile_channels`,
- outputs: `benchmark_runs.csv`, `leaderboard.json`, `reproducibility_manifest.json`.

Per-suite scenarios:

- `v1`: `transit_cbrn`, `transit_shooter`, `transit_mixed` (baseline CBRN + active-shooter mix).
- `v2`: `wildfire_wui`, `transit_shooter` (wildland-urban interface egress + active shooter).
- `v3`: `flood_urban`, `quake_aftershock` (urban flood inundation + earthquake re-evacuation).

Select a suite with `chiyoda benchmark submit --suite {v1,v2,v3}`.

The composite score rewards lower travel time, lower hazard exposure, lower equity gap, and lower induced harmful convergence:

```text
100 * (0.35 * egress + 0.30 * exposure + 0.20 * equity + 0.15 * hci)
```

## Artifact Boundaries

- Chiyoda owns replayable scenario execution, information-control interventions, and benchmark telemetry.
- High-fidelity CFD/fire modeling remains outside the runtime; Chiyoda can import scalar hazard fields or use stylized hazards.
- External validation is explicit and limited to the reference workflows documented in `docs/external_validation.md`, `docs/trajectory_reference_workflow.md`, and `docs/calibration_route_choice_2025.md`.
