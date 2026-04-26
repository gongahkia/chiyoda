#!/usr/bin/env python3
"""Summarize the Chiyoda regime robustness study.

The study names variants as:

    hazard_<level>__familiarity_<level>__<policy>

This script reads the exported summary table, parses those dimensions, and
writes a compact CSV sorted by hazard regime, familiarity regime, and
information-safety efficiency.
"""
from __future__ import annotations

import argparse
from pathlib import Path

import pandas as pd


METRICS = [
    "hazard_regime",
    "familiarity_regime",
    "policy",
    "agents_evacuated",
    "mean_travel_time_s",
    "mean_hazard_exposure",
    "peak_bottleneck_queue",
    "information_safety_efficiency",
    "harmful_convergence_index",
    "intervention_entropy_reduction",
    "intervention_accuracy_gain",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("study_dir", help="Exported regime robustness study directory")
    parser.add_argument(
        "-o",
        "--out",
        default=None,
        help="Output CSV path. Defaults to <study_dir>/tables/regime_summary.csv",
    )
    return parser.parse_args()


def read_summary(study_dir: Path) -> pd.DataFrame:
    tables = study_dir / "tables"
    parquet = tables / "summary.parquet"
    csv = tables / "summary.csv"
    if parquet.exists():
        return pd.read_parquet(parquet)
    if csv.exists():
        return pd.read_csv(csv)
    raise FileNotFoundError(f"No summary table found in {tables}")


def parse_variant_name(name: str) -> tuple[str, str, str]:
    parts = name.split("__")
    if len(parts) != 3:
        raise ValueError(f"Unexpected regime robustness variant name: {name}")
    hazard, familiarity, policy = parts
    return (
        hazard.removeprefix("hazard_"),
        familiarity.removeprefix("familiarity_"),
        policy,
    )


def summarize(study_dir: Path) -> pd.DataFrame:
    summary = read_summary(study_dir)
    variants = summary[summary["record_type"] == "variant_aggregate"].copy()
    if variants.empty:
        raise ValueError("No variant_aggregate rows found in summary table")

    parsed = variants["variant_name"].map(parse_variant_name)
    variants["hazard_regime"] = [item[0] for item in parsed]
    variants["familiarity_regime"] = [item[1] for item in parsed]
    variants["policy"] = [item[2] for item in parsed]

    cols = [column for column in METRICS if column in variants.columns]
    return variants[cols].sort_values(
        ["hazard_regime", "familiarity_regime", "information_safety_efficiency"],
        ascending=[True, True, False],
    )


def main() -> int:
    args = parse_args()
    study_dir = Path(args.study_dir)
    output = Path(args.out) if args.out else study_dir / "tables" / "regime_summary.csv"
    output.parent.mkdir(parents=True, exist_ok=True)
    frame = summarize(study_dir)
    frame.to_csv(output, index=False)
    print(f"wrote {output}")
    print(frame.to_string(index=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
