"""Benchmark suite definitions, scoring, and submission helpers."""

from __future__ import annotations

import json
from collections.abc import Iterable
from dataclasses import asdict, dataclass, field
from datetime import UTC, datetime
from hashlib import sha256
from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd
import yaml

from chiyoda import __version__
from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.scenarios.manager import ScenarioManager


@dataclass(frozen=True)
class BenchmarkScenario:
    name: str
    scenario_file: str


@dataclass(frozen=True)
class BenchmarkSpec:
    suite: str
    metrics: list[str]
    seeds: list[int]
    scoring_rule: str
    allowed_knobs: list[str]
    scenarios: list[BenchmarkScenario] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)

    @staticmethod
    def json_schema() -> dict[str, Any]:
        return {
            "type": "object",
            "required": [
                "suite",
                "metrics",
                "seeds",
                "scoring_rule",
                "allowed_knobs",
                "scenarios",
            ],
            "properties": {
                "suite": {"type": "string"},
                "metrics": {"type": "array", "items": {"type": "string"}},
                "seeds": {"type": "array", "items": {"type": "integer"}},
                "scoring_rule": {"type": "string"},
                "allowed_knobs": {"type": "array", "items": {"type": "string"}},
                "scenarios": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["name", "scenario_file"],
                        "properties": {
                            "name": {"type": "string"},
                            "scenario_file": {"type": "string"},
                        },
                    },
                },
            },
        }


_BENCHMARK_METRICS_V1 = [
    "mean_travel_time_s",
    "p95_hazard_exposure",
    "equity_time_gap_s",
    "harmful_convergence_index_induced",
    "benchmark_score",
]

_BENCHMARK_ALLOWED_KNOBS_V1 = [
    "interventions",
    "information",
    "behavior",
    "hostile_channels",
]
BENCHMARK_BOOTSTRAP_N = 1000
BENCHMARK_BOOTSTRAP_SEED = 90210
OFFICIAL_MIN_SEEDS = 20


def benchmark_spec_v1() -> BenchmarkSpec:
    return BenchmarkSpec(
        suite="v1",
        metrics=list(_BENCHMARK_METRICS_V1),
        seeds=[42, 137],
        scoring_rule="composite_v1",
        allowed_knobs=list(_BENCHMARK_ALLOWED_KNOBS_V1),
        scenarios=[
            BenchmarkScenario("transit_cbrn", "scenarios/benchmark/transit_cbrn.yaml"),
            BenchmarkScenario("transit_shooter", "scenarios/transit_shooter.yaml"),
            BenchmarkScenario(
                "transit_mixed", "scenarios/benchmark/transit_mixed.yaml"
            ),
            BenchmarkScenario(
                "large_station_multifloor",
                "scenarios/benchmark/v1/large_station_multifloor.yaml",
            ),
            BenchmarkScenario(
                "open_air_event_funnel",
                "scenarios/benchmark/v1/open_air_event_funnel.yaml",
            ),
            BenchmarkScenario(
                "mixed_indoor_outdoor_arena",
                "scenarios/benchmark/v1/mixed_indoor_outdoor_arena.yaml",
            ),
        ],
    )


def benchmark_spec_v2() -> BenchmarkSpec:
    # v2 stresses wildland-urban interface egress in addition to active-shooter
    return BenchmarkSpec(
        suite="v2",
        metrics=list(_BENCHMARK_METRICS_V1),
        seeds=[42, 137],
        scoring_rule="composite_v1",
        allowed_knobs=list(_BENCHMARK_ALLOWED_KNOBS_V1),
        scenarios=[
            BenchmarkScenario("wildfire_wui", "scenarios/benchmark/wildfire_wui.yaml"),
            BenchmarkScenario("transit_shooter", "scenarios/transit_shooter.yaml"),
        ],
    )


def benchmark_spec_v3() -> BenchmarkSpec:
    # v3 covers urban flood and earthquake-aftershock re-evacuation
    return BenchmarkSpec(
        suite="v3",
        metrics=list(_BENCHMARK_METRICS_V1),
        seeds=[42, 137],
        scoring_rule="composite_v1",
        allowed_knobs=list(_BENCHMARK_ALLOWED_KNOBS_V1),
        scenarios=[
            BenchmarkScenario("flood_urban", "scenarios/benchmark/flood_urban.yaml"),
            BenchmarkScenario(
                "quake_aftershock", "scenarios/benchmark/quake_aftershock.yaml"
            ),
        ],
    )


def load_policy(path: str | None) -> dict[str, Any]:
    if path is None:
        return {}
    source = Path(path)
    if not source.exists():
        raise FileNotFoundError(source)
    if source.suffix.lower() == ".json":
        return json.loads(source.read_text())
    return yaml.safe_load(source.read_text()) or {}


def submit_policy(
    *,
    policy_path: str | None,
    suite: str = "v1",
    output_dir: str | Path = "out/benchmark_submission",
) -> dict[str, Any]:
    spec = _spec_for_suite(suite)
    policy = load_policy(policy_path)
    policy_hash = _hash_json(policy)
    manager = ScenarioManager()
    analytics = SimulationAnalytics()
    rows: list[dict[str, Any]] = []

    for scenario in spec.scenarios:
        for seed in spec.seeds:
            config = manager.load_config(scenario.scenario_file)
            config = _apply_policy(config, policy, spec.allowed_knobs)
            config.setdefault("simulation", {})
            config["simulation"]["random_seed"] = seed
            sim = manager.build_simulation(config)
            sim.run()
            metrics = analytics.calculate_performance_metrics(sim)
            metrics["equity_time_gap_s"] = _equity_time_gap(sim)
            metrics["benchmark_score"] = benchmark_score(metrics)
            rows.append(
                {
                    "suite": spec.suite,
                    "scenario": scenario.name,
                    "seed": seed,
                    "policy_hash": policy_hash,
                    **metrics,
                }
            )

    frame = pd.DataFrame(rows)
    leaderboard = _leaderboard(frame, policy_hash, suite=spec.suite)
    out = Path(output_dir)
    out.mkdir(parents=True, exist_ok=True)
    frame.to_csv(out / "benchmark_runs.csv", index=False)
    (out / "leaderboard.json").write_text(
        json.dumps(leaderboard, indent=2, default=str) + "\n"
    )
    manifest = reproducibility_manifest(spec, policy_hash)
    (out / "reproducibility_manifest.json").write_text(
        json.dumps(manifest, indent=2, default=str) + "\n"
    )
    write_leaderboard_site(leaderboard, Path("docs/benchmark/index.html"))
    return {"leaderboard": leaderboard, "manifest": manifest, "output_dir": str(out)}


def benchmark_score(metrics: dict[str, Any]) -> float:
    egress = 1.0 / (1.0 + float(metrics.get("mean_travel_time_s", 0.0)))
    exposure = 1.0 / (1.0 + float(metrics.get("p95_hazard_exposure", 0.0)))
    equity = 1.0 / (1.0 + float(metrics.get("equity_time_gap_s", 0.0)))
    hci = 1.0 / (
        1.0
        + float(
            metrics.get(
                "harmful_convergence_index_induced",
                metrics.get("harmful_convergence_index", 0.0),
            )
        )
    )
    return float(100.0 * (0.35 * egress + 0.30 * exposure + 0.20 * equity + 0.15 * hci))


def composite_v1_causal(
    intervention_metrics: dict[str, Any],
    no_intervention_metrics: dict[str, Any],
) -> dict[str, float]:
    """Opt-in counterfactual companion to ``benchmark_score``.

    Returns the raw composite scores for both arms plus
    ``delta_vs_no_intervention``. The delta uses simple matched-pair
    differencing (mirroring the ``ate`` estimator in
    ``chiyoda.studies.causal``); positive deltas indicate the
    intervention arm improves the composite score.

    Callers may pass aggregated dictionaries across seeds; the function
    is a pure transform and does not refit any model.
    """
    if _is_study_bundle(intervention_metrics) and _is_study_bundle(
        no_intervention_metrics
    ):
        return _composite_v1_causal_bundles(
            treated=intervention_metrics,
            baseline=no_intervention_metrics,
        )

    treated = benchmark_score(intervention_metrics)
    baseline = benchmark_score(no_intervention_metrics)
    return {
        "composite_v1_treated": treated,
        "composite_v1_no_intervention": baseline,
        "delta_vs_no_intervention": treated - baseline,
    }


def _composite_v1_causal_bundles(*, treated, baseline) -> dict[str, float]:
    from dataclasses import replace

    from chiyoda.studies.causal import compare_bundles

    treated_scored = replace(
        treated, summary=_summary_with_composite_v1(treated.summary)
    )
    baseline_scored = replace(
        baseline, summary=_summary_with_composite_v1(baseline.summary)
    )
    result = compare_bundles(
        baseline_scored,
        treated_scored,
        metrics=["composite_v1"],
        bootstrap_samples=0,
    )
    if result.empty:
        return {
            "composite_v1_treated": 0.0,
            "composite_v1_no_intervention": 0.0,
            "delta_vs_no_intervention": 0.0,
        }
    row = result.iloc[0]
    return {
        "composite_v1_treated": float(row["treated_mean"]),
        "composite_v1_no_intervention": float(row["baseline_mean"]),
        "delta_vs_no_intervention": float(row["ate"]),
    }


def _summary_with_composite_v1(summary: pd.DataFrame) -> pd.DataFrame:
    frame = summary.copy()
    frame["composite_v1"] = [
        (
            benchmark_score(row.to_dict())
            if row.get("record_type", "run") == "run"
            else pd.NA
        )
        for _, row in frame.iterrows()
    ]
    return frame


def _is_study_bundle(value: Any) -> bool:
    return hasattr(value, "summary") and hasattr(value, "tables")


def generate_reference_trajectories(
    *,
    suite: str = "v1",
    output_file: str | Path = "data/benchmark/v1/reference_trajectories.parquet",
) -> Path:
    spec = _spec_for_suite(suite)
    manager = ScenarioManager()
    rows: list[dict[str, Any]] = []
    for scenario in spec.scenarios:
        for seed in spec.seeds:
            config = manager.load_config(scenario.scenario_file)
            config.setdefault("simulation", {})
            config["simulation"]["random_seed"] = seed
            sim = manager.build_simulation(config)
            sim.run()
            for agent_id, trace in sim.agent_traces.items():
                for index, point in enumerate(trace):
                    rows.append(
                        {
                            "suite": spec.suite,
                            "scenario": scenario.name,
                            "seed": seed,
                            "agent_id": agent_id,
                            "trace_index": index,
                            "x": float(point[0]),
                            "y": float(point[1]),
                            "z": float(point[2]) if len(point) >= 3 else 0.0,
                        }
                    )
    output = Path(output_file)
    output.parent.mkdir(parents=True, exist_ok=True)
    pd.DataFrame(rows).to_parquet(output, index=False)
    return output


def reproducibility_manifest(spec: BenchmarkSpec, policy_hash: str) -> dict[str, Any]:
    return {
        "suite": spec.suite,
        "created_at_utc": datetime.now(UTC).isoformat(),
        "config_hash": _hash_json(spec.to_dict()),
        "policy_hash": policy_hash,
        "seed_set": list(spec.seeds),
        "version": __version__,
        "scenarios": [scenario.name for scenario in spec.scenarios],
    }


def write_spec_artifacts() -> None:
    root = Path("docs/benchmark")
    root.mkdir(parents=True, exist_ok=True)
    (root / "benchmark_spec_v1.schema.json").write_text(
        json.dumps(BenchmarkSpec.json_schema(), indent=2) + "\n"
    )
    for suite, builder in _SUITE_BUILDERS.items():
        spec = builder()
        (root / f"benchmark_spec_{suite}.json").write_text(
            json.dumps(spec.to_dict(), indent=2) + "\n"
        )


def write_leaderboard_site(leaderboard: dict[str, Any], output_file: Path) -> Path:
    output_file.parent.mkdir(parents=True, exist_ok=True)
    rows = "\n".join(
        "<tr>"
        f"<td>{item['policy_hash']}</td>"
        f"<td>{item['mean_score']:.3f}</td>"
        f"<td>{item.get('score_ci_low', item['mean_score']):.3f}-"
        f"{item.get('score_ci_high', item['mean_score']):.3f}</td>"
        f"<td>{item.get('tier', 'smoke')}</td>"
        f"<td>{item['run_count']}</td>"
        "</tr>"
        for item in leaderboard["entries"]
    )
    output_file.write_text(
        '<!doctype html><html><head><meta charset="utf-8"><title>Chiyoda Benchmark</title></head>'
        "<body><h1>Chiyoda Benchmark v1</h1>"
        "<table><thead><tr><th>Policy</th><th>Score</th><th>95% CI</th>"
        "<th>Tier</th><th>Runs</th></tr></thead>"
        f"<tbody>{rows}</tbody></table></body></html>\n"
    )
    return output_file


def _leaderboard(
    frame: pd.DataFrame, policy_hash: str, *, suite: str = "v1"
) -> dict[str, Any]:
    score = _score_summary(frame, bootstrap_n=BENCHMARK_BOOTSTRAP_N)
    seeds_used = _seeds_used(frame)
    return {
        "suite": suite,
        "entries": [
            {
                "policy_hash": policy_hash,
                "mean_score": score["mean_score"],
                "score_ci_low": score["score_ci_low"],
                "score_ci_high": score["score_ci_high"],
                "seeds_used": seeds_used,
                "seed_count": len(seeds_used),
                "bootstrap_n": BENCHMARK_BOOTSTRAP_N,
                "tier": (
                    "official" if len(seeds_used) >= OFFICIAL_MIN_SEEDS else "smoke"
                ),
                "official_min_seeds": OFFICIAL_MIN_SEEDS,
                "run_count": int(len(frame)),
                "scenario_breakdown": _scenario_breakdown(frame),
            }
        ],
    }


def _scenario_breakdown(frame: pd.DataFrame) -> list[dict[str, Any]]:
    if frame.empty:
        return []
    rows: list[dict[str, Any]] = []
    for scenario, group in frame.groupby("scenario", sort=True):
        score = _score_summary(group, bootstrap_n=BENCHMARK_BOOTSTRAP_N)
        seeds_used = _seeds_used(group)
        rows.append(
            {
                "scenario": str(scenario),
                "mean_score": score["mean_score"],
                "score_ci_low": score["score_ci_low"],
                "score_ci_high": score["score_ci_high"],
                "seeds_used": seeds_used,
                "seed_count": len(seeds_used),
                "bootstrap_n": BENCHMARK_BOOTSTRAP_N,
                "run_count": int(len(group)),
            }
        )
    return rows


def _score_summary(frame: pd.DataFrame, *, bootstrap_n: int) -> dict[str, float]:
    if frame.empty or "benchmark_score" not in frame:
        return {"mean_score": 0.0, "score_ci_low": 0.0, "score_ci_high": 0.0}
    seed_scores = (
        frame.groupby("seed", sort=True)["benchmark_score"].mean()
        if "seed" in frame
        else frame["benchmark_score"]
    )
    scores = pd.to_numeric(seed_scores, errors="coerce").dropna().to_numpy(dtype=float)
    if len(scores) == 0:
        return {"mean_score": 0.0, "score_ci_low": 0.0, "score_ci_high": 0.0}
    mean_score = float(np.mean(scores))
    if len(scores) < 2 or bootstrap_n <= 0:
        return {
            "mean_score": mean_score,
            "score_ci_low": mean_score,
            "score_ci_high": mean_score,
        }
    rng = np.random.default_rng(BENCHMARK_BOOTSTRAP_SEED)
    sampled = rng.choice(scores, size=(int(bootstrap_n), len(scores)), replace=True)
    means = sampled.mean(axis=1)
    return {
        "mean_score": mean_score,
        "score_ci_low": float(np.percentile(means, 2.5)),
        "score_ci_high": float(np.percentile(means, 97.5)),
    }


def _seeds_used(frame: pd.DataFrame) -> list[int]:
    if frame.empty or "seed" not in frame:
        return []
    seeds = pd.to_numeric(frame["seed"], errors="coerce").dropna().unique()
    return [int(seed) for seed in sorted(seeds)]


def _equity_time_gap(simulation) -> float:
    values = [float(agent.travel_time_s) for agent in simulation.completed_agents]
    if not values:
        return 0.0
    return float(max(values) - min(values))


def _apply_policy(
    config: dict[str, Any], policy: dict[str, Any], allowed_knobs: Iterable[str]
) -> dict[str, Any]:
    if not policy:
        return config
    overrides = policy.get("scenario_overrides", policy)
    for key in overrides:
        if key not in set(allowed_knobs):
            raise ValueError(f"Policy knob not allowed by benchmark spec: {key}")
    merged = dict(config)
    for key, value in overrides.items():
        merged[key] = value
    return merged


_SUITE_BUILDERS = {
    "v1": benchmark_spec_v1,
    "v2": benchmark_spec_v2,
    "v3": benchmark_spec_v3,
}


def _spec_for_suite(suite: str) -> BenchmarkSpec:
    builder = _SUITE_BUILDERS.get(suite)
    if builder is None:
        raise ValueError(
            f"Unknown benchmark suite: {suite!r}; available: {sorted(_SUITE_BUILDERS)}"
        )
    return builder()


def _hash_json(value: Any) -> str:
    return sha256(
        json.dumps(value, sort_keys=True, default=str).encode("utf-8")
    ).hexdigest()[:16]
