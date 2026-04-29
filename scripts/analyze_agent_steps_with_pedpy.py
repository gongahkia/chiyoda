#!/usr/bin/env python3
"""Prepare Chiyoda agent_steps exports for optional PedPy analysis."""
from __future__ import annotations

import argparse
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from chiyoda.analysis.trajectory_reference import (
    external_tool_trajectory_frame,
    load_trajectory_table,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("agent_steps", help="Chiyoda agent_steps table or study bundle")
    parser.add_argument(
        "-o",
        "--out",
        default="out/pedpy_agent_steps.csv",
        help="Output CSV with pedestrian_id, frame, time_s, x, y columns.",
    )
    parser.add_argument("--frame-rate-hz", type=float, default=10.0)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    table = load_trajectory_table(args.agent_steps)
    normalized = external_tool_trajectory_frame(table, frame_rate_hz=args.frame_rate_hz)
    pedpy_ready = normalized.rename(columns={"agent_id": "pedestrian_id"})[
        ["pedestrian_id", "frame", "time_s", "x", "y"]
    ]
    output = Path(args.out)
    output.parent.mkdir(parents=True, exist_ok=True)
    pedpy_ready.to_csv(output, index=False)
    print(f"wrote PedPy-ready trajectory CSV to {output}")
    try:
        import pedpy  # noqa: F401
    except ImportError:
        print("PedPy is not installed; install it separately to run density/speed analyses.")
    else:
        print("PedPy is installed; load the CSV into a PedPy TrajectoryData workflow.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
