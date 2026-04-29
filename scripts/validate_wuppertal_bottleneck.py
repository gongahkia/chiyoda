#!/usr/bin/env python3
"""Compare Chiyoda trajectory output with the Wuppertal bottleneck reference."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

import pandas as pd

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from chiyoda.analysis.external_validation import (  # noqa: E402
    compare_bottleneck_flow,
    load_petrack_trajectory,
    summarize_bottleneck_flow,
)
from chiyoda.analysis.trajectory_reference import load_trajectory_table  # noqa: E402


DEFAULT_REFERENCE = ROOT / "data/external/wuppertal_bottleneck_2018/040_c_56_h-.txt"
DEFAULT_METADATA = ROOT / "data/external/wuppertal_bottleneck_2018/metadata.json"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("simulated", help="Chiyoda agent_steps table or study bundle")
    parser.add_argument(
        "--reference",
        default=str(DEFAULT_REFERENCE),
        help="PeTrack bottleneck reference trajectory.",
    )
    parser.add_argument(
        "-o",
        "--out",
        default="out/wuppertal_bottleneck_validation",
        help="Output directory for validation tables.",
    )
    parser.add_argument("--frame-rate-hz", type=float, default=25.0)
    parser.add_argument(
        "--simulated-line",
        nargs=4,
        type=float,
        metavar=("X1", "Y1", "X2", "Y2"),
        default=(4.0, 6.0, 6.0, 6.0),
        help="Measurement line for Chiyoda coordinates.",
    )
    parser.add_argument(
        "--reference-line",
        nargs=4,
        type=float,
        metavar=("X1", "Y1", "X2", "Y2"),
        default=(0.35, 0.0, -0.35, 0.0),
        help="Measurement line for reference coordinates.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    simulated = load_trajectory_table(args.simulated)
    reference = load_petrack_trajectory(args.reference, frame_rate_hz=args.frame_rate_hz)

    simulated_line = ((args.simulated_line[0], args.simulated_line[1]), (args.simulated_line[2], args.simulated_line[3]))
    reference_line = ((args.reference_line[0], args.reference_line[1]), (args.reference_line[2], args.reference_line[3]))
    simulated_summary = summarize_bottleneck_flow(
        simulated,
        source="chiyoda",
        measurement_line=simulated_line,
    )
    reference_summary = summarize_bottleneck_flow(
        reference,
        source="wuppertal_2018",
        measurement_line=reference_line,
    )
    comparison = compare_bottleneck_flow(simulated_summary, reference_summary)

    summaries = pd.concat(
        [reference_summary.to_frame(), simulated_summary.to_frame()],
        ignore_index=True,
    )
    summaries.to_csv(out_dir / "bottleneck_flow_summary.csv", index=False)
    comparison.to_csv(out_dir / "bottleneck_flow_comparison.csv", index=False)
    if DEFAULT_METADATA.exists():
        (out_dir / "reference_metadata.json").write_text(
            json.dumps(json.loads(DEFAULT_METADATA.read_text()), indent=2) + "\n"
        )
    print(f"wrote {out_dir / 'bottleneck_flow_summary.csv'}")
    print(f"wrote {out_dir / 'bottleneck_flow_comparison.csv'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
