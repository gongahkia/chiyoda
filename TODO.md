(A) 2026-06-21 Curate canonical benchmark scenario suite v1 (transit_cbrn, transit_shooter, transit_mixed) +Benchmark @scenarios release:v1
(A) 2026-06-21 Define BenchmarkSpec dataclass and JSON schema (metrics, seeds, scoring rule, allowed knobs) +Benchmark @studies release:v1
(A) 2026-06-21 Implement submission API chiyoda.cli benchmark submit --policy path --suite v1 +Benchmark @core release:v1
(A) 2026-06-21 Implement scoring rule combining egress time, exposure, equity, HCI-adversarial into a single composite +Benchmark @analysis release:v1
(A) 2026-06-21 Generate reference trajectories per scenario across N seeds and persist as parquet artifacts +Benchmark @scenarios release:v1
(A) 2026-06-21 Add leaderboard JSON export and reproducibility kit (config hash, seed set, version pin) +Benchmark @studies release:v1
(B) 2026-06-21 Static leaderboard site generator (markdown to HTML) checked into docs/benchmark/ +Benchmark @docs release:v1
(B) 2026-06-21 Write benchmark spec doc (metric definitions, submission rules, scoring, ablation expectations) +Benchmark @docs release:v1
(A) 2026-06-21 Ingest Sci Data 2025 s41597-025-04440-y route-choice dataset into data/calibration/route_choice_2025/ +Calibration @env release:v1
(A) 2026-06-21 Fit route-choice prior parameters (familiarity, herding, exit-affinity) against the 2025 dataset +Calibration @info release:v1
(A) 2026-06-21 Document calibration procedure and provenance in docs/calibration_route_choice_2025.md +Calibration @docs release:v1
(B) 2026-06-21 Add calibration regression test that asserts fit quality above floor on held-out split +Calibration @tests release:v1
(A) 2026-06-21 Enrich group attachment semantics family_id, role-in-group, separation-anxiety threshold +Homophily3D @agents release:v1
(A) 2026-06-21 Implement homophily-weighted destination choice (Marshall-Fire reference) +Homophily3D @info release:v1
(A) 2026-06-21 Add 3D-height to layout cells; smoke layering and gas density as height-dependent fields +Homophily3D @env release:v1
(A) 2026-06-21 Make LOS, hazard exposure, and connector traversal height-aware in navigation +Homophily3D @nav release:v1
(B) 2026-06-21 Add equity metrics left-behind index, exposure-by-group, percentile gap in time-to-safety +Homophily3D @analysis release:v1
(B) 2026-06-21 Heterogeneous mobility classes (wheelchair, walker, visual-impairment) as opt-in cohort kind +Homophily3D @agents release:v1
(B) 2026-06-21 Persona-conditioned population generation via bounded LLM call with caching and validation +LLMSelective @info release:v1
(B) 2026-06-21 Multi-responder coordination policy (LLM negotiates broadcast targets given live entropy field) +LLMSelective @info release:v1
(C) 2026-06-21 Provider abstraction OpenAI / Anthropic / local replay; cache budget and cost guard +LLMSelective @info release:v1
(C) 2026-06-21 LLM-call audit log surfaced in study bundle for replay verification +LLMSelective @analysis release:v1
(A) 2026-06-21 Write paper outline info-warfare in info-aware evacuation, benchmark introduction, key results +Docs @docs release:v1
(A) 2026-06-21 Add architecture overview doc covering ITED runtime, info-warfare extension, benchmark layer +Docs @docs release:v1
(B) 2026-06-21 Update README with benchmark posture, info-warfare angle, hazard staging roadmap +Docs @docs release:v1
(B) 2026-06-21 Create reproducibility kit doc (env pin, seed set, expected outputs, hash manifest) +Docs @docs release:v1
(B) 2026-06-21 CI add benchmark suite v1 smoke run gated by PR label +Tests @ci release:v1
(B) 2026-06-21 Add Wildfire/WUI hazard kind, ember-spread field, long-range broadcast policy +HazardWildfire @env release:v2
(B) 2026-06-21 Vehicular-pedestrian coupling for WUI scenarios (egress road segments, mode switch) +HazardWildfire @nav release:v2
(B) 2026-06-21 Author wildfire_wui.yaml benchmark scenario based on Marshall-Fire-like geometry +HazardWildfire @scenarios release:v2
(C) 2026-06-21 Add Flood hazard with time-evolving inundation field +HazardFlood @env release:v3
(C) 2026-06-21 Add Earthquake-aftershock hazard with terrain damage and re-evacuation waves +HazardFlood @env release:v3
(C) 2026-06-21 Author flood_urban.yaml and quake_aftershock.yaml benchmark scenarios +HazardFlood @scenarios release:v3
