# Information-Safety Theory

Chiyoda's harmful convergence index (HCI) is an internal diagnostic for cases
where communication reduces belief entropy while synchronizing agents into
unsafe queues, exits, or hazard-adjacent routes.

## Condition

[Inference] Let:

- `H(B)` be the population belief entropy before a message.
- `r >= 0` be cumulative entropy reduction already credited to interventions.
- `delta_r > 0` be the entropy reduction caused by a candidate message.
- `C(B)` be the physical convergence pressure induced by the current belief
  distribution: exit imbalance, queue pressure, and exposure pressure.
- `C(B') = C(B) + delta_C` be the convergence pressure after the message.

Use the normalized HCI proxy:

```text
HCI(B, r) = C(B) / (1 + r)
```

The message sits in the harmful entropy-reduction regime when:

```text
C(B') / C(B) > (1 + r + delta_r) / (1 + r)
```

Equivalently:

```text
delta_C > C(B) * delta_r / (1 + r)
```

## Proof Sketch

[Inference] The message is harmful when post-message HCI exceeds pre-message
HCI:

```text
C(B') / (1 + r + delta_r) > C(B) / (1 + r)
```

Cross-multiplying positive denominators gives:

```text
C(B') * (1 + r) > C(B) * (1 + r + delta_r)
```

Substitute `C(B') = C(B) + delta_C` and cancel the common
`C(B) * (1 + r)` term:

```text
delta_C * (1 + r) > C(B) * delta_r
```

So entropy reduction increases HCI when:

```text
delta_C > C(B) * delta_r / (1 + r)
```

## Static Detector

`chiyoda.analysis.info_safety_frontier.check_info_safety_scenario` estimates
this condition before running the simulation. It does not claim to predict the
exact dynamic HCI; it flags configurations that have the static ingredients for
the harmful regime:

- high entropy-reduction potential from low familiarity plus strong observation,
  beacon, or gossip reach;
- high convergence pressure from population-per-exit, hostile channels, or
  broadcast-style interventions;
- queue pressure from bottleneck zones and density;
- exposure pressure from hazard severity and radius.

CLI:

```console
$ python -m chiyoda.cli info-safety-check scenarios/station_sarin.yaml --json
```

Verdicts:

| Verdict | Meaning |
|:---|:---|
| `safe` | Static proxies do not show the harmful regime. |
| `borderline` | One side of the inequality is plausible; run the scenario before making claims. |
| `harmful` | Static proxies indicate entropy reduction can plausibly increase HCI. |

All detector coefficients are [Inference] thresholds. They are intentionally
simple, documented, and covered by scenario tests; they are not an externally
validated evacuation-safety classifier.
