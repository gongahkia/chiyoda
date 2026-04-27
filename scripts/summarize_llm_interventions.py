#!/usr/bin/env python3
"""Summarize generated-message intervention artifacts for LLM pilot studies."""
from __future__ import annotations

import argparse
from pathlib import Path

import pandas as pd


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("study_dir", help="Study output directory, e.g. out/llm_openai_pilot")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    study_dir = Path(args.study_dir)
    interventions_path = study_dir / "tables" / "interventions.parquet"
    summary_path = study_dir / "tables" / "summary.parquet"
    if not interventions_path.exists():
        raise SystemExit(f"missing interventions table: {interventions_path}")

    interventions = pd.read_parquet(interventions_path)
    llm = interventions[interventions["policy"] == "llm_guidance"].copy()
    tables_dir = study_dir / "tables"
    tables_dir.mkdir(parents=True, exist_ok=True)

    if llm.empty:
        empty = pd.DataFrame()
        empty.to_csv(tables_dir / "llm_generation_summary.csv", index=False)
        empty.to_csv(tables_dir / "llm_validation_reasons.csv", index=False)
        print("no llm_guidance interventions found")
        return 0

    summary = _generation_summary(llm)
    summary.to_csv(tables_dir / "llm_generation_summary.csv", index=False)

    reasons = _validation_reasons(llm)
    reasons.to_csv(tables_dir / "llm_validation_reasons.csv", index=False)

    if summary_path.exists():
        comparison = _policy_comparison(pd.read_parquet(summary_path))
        comparison.to_csv(tables_dir / "llm_policy_comparison.csv", index=False)

    print(f"wrote {tables_dir / 'llm_generation_summary.csv'}")
    print(f"wrote {tables_dir / 'llm_validation_reasons.csv'}")
    if summary_path.exists():
        print(f"wrote {tables_dir / 'llm_policy_comparison.csv'}")
    return 0


def _generation_summary(llm: pd.DataFrame) -> pd.DataFrame:
    group_cols = ["variant_name", "generation_provider", "generation_model"]
    rows = []
    for keys, group in llm.groupby(group_cols, dropna=False):
        variant_name, provider, model = keys
        cache_counts = group.get("cache_status", pd.Series(dtype=str)).fillna("").value_counts()
        validation_counts = group.get("validation_status", pd.Series(dtype=str)).fillna("").value_counts()
        rows.append(
            {
                "variant_name": variant_name,
                "generation_provider": provider,
                "generation_model": model,
                "events": int(len(group)),
                "recipients": int(group["recipients"].sum()),
                "cache_hits": int(cache_counts.get("hit", 0)),
                "cache_misses": int(cache_counts.get("miss", 0)),
                "cache_disabled": int(cache_counts.get("disabled", 0)),
                "accepted": int(validation_counts.get("accepted", 0)),
                "rejected": int(validation_counts.get("rejected", 0)),
                "unique_cache_keys": int(group["cache_key"].replace("", pd.NA).dropna().nunique()),
                "fallback_events": _fallback_count(group),
                "mean_generated_confidence": _mean_column(group, "generated_confidence"),
                "recommendation_diversity": _token_diversity(group, "generated_recommended_exits"),
                "avoidance_diversity": _token_diversity(group, "generated_avoid_exits"),
                "raw_congested_recommendations": _reason_count(group, "congested_recommendation"),
                "mean_entropy_delta": float(group["entropy_delta"].mean()),
                "mean_accuracy_delta": float(group["accuracy_delta"].mean()),
            }
        )
    return pd.DataFrame(rows).sort_values(["variant_name", "generation_provider"])


def _validation_reasons(llm: pd.DataFrame) -> pd.DataFrame:
    rows = []
    for _, row in llm.iterrows():
        raw = str(row.get("validation_reasons", "") or "")
        reasons = [item for item in raw.split(";") if item]
        if not reasons:
            reasons = ["accepted"]
        for reason in reasons:
            rows.append(
                {
                    "variant_name": row["variant_name"],
                    "generation_provider": row.get("generation_provider", ""),
                    "validation_status": row.get("validation_status", ""),
                    "reason": reason,
                }
            )
    frame = pd.DataFrame(rows)
    return (
        frame.groupby(
            ["variant_name", "generation_provider", "validation_status", "reason"],
            as_index=False,
        )
        .size()
        .rename(columns={"size": "count"})
        .sort_values(["variant_name", "count"], ascending=[True, False])
    )


def _policy_comparison(summary: pd.DataFrame) -> pd.DataFrame:
    run_rows = summary[summary["record_type"].isin(["variant_aggregate", "aggregate"])].copy()
    if "agents_evacuated" not in run_rows.columns and "evacuated" in run_rows.columns:
        run_rows["agents_evacuated"] = run_rows["evacuated"]
    columns = [
        "variant_name",
        "agents_evacuated",
        "mean_travel_time_s",
        "mean_hazard_exposure",
        "information_safety_efficiency",
        "harmful_convergence_index",
        "intervention_count",
        "intervention_recipients",
    ]
    available = [column for column in columns if column in run_rows.columns]
    return run_rows[available].sort_values("variant_name")


def _fallback_count(group: pd.DataFrame) -> int:
    if "used_fallback" not in group.columns:
        return 0
    return int(group["used_fallback"].fillna(False).astype(bool).sum())


def _mean_column(group: pd.DataFrame, column: str) -> float:
    if column not in group.columns:
        return 0.0
    values = pd.to_numeric(group[column], errors="coerce").dropna()
    if values.empty:
        return 0.0
    return float(values.mean())


def _token_diversity(group: pd.DataFrame, column: str) -> int:
    if column not in group.columns:
        return 0
    tokens = set()
    for raw in group[column].fillna(""):
        for token in str(raw).split(";"):
            token = token.strip()
            if token:
                tokens.add(token)
    return len(tokens)


def _reason_count(group: pd.DataFrame, prefix: str) -> int:
    if "validation_reasons" not in group.columns:
        return 0
    count = 0
    for raw in group["validation_reasons"].fillna(""):
        count += sum(1 for reason in str(raw).split(";") if reason.startswith(prefix))
    return count


if __name__ == "__main__":
    raise SystemExit(main())
