# Glossary

This glossary defines Chiyoda terms used in `docs/paper_outline_info_warfare.md` and `docs/architecture_overview.md`.

| Term | Definition |
|:--|:--|
| A* | Graph-search route planner available through Chiyoda's `networkx_astar` and `heap_astar` strategies. |
| Active agent | An agent whose release time has passed and who is participating in the simulation step loop. |
| Active-shooter scenario | Benchmark hazard scenario that represents a moving security threat and evacuation response. |
| Adaptive intervention | Information-control action selected from current telemetry, such as entropy, density, exposure, or bottleneck state. |
| Agent | Simulated person with position, speed, state, route intent, beliefs, and telemetry. |
| Agent decision | Optional LLM-controlled or deterministic update to an agent's route intent and target exit. |
| Analysis layer | Reporting, metrics, figures, static viewer, and comparison code under `chiyoda/analysis/`. |
| Anthropic provider | Live Anthropic Messages API generator behind Chiyoda's LLM abstraction. |
| Authority confusion | Hostile-channel objective that presents a false claim under a responder-like source identity. |
| Belief | Agent-local estimate of exits, hazards, and source credibility, distinct from ground truth. |
| Belief entropy | Uncertainty score over an agent's belief distribution. Lower entropy means a more concentrated belief state. |
| Belief persistence | Continued influence of a belief after the original cue or hostile claim is no longer current. |
| Belief revision | Update process that changes beliefs from observation, gossip, signage, interventions, or hostile channels. |
| Belief vector | Per-agent numeric representation of exit or hazard belief state. |
| Belief-weighted A* | A* routing whose edge or target costs use believed hazard and exit state rather than only ground truth. |
| Reverse Dijkstra | Multi-source routing strategy that computes cached next hops from exits back to many starts. |
| Benchmark | Repeatable scenario set with fixed seeds, allowed knobs, metrics, scoring, and manifest outputs. |
| Benchmark manifest | Exported record of benchmark inputs, hashes, seeds, and outputs used for reproducibility. |
| Benchmark suite | Named benchmark group such as `v1`, `v2`, or `v3`. |
| Bottleneck | Layout zone or local crowd state with high queueing, dwell time, or density. |
| Bottleneck avoidance | Intervention policy that steers recipients away from queue pressure or high-density routes. |
| Bounded LLM proposal | Generated message or decision constrained by known exits, hazards, budget, validation, and replay audit. |
| Budget guard | LLM call gate that records or blocks calls by count, estimated tokens, or estimated USD. |
| Cache key | Stable hash of LLM request state used to replay or audit generated outputs. |
| Cache status | Per-call state such as cache hit, miss, disabled, replay-only miss, or budget block. |
| Causal delta | Matched-seed baseline-vs-treated effect payload exported as `causal_delta.json`. |
| CFD | Computational fluid dynamics; outside Chiyoda's runtime except as an external reference or imported field source. |
| Chiyoda | Research simulator for coupled crowd movement, information flow, hostile messaging, and benchmark telemetry. |
| CFAST | NIST zone fire model referenced as an external high-fidelity fire/smoke tool. |
| Composite score | Benchmark score combining normalized egress, exposure, equity, and HCI terms. |
| Connector queue | Queue model for constrained movement through floor connectors such as stairs or elevators. |
| Corrective messaging | Intervention intended to improve route or hazard beliefs relative to ground truth. |
| Credibility | Source trust weight used during belief revision. |
| Density-aware policy | Intervention policy that targets high-density local crowd states. |
| Deterministic bounded behavior | Non-LLM fallback action with fixed behavior under the same state and seed. |
| Egress | Evacuation progress or travel-time component of benchmark scoring. |
| Entropy | See belief entropy. |
| Entropy-targeted policy | Intervention policy that targets high-uncertainty belief states. |
| Equity | Benchmark dimension measuring subgroup gaps in evacuation, travel time, or exposure. |
| Equity gap | Difference between subgroup outcomes and run-level outcomes. |
| Exposure | Accumulated contact with hazard intensity over time. |
| Exposure-aware policy | Intervention policy that prioritizes agents or areas with high hazard load. |
| External validation | Comparison against reference data or external tools, documented as limited workflows rather than global simulator validation. |
| False protective action | Hostile-channel objective that recommends an unsafe or false protective route/action. |
| FDS | NIST Fire Dynamics Simulator; external high-fidelity reference for fire/smoke modeling. |
| Floor-aware connector | Navigation link that moves agents between floors while respecting connector capacity and geometry. |
| Generated message | LLM or template-produced evacuation guidance proposed for an intervention or hostile channel. |
| Global policy | Intervention policy that broadcasts without local targeting. |
| Gossip | Agent-to-agent information transfer that can alter beliefs and source provenance. |
| Ground truth | Scenario state known to the simulator, including layout, exits, hazards, and physical positions. |
| Harmful convergence | Crowd synchronization onto a route or exit that raises queue pressure, exposure, or failure risk. |
| Hazard | Threat field or event affecting exposure, route cost, movement, or warning behavior. |
| Hazard exposure | See exposure. |
| Hazard field | Spatial scalar field for smoke, gas, flood depth, fire intensity, or similar threat intensity. |
| Hash-chain audit | Tamper-evident sequence of row hashes used to verify exported `llm_calls` row order and content. |
| HCI | Harmful Convergence Index: metric for attacker-induced or policy-induced concentration onto risky routes. |
| Homophily prior | Parameter controlling tendency to trust or follow similar agents. |
| Hostile agent | Simulated adversarial person entity that can emit local hostile claims. |
| Hostile channel | Bounded misinformation emitter that changes beliefs without mutating ground truth. |
| Hostile-channel objective | Attack goal such as false protective action, threat amplification, authority confusion, or social-proof poisoning. |
| Information-control policy | Policy that changes message timing, target, content, or channel to influence agent beliefs. |
| Information flow | Movement of observations, gossip, signage, interventions, and hostile claims through the population. |
| Information layer | Belief, entropy, gossip, intervention, hostile-channel, and LLM-control code under `chiyoda/information/`. |
| Information-safety frontier | Plot or comparison of uncertainty reduction against exposure, HCI, or other safety costs. |
| Information-safety tradeoff | Case where better-informed or more synchronized beliefs can still worsen physical safety metrics. |
| Imported hazard field | External scalar hazard data loaded into Chiyoda instead of generated by a stylized built-in hazard. |
| Intent | Agent's current behavioral target, such as evacuating toward a selected exit. |
| Intervention | Simulator-controlled message or guidance action affecting agent beliefs. |
| ITED | Information-Theoretic Evacuation Dynamics runtime: Chiyoda's coupled physical-information simulation framing. |
| Layout floors | Strict `layout.floors` scenario representation for multi-floor geometry. |
| LLM | Large language model provider used only behind bounded generation, validation, cache, and audit controls. |
| LLM call audit | Per-call `llm_calls` export with provider, model, cache, validation, cost, fallback, and hash-chain fields. |
| LLM layer | Narrow subsystem for bounded generated messages and optional agent decisions. |
| LLMSelective | Experiment family that varies LLM provider, replay/cache mode, budget guard, and coordination policy. |
| Local replay | Replay mode using cached local generation records instead of live provider calls. |
| Matched seed | Same random seed used in paired baseline and treated runs for causal comparison. |
| Misinformation | False or misleading information that changes beliefs relative to ground truth. |
| Mixed misinformation scenario | Scenario combining physical hazard stressors with hostile-message stressors. |
| Model | Provider-specific LLM identifier recorded with generated calls. |
| Multi-floor scenario | Scenario using strict `layout.floors` and floor-aware connectors. |
| Non-evacuation failure | Failure mode where agents remain, delay, or choose unsuitable actions despite low direct hazard exposure. |
| OpenAI provider | Live OpenAI Responses API generator behind Chiyoda's LLM abstraction. |
| Parquet/CSV tables | Study bundle telemetry table formats. |
| Parameter provenance | Metadata recording where calibrated or generated scenario parameters came from. |
| Physical runtime | Simulation core for movement, hazards, connector queues, interventions, and telemetry. |
| Policy brief | Markdown comparison summary for baseline-vs-variant decisions and LLM provider costs. |
| Policy knob | Scenario or benchmark setting the participant is allowed to change. |
| Provider | LLM backend or deterministic generator family recorded in audit rows. |
| Provenance | Source, channel, time, objective, claim target, or observed outcome metadata for claims and scenario inputs. |
| Queue pressure | Congestion signal from bottleneck occupancy, flow imbalance, dwell time, or density. |
| Replay | Running or auditing from cached/exported records rather than making fresh live LLM calls. |
| Reproducibility manifest | Exported hashes and run metadata needed to reproduce benchmark outputs. |
| Responder | Agent or intervention source representing emergency staff or coordinated response. |
| Responder relay | Intervention mode where responder-origin information is propagated through recipients. |
| Route-choice prior | Calibrated or configured parameter influencing how agents choose exits/routes. |
| Route intent | Agent's selected movement goal and target exit. |
| Scenario assertion | Runtime or static check that validates scenario reachability, exits, starts, or expected behavior. |
| Scenario manager | Loader/builder that converts scenario YAML into simulation objects. |
| Scenario YAML | Input configuration format for layout, hazards, agents, policies, and metadata. |
| Signage/beacon | Non-agent information source that can update beliefs locally. |
| Smoke baseline | Small benchmark run used to verify scoring, telemetry, and reproducibility paths. |
| Social-force movement | Pedestrian motion model using desired velocity and interaction forces. |
| Social-proof poisoning | Hostile-channel objective that presents a false claim as peer-supported or socially common. |
| Source credibility | Trust assigned to information source during belief revision. |
| Source provenance | Record of where a belief-changing claim came from. |
| Station provenance | Metadata describing report-facing station source data, license, limitations, and source objects/files. |
| Static policy | Fixed intervention policy that does not adapt to current telemetry. |
| Step loop | Ordered simulation update sequence run once per simulated time step. |
| Study bundle | Exported study directory with metadata, telemetry tables, figures, viewer assets, and reports. |
| Study config | Repeated-run configuration with seeds, variants, sweeps, jobs, and export options. |
| Stylized hazard | Simplified hazard model used for comparative simulation rather than high-fidelity physics. |
| Telemetry | Recorded per-run tables for steps, cells, agents, hazards, interventions, LLM calls, and summaries. |
| Template provider | Deterministic non-live LLM generator used for tests and replay-safe dry runs. |
| Threat amplification | Hostile-channel objective that exaggerates or fabricates hazard severity. |
| Travel time | Time from release to evacuation for an agent or aggregate group. |
| Validation | Rule-based or judge-based check on generated messages, scenarios, or benchmark artifacts. |
| Validator/judge reasons | Exported reasons explaining why a generated message or decision was accepted or rejected. |
| Variant | Named study condition such as baseline, treated, or a policy configuration. |
| Viewer | Static Three.js replay/export surface under `viewer/`. |
