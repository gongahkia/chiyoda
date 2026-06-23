# PADM Mapping

Chiyoda exposes a four-stage PADM update surface for each active agent:
`receive`, `understand`, `personalize`, and `decide`.

Sources:

- Lindell, M. K., & Perry, R. W. (2012). "The Protective Action Decision Model: Theoretical Modifications and Additional Evidence." Risk Analysis, 32(4), 616-632. https://doi.org/10.1111/j.1539-6924.2011.01647.x
- PubMed record: https://pubmed.ncbi.nlm.nih.gov/21689129/
- WEA 360-character message study: https://pmc.ncbi.nlm.nih.gov/articles/PMC11424238/
- NWS 360-character WEA examples: https://www.weather.gov/wrn/wea360

## Runtime Mapping

| PADM construct | Chiyoda stage | Code symbol | Runtime effect | Telemetry |
|:---|:---|:---|:---|:---|
| Warning, environmental, and social cue reception | `receive` | `InformationField.padm_receive`; `Simulation._step_information` gossip gate | Direct observations, beacon/PA inputs, pending provenance checks, and gossip transfer are admitted when the stage is enabled. | `AgentStepTelemetry.padm_receive` |
| Attention/comprehension of received information | `understand` | `InformationField.padm_understand` | Existing interpreted beliefs are aged through freshness decay after received evidence is integrated. [Inference] This is Chiyoda's compact operational proxy for PADM attention/comprehension rather than a separate cognitive parser. | `AgentStepTelemetry.padm_understand` |
| Threat, protective-action, and stakeholder perception personalization | `personalize` | `CognitiveAgent.padm_personalize`; `Simulation._padm_personalize` | Perceived hazard, observed hazard load, and retained hazard risk are collapsed into `personalized_risk`. [Inference] This maps PADM core perceptions to the current agent-local risk state. | `AgentStepTelemetry.padm_personalize` |
| Protective-action decision making | `decide` | `CognitiveAgent.padm_decide`; `Simulation._padm_decide` | The BDI intention is selected from beliefs, physiology, assistance state, and shooter pressure. | `AgentStepTelemetry.padm_decide` |

## Muting

`PADMStageConfig.with_muted(...)` disables any subset of stages for unit tests or ablations. A muted stage does not mutate its stage-owned state and does not increment its `padm_*` counter. The default `SimulationConfig.padm_enabled_stages` enables all four stages, preserving the prior simulation path.

## Message-Length Note

WEA evidence is not encoded as a new behavioral prior in T0.1. The source is tracked here because Chiyoda's warning-message layer can carry short or long alert text, and future empirical priors should distinguish message receipt/comprehension effects from downstream action choice.
