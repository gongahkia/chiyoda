# Policy Engagement Note

Chiyoda is not a compliance tool or operational alerting system. Its policy value is pre-deployment evidence: scenario bundles, replayable message audits, and comparative safety metrics that let alerting authorities stress-test candidate communication strategies before field use.

## U.S. alerting references

| Reference | Policy question | Chiyoda outputs relevant to the question |
|:--|:--|:--|
| FCC modernization NPRM, [FCC 25-50 / PS Docket No. 25-224](https://www.fcc.gov/document/fcc-proposes-modernization-nations-alerting-systems) | Reexamines EAS/WEA objectives, sender needs, transmission capabilities, resilience, geographic targeting, and security. | Benchmark bundles quantify travel time, hazard exposure, equity, harmful convergence, message reach, hostile-channel effects, and replay/audit hashes under matched seeds. |
| FCC WEA FNPRM, [FCC 25-14 / PS Docket Nos. 15-91 and 15-94](https://www.federalregister.gov/documents/2025/03/18/2025-04125/wireless-emergency-alerts-emergency-alert-system) | Proposes broader use of WEA Public Safety Messages and asks about alert fatigue, subscriber customization, timely alerts, and accurate information in fast-evolving incidents. | Intervention timelines compare message radius, recipients, entropy change, exposure change, queue pressure, and LLM validation/fallback state for candidate public-safety messages. |
| FEMA IPAWS, [Best Practices for Alerting Authorities using Wireless Emergency Alerts](https://www.fema.gov/emergency-managers/practitioners/integrated-public-alert-warning-system/public-safety-officials/alerting-authorities/best-practices) | Advises alerting authorities to maintain policies, procedures, routine training, and message discipline. | Scenario assertions, replay-only runs, policy briefs, and viewer exports can support tabletop exercises, after-action review, and training comparisons of alert wording and targeting. |

## Outputs to package for authorities

- `metadata.json`: scenario provenance, benchmark hashes, LLM provider/model/cost summary, and run manifest.
- `tables/interventions.*`: message type, target, radius, recipients, entropy/exposure deltas, validation status, and generation metadata.
- `tables/llm_calls.*`: provider/model, cache status, validation/judge reasons, token/cost estimates, fallback state, and hash-chain links.
- `tables/equity_subgroups.*`: evacuation, travel-time, and exposure gaps by impairment, age, familiarity, mobility, responder, and hostile-agent flags.
- `policy_brief.md`: baseline-vs-variant decision table plus LLM provider cost summary.
- `viewer/`: inspectable 3D replay for tabletop review and stakeholder walkthroughs.

## International analogues

For ICAO, UNDRR, and national alerting authorities outside the U.S., the same package is relevant as evidence for drill design rather than rule compliance: compare message timing, target area, route-safety tradeoffs, accessibility/equity impacts, and hostile-message robustness across repeated seeds.
