#!/usr/bin/env python3
"""Render a compact robustness heatmap for the paper."""
from __future__ import annotations

import argparse
from pathlib import Path

import matplotlib.pyplot as plt
import pandas as pd


HAZARDS = ["low", "medium", "high"]
FAMILIARITIES = ["low", "mixed", "high"]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "summary_csv",
        help="Path to out/regime_robustness_900/tables/regime_summary.csv",
    )
    parser.add_argument(
        "-o",
        "--output",
        default="figures/regime-robustness-heatmap.pdf",
        help="Output figure path",
    )
    return parser.parse_args()


def pivot_metric(frame: pd.DataFrame, policy: str, metric: str) -> pd.DataFrame:
    subset = frame[frame["policy"] == policy]
    pivot = subset.pivot(
        index="hazard_regime",
        columns="familiarity_regime",
        values=metric,
    )
    return pivot.loc[HAZARDS, FAMILIARITIES]


def annotate(ax: plt.Axes, values: pd.DataFrame, fmt: str) -> None:
    for row, hazard in enumerate(HAZARDS):
        for col, familiarity in enumerate(FAMILIARITIES):
            ax.text(
                col,
                row,
                fmt.format(values.loc[hazard, familiarity]),
                ha="center",
                va="center",
                color="#1a1a1a",
                fontsize=9,
            )


def main() -> int:
    args = parse_args()
    summary = pd.read_csv(args.summary_csv)
    static_ise = pivot_metric(summary, "static_beacon", "information_safety_efficiency")
    global_ise = pivot_metric(summary, "global_broadcast", "information_safety_efficiency")
    ratio = global_ise / static_ise

    fig, axes = plt.subplots(1, 2, figsize=(7.0, 3.0), constrained_layout=True)

    im0 = axes[0].imshow(static_ise.to_numpy(), cmap="YlGnBu", vmin=0.0)
    axes[0].set_title("Static beacon ISE")
    annotate(axes[0], static_ise, "{:.4f}")
    fig.colorbar(im0, ax=axes[0], fraction=0.046, pad=0.04)

    im1 = axes[1].imshow(ratio.to_numpy(), cmap="YlOrRd", vmin=0.0, vmax=1.0)
    axes[1].set_title("Global / static ISE")
    annotate(axes[1], ratio, "{:.2f}")
    fig.colorbar(im1, ax=axes[1], fraction=0.046, pad=0.04)

    for ax in axes:
        ax.set_xticks(range(len(FAMILIARITIES)), [item.title() for item in FAMILIARITIES])
        ax.set_yticks(range(len(HAZARDS)), [item.title() for item in HAZARDS])
        ax.set_xlabel("Population familiarity")
        ax.set_ylabel("Hazard regime")
        ax.tick_params(length=0)

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output, bbox_inches="tight")
    plt.close(fig)
    print(f"wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
