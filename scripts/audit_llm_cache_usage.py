#!/usr/bin/env python3
"""Audit cached OpenAI LLM generation records and token usage."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import pandas as pd


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--cache-root",
        default="out/llm_cache",
        help="Root directory containing LLM cache subdirectories.",
    )
    parser.add_argument(
        "--population-cache-root",
        default="out/population_calibration_cache",
        help="Root directory containing generated population calibration cache records.",
    )
    parser.add_argument(
        "-o",
        "--out",
        default="out/llm_synthesis",
        help="Output directory for cache usage audit CSV artifacts.",
    )
    parser.add_argument(
        "--input-usd-per-mtok",
        type=float,
        default=0.0,
        help="Optional input-token price in USD per million tokens.",
    )
    parser.add_argument(
        "--output-usd-per-mtok",
        type=float,
        default=0.0,
        help="Optional output-token price in USD per million tokens.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    records = collect_cache_records(Path(args.cache_root))
    summary = summarize_cache_records(
        records,
        input_usd_per_mtok=args.input_usd_per_mtok,
        output_usd_per_mtok=args.output_usd_per_mtok,
    )
    totals = summarize_totals(summary)
    population_records = collect_population_calibration_records(Path(args.population_cache_root))
    population_summary = summarize_population_calibration_records(
        population_records,
        input_usd_per_mtok=args.input_usd_per_mtok,
        output_usd_per_mtok=args.output_usd_per_mtok,
    )
    population_totals = summarize_totals(population_summary)

    summary.to_csv(out_dir / "llm_cache_usage.csv", index=False)
    totals.to_csv(out_dir / "llm_cache_usage_totals.csv", index=False)
    population_records.to_csv(out_dir / "generated_population_calibration_cache_records.csv", index=False)
    population_summary.to_csv(out_dir / "generated_population_calibration_cache_usage.csv", index=False)
    population_totals.to_csv(out_dir / "generated_population_calibration_cache_usage_totals.csv", index=False)
    print(f"wrote {out_dir / 'llm_cache_usage.csv'}")
    print(f"wrote {out_dir / 'llm_cache_usage_totals.csv'}")
    print(f"wrote {out_dir / 'generated_population_calibration_cache_records.csv'}")
    print(f"wrote {out_dir / 'generated_population_calibration_cache_usage.csv'}")
    print(f"wrote {out_dir / 'generated_population_calibration_cache_usage_totals.csv'}")
    return 0


def collect_cache_records(cache_root: Path) -> pd.DataFrame:
    rows: list[dict[str, Any]] = []
    if not cache_root.exists():
        return pd.DataFrame()
    for path in sorted(cache_root.glob("**/*.json")):
        payload = json.loads(path.read_text())
        message = payload.get("message", {})
        validation = payload.get("validation", {})
        raw = message.get("raw_response", {}) or {}
        usage = raw.get("usage", {}) or {}
        provider = str(message.get("provider", ""))
        model = str(message.get("model", ""))
        status = "accepted" if validation.get("accepted") else "rejected"
        rows.append(
            {
                "cache_dir": str(path.parent.relative_to(cache_root)),
                "cache_key": payload.get("cache_key", path.stem),
                "provider": provider,
                "model": model,
                "validation_status": status,
                "validation_reasons": ";".join(str(item) for item in validation.get("reasons", []) or []),
                "input_tokens": _usage_int(usage, "input_tokens", "prompt_tokens"),
                "output_tokens": _usage_int(usage, "output_tokens", "completion_tokens"),
                "total_tokens": _usage_int(usage, "total_tokens"),
                "has_usage": bool(usage),
                "path": str(path),
            }
        )
    return pd.DataFrame(rows)


def summarize_cache_records(
    records: pd.DataFrame,
    *,
    input_usd_per_mtok: float = 0.0,
    output_usd_per_mtok: float = 0.0,
) -> pd.DataFrame:
    if records.empty:
        return pd.DataFrame(
            columns=[
                "cache_dir",
                "provider",
                "model",
                "records",
                "accepted",
                "rejected",
                "records_with_usage",
                "input_tokens",
                "output_tokens",
                "total_tokens",
                "estimated_usd",
            ]
        )
    grouped = (
        records.groupby(["cache_dir", "provider", "model"], dropna=False)
        .agg(
            records=("cache_key", "count"),
            accepted=("validation_status", lambda values: int((values == "accepted").sum())),
            rejected=("validation_status", lambda values: int((values == "rejected").sum())),
            records_with_usage=("has_usage", "sum"),
            input_tokens=("input_tokens", "sum"),
            output_tokens=("output_tokens", "sum"),
            total_tokens=("total_tokens", "sum"),
        )
        .reset_index()
    )
    grouped["estimated_usd"] = _estimate_usd(
        grouped["input_tokens"],
        grouped["output_tokens"],
        input_usd_per_mtok,
        output_usd_per_mtok,
    )
    return grouped.sort_values(["cache_dir", "provider", "model"])


def collect_population_calibration_records(cache_root: Path) -> pd.DataFrame:
    rows: list[dict[str, Any]] = []
    if not cache_root.exists():
        return _empty_population_records()
    for path in sorted(cache_root.glob("**/*.json")):
        payload = json.loads(path.read_text())
        if not {"calibration", "validation"}.issubset(payload):
            continue
        request = payload.get("request", {}) or {}
        calibration = payload.get("calibration", {}) or {}
        validation = payload.get("validation", {}) or {}
        application = payload.get("application", {}) or {}
        raw = calibration.get("raw_response", {}) or {}
        usage = raw.get("usage", {}) or {}
        if not usage and isinstance(raw.get("response"), dict):
            usage = raw["response"].get("usage", {}) or {}

        proposed_targets = _proposed_population_targets(calibration)
        rows.append(
            {
                "cache_dir": str(path.parent.relative_to(cache_root)),
                "cache_key": payload.get("cache_key", path.stem),
                "scenario_name": request.get("scenario_name", ""),
                "provider": str(calibration.get("provider", "")),
                "model": str(calibration.get("model", "")),
                "validation_status": "accepted" if validation.get("accepted") else "rejected",
                "validation_reasons": ";".join(str(item) for item in validation.get("reasons", []) or []),
                "confidence": float(calibration.get("confidence", 0.0) or 0.0),
                "abstain": bool(calibration.get("abstain", False)),
                "allowed_targets": ";".join(str(item) for item in request.get("allowed_targets", []) or []),
                "proposed_targets": ";".join(proposed_targets),
                "applied_targets": ";".join(str(item) for item in application.get("applied_targets", []) or []),
                "skipped_overwrite_attempts": ";".join(str(item) for item in application.get("skipped", []) or []),
                "input_tokens": _usage_int(usage, "input_tokens", "prompt_tokens"),
                "output_tokens": _usage_int(usage, "output_tokens", "completion_tokens"),
                "total_tokens": _usage_int(usage, "total_tokens"),
                "has_usage": bool(usage),
                "path": str(path),
            }
        )
    if not rows:
        return _empty_population_records()
    return pd.DataFrame(rows)


def summarize_population_calibration_records(
    records: pd.DataFrame,
    *,
    input_usd_per_mtok: float = 0.0,
    output_usd_per_mtok: float = 0.0,
) -> pd.DataFrame:
    if records.empty:
        return pd.DataFrame(
            columns=[
                "cache_dir",
                "provider",
                "model",
                "records",
                "accepted",
                "rejected",
                "records_with_usage",
                "input_tokens",
                "output_tokens",
                "total_tokens",
                "estimated_usd",
            ]
        )
    grouped = (
        records.groupby(["cache_dir", "provider", "model"], dropna=False)
        .agg(
            records=("cache_key", "count"),
            accepted=("validation_status", lambda values: int((values == "accepted").sum())),
            rejected=("validation_status", lambda values: int((values == "rejected").sum())),
            records_with_usage=("has_usage", "sum"),
            input_tokens=("input_tokens", "sum"),
            output_tokens=("output_tokens", "sum"),
            total_tokens=("total_tokens", "sum"),
        )
        .reset_index()
    )
    grouped["estimated_usd"] = _estimate_usd(
        grouped["input_tokens"],
        grouped["output_tokens"],
        input_usd_per_mtok,
        output_usd_per_mtok,
    )
    return grouped.sort_values(["cache_dir", "provider", "model"])


def summarize_totals(summary: pd.DataFrame) -> pd.DataFrame:
    if summary.empty:
        return pd.DataFrame()
    totals = {
        "cache_dir": "ALL",
        "provider": "ALL",
        "model": "ALL",
        "records": int(summary["records"].sum()),
        "accepted": int(summary["accepted"].sum()),
        "rejected": int(summary["rejected"].sum()),
        "records_with_usage": int(summary["records_with_usage"].sum()),
        "input_tokens": int(summary["input_tokens"].sum()),
        "output_tokens": int(summary["output_tokens"].sum()),
        "total_tokens": int(summary["total_tokens"].sum()),
        "estimated_usd": float(summary["estimated_usd"].sum()),
    }
    return pd.DataFrame([totals])


def _usage_int(usage: dict[str, Any], *keys: str) -> int:
    for key in keys:
        value = usage.get(key)
        if value is not None:
            return int(value)
    return 0


def _proposed_population_targets(calibration: dict[str, Any]) -> list[str]:
    targets: list[str] = []
    if calibration.get("cohorts"):
        targets.append("cohort_mix")
    if calibration.get("parameter_priors"):
        targets.append("parameter_priors")
    if calibration.get("scenario_metadata"):
        targets.append("scenario_metadata")
    return targets


def _empty_population_records() -> pd.DataFrame:
    return pd.DataFrame(
        columns=[
            "cache_dir",
            "cache_key",
            "scenario_name",
            "provider",
            "model",
            "validation_status",
            "validation_reasons",
            "confidence",
            "abstain",
            "allowed_targets",
            "proposed_targets",
            "applied_targets",
            "skipped_overwrite_attempts",
            "input_tokens",
            "output_tokens",
            "total_tokens",
            "has_usage",
            "path",
        ]
    )


def _estimate_usd(
    input_tokens: pd.Series,
    output_tokens: pd.Series,
    input_usd_per_mtok: float,
    output_usd_per_mtok: float,
) -> pd.Series:
    input_cost = input_tokens.astype(float) * float(input_usd_per_mtok) / 1_000_000.0
    output_cost = output_tokens.astype(float) * float(output_usd_per_mtok) / 1_000_000.0
    return input_cost + output_cost


if __name__ == "__main__":
    raise SystemExit(main())
