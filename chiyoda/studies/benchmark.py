"""Benchmark suite definitions, scoring, and submission helpers."""

from __future__ import annotations

import json
import re
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
from chiyoda.scenarios.assertions import evaluate_scenario_assertions
from chiyoda.scenarios.calibration_audit import build_calibration_audit
from chiyoda.scenarios.geometry_audit import build_geometry_audit
from chiyoda.scenarios.hazard_audit import build_hazard_audit
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios.validation import validate_scenario_config


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
SUBMISSION_SCHEMA_VERSION = "chiyoda-benchmark-submission-v1"
_HASH_PATTERN = re.compile(r"^[0-9a-f]{16,64}$")
_CLAIM_TIERS = {"smoke", "diagnostic", "benchmark_grade"}
_SUBMISSION_REQUIRED_FIELDS = {
    "schema_version",
    "suite",
    "submitter",
    "policy_hash",
    "config_hash",
    "env_version",
    "seed_set",
    "overall",
    "scenarios",
}
_SUBMISSION_ALLOWED_FIELDS = _SUBMISSION_REQUIRED_FIELDS | {
    "created_at_utc",
    "command",
    "artifacts",
    "notes",
}


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
        payload = json.loads(source.read_text())
    else:
        payload = yaml.safe_load(source.read_text()) or {}
    if not isinstance(payload, dict):
        raise ValueError("benchmark policy must be a mapping")
    return dict(payload)


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
            evidence = _run_validation_evidence(config, sim, manager)
            rows.append(
                {
                    "suite": spec.suite,
                    "scenario": scenario.name,
                    "seed": seed,
                    "policy_hash": policy_hash,
                    **evidence,
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


def _composite_v1_causal_bundles(*, treated: Any, baseline: Any) -> dict[str, float]:
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


def validate_submission_file(path: str | Path) -> dict[str, Any]:
    source = Path(path)
    try:
        payload = json.loads(source.read_text())
    except Exception as exc:
        return {
            "ok": False,
            "submission_file": str(source),
            "issues": [{"path": "$", "message": f"invalid JSON: {exc}"}],
        }
    result = validate_submission(payload)
    result["submission_file"] = str(source)
    return result


def validate_submission(payload: Any) -> dict[str, Any]:
    issues: list[dict[str, str]] = []
    if not isinstance(payload, dict):
        _issue(issues, "$", "submission must be a JSON object")
        return {"ok": False, "issues": issues}

    missing = sorted(_SUBMISSION_REQUIRED_FIELDS - set(payload))
    for field_name in missing:
        _issue(issues, f"$.{field_name}", "required field missing")
    for field_name in sorted(set(payload) - _SUBMISSION_ALLOWED_FIELDS):
        _issue(issues, f"$.{field_name}", "unknown field")

    if payload.get("schema_version") != SUBMISSION_SCHEMA_VERSION:
        _issue(
            issues,
            "$.schema_version",
            f"must be {SUBMISSION_SCHEMA_VERSION}",
        )
    _validate_non_empty_string(payload.get("submitter"), "$.submitter", issues)

    suite = payload.get("suite")
    spec = None
    if isinstance(suite, str) and suite in _SUITE_BUILDERS:
        spec = _spec_for_suite(suite)
    else:
        _issue(issues, "$.suite", f"must be one of {sorted(_SUITE_BUILDERS)}")

    _validate_hash(payload.get("policy_hash"), "$.policy_hash", issues)
    _validate_hash(payload.get("config_hash"), "$.config_hash", issues)
    if spec is not None and payload.get("config_hash") != _hash_json(spec.to_dict()):
        _issue(issues, "$.config_hash", "does not match benchmark suite config hash")

    seed_set = _validate_seed_array(payload.get("seed_set"), "$.seed_set", issues)
    _validate_env_version(payload.get("env_version"), issues)
    _validate_overall(payload.get("overall"), issues)
    _validate_scenarios(payload.get("scenarios"), spec, seed_set, issues)

    return {"ok": len(issues) == 0, "issues": issues}


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
        f"<td>{item.get('claim_tier', 'smoke')}</td>"
        f"<td>{item['run_count']}</td>"
        "</tr>"
        for item in leaderboard["entries"]
    )
    output_file.write_text(
        '<!doctype html><html><head><meta charset="utf-8"><title>Chiyoda Benchmark</title></head>'
        "<body><h1>Chiyoda Benchmark v1</h1>"
        "<table><thead><tr><th>Policy</th><th>Score</th><th>95% CI</th>"
        "<th>Tier</th><th>Claim</th><th>Runs</th></tr></thead>"
        f"<tbody>{rows}</tbody></table></body></html>\n"
    )
    return output_file


def _leaderboard(
    frame: pd.DataFrame, policy_hash: str, *, suite: str = "v1"
) -> dict[str, Any]:
    score = _score_summary(frame, bootstrap_n=BENCHMARK_BOOTSTRAP_N)
    seeds_used = _seeds_used(frame)
    claim = _claim_assessment(frame)
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
                "claim_tier": claim["claim_tier"],
                "claim_limitations": claim["claim_limitations"],
                "validation_evidence": claim["validation_evidence"],
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
        claim = _claim_assessment(group)
        rows.append(
            {
                "scenario": str(scenario),
                "mean_score": score["mean_score"],
                "score_ci_low": score["score_ci_low"],
                "score_ci_high": score["score_ci_high"],
                "seeds_used": seeds_used,
                "seed_count": len(seeds_used),
                "bootstrap_n": BENCHMARK_BOOTSTRAP_N,
                "claim_tier": claim["claim_tier"],
                "claim_limitations": claim["claim_limitations"],
                "validation_evidence": claim["validation_evidence"],
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


def _run_validation_evidence(
    config: dict[str, Any], simulation: Any, manager: Any
) -> dict[str, Any]:
    validation = validate_scenario_config(config, manager=manager)
    calibration = build_calibration_audit(config)
    geometry = build_geometry_audit(config, manager=manager)
    hazard = build_hazard_audit(config)
    assertions = evaluate_scenario_assertions(config, simulation)
    return {
        "scenario_validation_ok": not validation.has_errors,
        "scenario_validation_issue_count": len(validation.issues),
        "calibration_audit_ok": bool(calibration["ok"]),
        "calibration_audit_issue_count": len(calibration["issues"]),
        "generic_social_force_profile": bool(
            calibration["social_force"]["generic_legacy"]
        ),
        "geometry_audit_ok": bool(geometry["ok"]),
        "geometry_audit_issue_count": len(geometry["issues"]),
        "hazard_audit_ok": bool(hazard["ok"]),
        "hazard_audit_issue_count": len(hazard["issues"]),
        "stylized_hazard_count": int(hazard["counts"]["stylized"]),
        "imported_hazard_count": int(hazard["counts"]["imported_fields"]),
        "runtime_assertions_present": bool(config.get("assertions")),
        "runtime_assertions_ok": assertions.ok,
        "runtime_assertion_issue_count": len(assertions.issues),
    }


def _claim_assessment(frame: pd.DataFrame) -> dict[str, Any]:
    seeds_used = _seeds_used(frame)
    evidence = {
        "seed_count": len(seeds_used),
        "official_min_seeds": OFFICIAL_MIN_SEEDS,
        "scenario_validation_ok": _column_all_true(frame, "scenario_validation_ok"),
        "calibration_audit_ok": _column_all_true(frame, "calibration_audit_ok"),
        "generic_social_force_profile": _column_any_true(
            frame, "generic_social_force_profile"
        ),
        "geometry_audit_ok": _column_all_true(frame, "geometry_audit_ok"),
        "hazard_audit_ok": _column_all_true(frame, "hazard_audit_ok"),
        "stylized_hazard_count": _column_int_sum(frame, "stylized_hazard_count"),
        "imported_hazard_count": _column_int_sum(frame, "imported_hazard_count"),
        "runtime_assertions_present": _column_all_true(
            frame, "runtime_assertions_present"
        ),
        "runtime_assertions_ok": _column_all_true(frame, "runtime_assertions_ok"),
    }
    limitations: list[str] = []
    if evidence["seed_count"] < OFFICIAL_MIN_SEEDS:
        limitations.append(
            f"seed_count {evidence['seed_count']} < official_min_seeds {OFFICIAL_MIN_SEEDS}"
        )
    if not evidence["scenario_validation_ok"]:
        limitations.append(
            "one or more scenario static validations failed or are missing"
        )
    if not evidence["calibration_audit_ok"]:
        limitations.append("one or more calibration audits failed or are missing")
    if evidence["generic_social_force_profile"]:
        limitations.append("one or more scenarios use generic social-force defaults")
    if not evidence["geometry_audit_ok"]:
        limitations.append("one or more geometry audits failed or are missing")
    if not evidence["hazard_audit_ok"]:
        limitations.append("one or more hazard audits failed or are missing")
    if evidence["stylized_hazard_count"] > 0:
        limitations.append("one or more hazards use stylized built-in dynamics")
    if not evidence["runtime_assertions_present"]:
        limitations.append("one or more scenarios have no runtime assertions")
    if not evidence["runtime_assertions_ok"]:
        limitations.append("one or more runtime assertion checks failed or are missing")
    if evidence["seed_count"] < OFFICIAL_MIN_SEEDS:
        claim_tier = "smoke"
    elif limitations:
        claim_tier = "diagnostic"
    else:
        claim_tier = "benchmark_grade"
    return {
        "claim_tier": claim_tier,
        "claim_limitations": limitations,
        "validation_evidence": evidence,
    }


def _column_all_true(frame: pd.DataFrame, column: str) -> bool:
    if frame.empty or column not in frame:
        return False
    return all(bool(value) for value in frame[column].fillna(False).tolist())


def _column_any_true(frame: pd.DataFrame, column: str) -> bool:
    if frame.empty or column not in frame:
        return False
    return any(bool(value) for value in frame[column].fillna(False).tolist())


def _column_int_sum(frame: pd.DataFrame, column: str) -> int:
    if frame.empty or column not in frame:
        return 0
    return int(pd.to_numeric(frame[column], errors="coerce").fillna(0).sum())


def _equity_time_gap(simulation: Any) -> float:
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


def _issue(issues: list[dict[str, str]], path: str, message: str) -> None:
    issues.append({"path": path, "message": message})


def _validate_hash(value: Any, path: str, issues: list[dict[str, str]]) -> None:
    if not isinstance(value, str) or not _HASH_PATTERN.fullmatch(value):
        _issue(issues, path, "must be a 16-64 character lowercase hex string")


def _validate_non_empty_string(
    value: Any, path: str, issues: list[dict[str, str]]
) -> None:
    if not isinstance(value, str) or not value:
        _issue(issues, path, "must be a non-empty string")


def _validate_seed_array(
    value: Any, path: str, issues: list[dict[str, str]]
) -> list[int]:
    if not isinstance(value, list) or len(value) == 0:
        _issue(issues, path, "must be a non-empty integer array")
        return []
    if not all(isinstance(seed, int) and not isinstance(seed, bool) for seed in value):
        _issue(issues, path, "all seeds must be integers")
        return []
    if len(set(value)) != len(value):
        _issue(issues, path, "seeds must be unique")
    return [int(seed) for seed in value]


def _validate_env_version(value: Any, issues: list[dict[str, str]]) -> None:
    if not isinstance(value, dict):
        _issue(issues, "$.env_version", "must be an object")
        return
    for field_name in ("chiyoda", "python", "platform"):
        if not isinstance(value.get(field_name), str) or not value.get(field_name):
            _issue(issues, f"$.env_version.{field_name}", "must be a non-empty string")


def _validate_score_block(value: Any, path: str, issues: list[dict[str, str]]) -> None:
    if not isinstance(value, dict):
        _issue(issues, path, "must be an object")
        return
    for field_name in ("mean_score", "score_ci_low", "score_ci_high"):
        score = value.get(field_name)
        if not isinstance(score, (int, float)) or not np.isfinite(float(score)):
            _issue(issues, f"{path}.{field_name}", "must be a finite number")
    if all(
        field_name in value
        for field_name in ("mean_score", "score_ci_low", "score_ci_high")
    ):
        low = float(value["score_ci_low"])
        mean = float(value["mean_score"])
        high = float(value["score_ci_high"])
        if not low <= mean <= high:
            _issue(issues, path, "score CI must satisfy low <= mean <= high")
    run_count = value.get("run_count")
    if not isinstance(run_count, int) or isinstance(run_count, bool) or run_count < 1:
        _issue(issues, f"{path}.run_count", "must be a positive integer")
    _validate_claim_metadata(value, path, issues)


def _validate_claim_metadata(
    value: Any, path: str, issues: list[dict[str, str]]
) -> None:
    if not isinstance(value, dict):
        return
    claim_tier = value.get("claim_tier")
    if claim_tier is not None and claim_tier not in _CLAIM_TIERS:
        _issue(
            issues,
            f"{path}.claim_tier",
            f"must be one of {sorted(_CLAIM_TIERS)}",
        )
    limitations = value.get("claim_limitations")
    if limitations is not None and (
        not isinstance(limitations, list)
        or not all(isinstance(item, str) for item in limitations)
    ):
        _issue(issues, f"{path}.claim_limitations", "must be an array of strings")
    evidence = value.get("validation_evidence")
    if evidence is not None and not isinstance(evidence, dict):
        _issue(issues, f"{path}.validation_evidence", "must be an object")


def _validate_overall(value: Any, issues: list[dict[str, str]]) -> None:
    _validate_score_block(value, "$.overall", issues)
    if not isinstance(value, dict):
        return
    if value.get("tier") not in {"smoke", "official"}:
        _issue(issues, "$.overall.tier", "must be smoke or official")


def _validate_scenarios(
    value: Any,
    spec: BenchmarkSpec | None,
    seed_set: list[int],
    issues: list[dict[str, str]],
) -> None:
    if not isinstance(value, list) or len(value) == 0:
        _issue(issues, "$.scenarios", "must be a non-empty array")
        return
    names: list[str] = []
    for index, item in enumerate(value):
        path = f"$.scenarios[{index}]"
        if not isinstance(item, dict):
            _issue(issues, path, "must be an object")
            continue
        name = item.get("scenario")
        if not isinstance(name, str) or not name:
            _issue(issues, f"{path}.scenario", "must be a non-empty string")
        else:
            names.append(name)
        _validate_score_block(item, path, issues)
        seeds_used = _validate_seed_array(
            item.get("seeds_used"), f"{path}.seeds_used", issues
        )
        if seed_set and sorted(seeds_used) != sorted(seed_set):
            _issue(issues, f"{path}.seeds_used", "must match root seed_set")
    if len(set(names)) != len(names):
        _issue(issues, "$.scenarios", "scenario names must be unique")
    if spec is not None:
        expected = {scenario.name for scenario in spec.scenarios}
        actual = set(names)
        if actual != expected:
            missing = sorted(expected - actual)
            extra = sorted(actual - expected)
            detail = f"missing={missing} extra={extra}"
            _issue(issues, "$.scenarios", f"must match suite scenarios: {detail}")
