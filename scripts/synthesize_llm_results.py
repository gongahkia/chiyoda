#!/usr/bin/env python3
"""Build compact synthesis artifacts across completed LLM studies."""
from __future__ import annotations

import argparse
from pathlib import Path
from typing import Iterable

import pandas as pd


DEFAULT_STUDIES = {
    "medium": "out/llm_medium",
    "target_selection": "out/llm_target_selection_ablation",
    "regime_robustness": "out/llm_regime_robustness",
    "prompt_objective": "out/llm_prompt_objective_ablation",
    "budget_equivalence": "out/llm_budget_equivalence",
}

CORE_METRICS = [
    "agents_evacuated",
    "information_safety_efficiency",
    "harmful_convergence_index",
    "intervention_count",
    "intervention_recipients",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "-o",
        "--out",
        default="out/llm_synthesis",
        help="Output directory for synthesis CSV artifacts.",
    )
    for name, default in DEFAULT_STUDIES.items():
        parser.add_argument(
            f"--{name.replace('_', '-')}",
            default=default,
            help=f"Study directory for {name}.",
        )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    study_dirs = {
        name: Path(getattr(args, name))
        for name in DEFAULT_STUDIES
    }
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    synthesis = build_policy_synthesis(study_dirs)
    synthesis.to_csv(out_dir / "llm_policy_synthesis.csv", index=False)

    highlights = build_claim_highlights(synthesis)
    highlights.to_csv(out_dir / "llm_claim_highlights.csv", index=False)

    print(f"wrote {out_dir / 'llm_policy_synthesis.csv'}")
    print(f"wrote {out_dir / 'llm_claim_highlights.csv'}")
    return 0


def build_policy_synthesis(study_dirs: dict[str, Path]) -> pd.DataFrame:
    frames = []
    for study_name, study_dir in study_dirs.items():
        path = study_dir / "tables" / "llm_policy_comparison.csv"
        if not path.exists():
            continue
        frame = pd.read_csv(path)
        frame.insert(0, "study", study_name)
        frame["variant_family"] = frame["variant_name"].map(_variant_family)
        frame["llm_provider"] = frame["variant_name"].map(_llm_provider)
        frame["replay_pair"] = frame["variant_name"].map(_replay_pair)
        frames.append(frame)
    if not frames:
        return pd.DataFrame(columns=["study", "variant_name", "variant_family", *CORE_METRICS])
    combined = pd.concat(frames, ignore_index=True, sort=False)
    ordered = [
        "study",
        "variant_name",
        "variant_family",
        "llm_provider",
        "replay_pair",
        *[metric for metric in CORE_METRICS if metric in combined.columns],
    ]
    extra = [column for column in combined.columns if column not in ordered]
    return combined[ordered + extra].sort_values(["study", "variant_family", "variant_name"])


def build_claim_highlights(synthesis: pd.DataFrame) -> pd.DataFrame:
    rows: list[dict[str, object]] = []
    _append_pair_highlight(
        rows,
        synthesis,
        study="prompt_objective",
        baseline="static_beacon",
        variant="llm_openai_safety",
        claim="sparse_safety_vs_static",
    )
    _append_pair_highlight(
        rows,
        synthesis,
        study="prompt_objective",
        baseline="llm_openai_safety",
        variant="llm_openai_hazard_avoidance",
        claim="hazard_prompt_vs_safety_prompt",
    )
    _append_pair_highlight(
        rows,
        synthesis,
        study="prompt_objective",
        baseline="llm_openai_safety",
        variant="llm_openai_anti_convergence",
        claim="anti_convergence_prompt_vs_safety_prompt",
    )
    _append_pair_highlight(
        rows,
        synthesis,
        study="budget_equivalence",
        baseline="llm_openai_sparse",
        variant="llm_openai_static_equivalent",
        claim="static_budget_vs_sparse_llm",
    )
    _append_pair_highlight(
        rows,
        synthesis,
        study="budget_equivalence",
        baseline="llm_openai_sparse",
        variant="llm_openai_entropy_equivalent",
        claim="entropy_budget_vs_sparse_llm",
    )
    return pd.DataFrame(rows)


def _append_pair_highlight(
    rows: list[dict[str, object]],
    synthesis: pd.DataFrame,
    *,
    study: str,
    baseline: str,
    variant: str,
    claim: str,
) -> None:
    base = _one_row(synthesis, study, baseline)
    test = _one_row(synthesis, study, variant)
    if base is None or test is None:
        return
    rows.append(
        {
            "claim": claim,
            "study": study,
            "baseline_variant": baseline,
            "test_variant": variant,
            "baseline_ise": _value(base, "information_safety_efficiency"),
            "test_ise": _value(test, "information_safety_efficiency"),
            "ise_delta": _value(test, "information_safety_efficiency")
            - _value(base, "information_safety_efficiency"),
            "baseline_hci": _value(base, "harmful_convergence_index"),
            "test_hci": _value(test, "harmful_convergence_index"),
            "hci_delta": _value(test, "harmful_convergence_index")
            - _value(base, "harmful_convergence_index"),
            "baseline_recipients": _value(base, "intervention_recipients"),
            "test_recipients": _value(test, "intervention_recipients"),
            "recipient_ratio": _safe_ratio(
                _value(test, "intervention_recipients"),
                _value(base, "intervention_recipients"),
            ),
        }
    )


def _one_row(synthesis: pd.DataFrame, study: str, variant: str) -> pd.Series | None:
    rows = synthesis[(synthesis["study"] == study) & (synthesis["variant_name"] == variant)]
    if rows.empty:
        return None
    return rows.iloc[0]


def _variant_family(variant: str) -> str:
    if variant.startswith("llm_replay"):
        return "llm_replay"
    if variant.startswith("llm_openai"):
        return "llm_openai"
    if variant.startswith("llm_template") or variant.startswith("llm_target"):
        return "llm_template"
    if variant in {"static_beacon", "global_broadcast", "entropy_targeted", "bottleneck_avoidance"}:
        return "deterministic"
    return "other"


def _llm_provider(variant: str) -> str:
    if variant.startswith("llm_openai"):
        return "openai"
    if variant.startswith("llm_replay"):
        return "replay"
    if variant.startswith("llm_template") or variant.startswith("llm_target"):
        return "template"
    return ""


def _replay_pair(variant: str) -> str:
    if variant.startswith("llm_replay"):
        return variant.replace("llm_replay", "llm_openai", 1)
    if variant.startswith("llm_openai"):
        return variant.replace("llm_openai", "llm_replay", 1)
    return ""


def _value(row: pd.Series, column: str) -> float:
    return float(row[column]) if column in row and pd.notna(row[column]) else 0.0


def _safe_ratio(numerator: float, denominator: float) -> float:
    if abs(denominator) < 1e-12:
        return 0.0
    return float(numerator / denominator)


if __name__ == "__main__":
    raise SystemExit(main())
