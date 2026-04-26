#!/usr/bin/env python3
"""Run a Chiyoda study with per-run progress output.

This is a thin wrapper around the existing StudyBundle pipeline. It exists for
long empirical runs where the Click CLI's all-at-end export gives no visibility
into variant/seed progress.
"""
from __future__ import annotations

import argparse
import json
import shutil
import time
from datetime import datetime, timezone
from pathlib import Path

import pandas as pd

from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.studies.models import StudyBundle
from chiyoda.studies.runner import (
    _aggregate_summary,
    _collect_run_tables,
    _concat,
    _materialize_variants,
    _prepare_scenario,
    _resolve_seeds,
    load_study_config,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("study_file")
    parser.add_argument("-o", "--out", required=True)
    parser.add_argument("--seed-count", type=int, default=None)
    parser.add_argument("--seed-start", type=int, default=42)
    parser.add_argument("--no-figures", action="store_true")
    parser.add_argument(
        "--checkpoint-dir",
        default=None,
        help="Optional directory for per-run checkpoints. Defaults to <out>.checkpoints when --resume is used.",
    )
    parser.add_argument(
        "--resume",
        action="store_true",
        help="Reuse completed per-run checkpoints and skip those simulations.",
    )
    return parser.parse_args()


def seed_sequence(count: int, start: int) -> list[int]:
    return [start + index for index in range(count)]


def frame_names() -> list[str]:
    return [
        "summary",
        "steps",
        "cells",
        "agent_steps",
        "agents",
        "bottlenecks",
        "dwell_samples",
        "exits",
        "hazards",
        "measurements",
        "gossip",
        "interventions",
    ]


def checkpoint_root(args: argparse.Namespace) -> Path | None:
    if args.checkpoint_dir:
        return Path(args.checkpoint_dir)
    if args.resume:
        return Path(f"{args.out}.checkpoints")
    return None


def run_checkpoint_dir(root: Path, run_index: int) -> Path:
    return root / "runs" / f"{run_index:05d}"


def run_checkpoint_manifest(root: Path, run_index: int) -> Path:
    return run_checkpoint_dir(root, run_index) / "manifest.json"


def checkpoint_complete(root: Path | None, run_index: int) -> bool:
    return root is not None and run_checkpoint_manifest(root, run_index).exists()


def write_run_checkpoint(
    root: Path,
    run_index: int,
    tables: dict[str, pd.DataFrame],
    manifest: dict[str, object],
) -> None:
    final_dir = run_checkpoint_dir(root, run_index)
    if final_dir.exists() and (final_dir / "manifest.json").exists():
        return
    if final_dir.exists():
        shutil.rmtree(final_dir)

    tmp_dir = root / "runs" / f".{run_index:05d}.tmp"
    if tmp_dir.exists():
        shutil.rmtree(tmp_dir)
    tmp_dir.mkdir(parents=True, exist_ok=True)

    for table_name, frame in tables.items():
        frame.to_parquet(tmp_dir / f"{table_name}.parquet", index=False)
    (tmp_dir / "manifest.json").write_text(json.dumps(manifest, indent=2, default=str) + "\n")
    tmp_dir.replace(final_dir)


def load_run_checkpoint(root: Path, run_index: int) -> tuple[dict[str, pd.DataFrame], dict[str, object]]:
    run_dir = run_checkpoint_dir(root, run_index)
    manifest = json.loads((run_dir / "manifest.json").read_text())
    tables = {
        table_name: pd.read_parquet(run_dir / f"{table_name}.parquet")
        if (run_dir / f"{table_name}.parquet").exists()
        else pd.DataFrame()
        for table_name in frame_names()
    }
    return tables, manifest


def main() -> int:
    args = parse_args()
    config = load_study_config(args.study_file)
    if args.seed_count is not None:
        config = config.model_copy(update={"seeds": seed_sequence(args.seed_count, args.seed_start)})

    manager = ScenarioManager()
    analytics = SimulationAnalytics()
    variants = _materialize_variants(config)
    names = frame_names()
    frames = {name: [] for name in names}
    runs_manifest = []
    first_layout_text = None
    first_bottlenecks = []
    first_exit_labels = {}
    scenario_name = None
    ckpt_root = checkpoint_root(args)
    if ckpt_root is not None:
        ckpt_root.mkdir(parents=True, exist_ok=True)
        (ckpt_root / "study_file.txt").write_text(str(Path(args.study_file).resolve()) + "\n")

    total = sum(len(_resolve_seeds(config, variant)) for variant in variants)
    run_index = 0
    start_time = time.perf_counter()

    for variant in variants:
        for seed in _resolve_seeds(config, variant):
            run_index += 1
            run_start = time.perf_counter()
            run_id = f"{variant.name}__seed_{seed}__run_{run_index}"
            print(f"[{run_index}/{total}] {variant.name} seed={seed}", flush=True)

            if args.resume and checkpoint_complete(ckpt_root, run_index):
                tables, manifest = load_run_checkpoint(ckpt_root, run_index)
                for table_name, frame in tables.items():
                    frames[table_name].append(frame)
                runs_manifest.append(dict(manifest["run"]))
                context = manifest.get("context", {})
                if first_layout_text is None:
                    first_layout_text = str(context.get("layout_text", ""))
                    first_bottlenecks = list(context.get("bottleneck_zones", []))
                    first_exit_labels = dict(context.get("exit_labels", {}))
                    scenario_name = str(context.get("scenario_name", Path(config.scenario_file).stem))
                elapsed = time.perf_counter() - run_start
                print(
                    f"  checkpoint hit in {elapsed:.1f}s; interventions={manifest.get('interventions', 0)} "
                    f"evacuated={manifest.get('agents_evacuated', 0)}",
                    flush=True,
                )
                continue

            prepared = _prepare_scenario(manager, config.scenario_file, variant, seed)
            simulation = manager.build_simulation(prepared)
            simulation.run()

            scenario_name = prepared.get("name", Path(config.scenario_file).stem)
            if first_layout_text is None:
                first_layout_text = manager.serialize_layout(simulation.layout)
                first_bottlenecks = [
                    {
                        "zone_id": zone.zone_id,
                        "cells": [list(cell) for cell in zone.cells],
                        "orientation": zone.orientation,
                        "centroid": list(zone.centroid),
                    }
                    for zone in simulation.bottleneck_zones
                ]
                first_exit_labels = {
                    f"{cell[0]},{cell[1]}": label
                    for cell, label in simulation.exit_labels.items()
                }

            tables = _collect_run_tables(
                simulation=simulation,
                analytics=analytics,
                study_name=config.name,
                scenario_name=scenario_name,
                variant_name=variant.name,
                seed=seed,
                run_id=run_id,
            )
            for table_name, frame in tables.items():
                frames[table_name].append(frame)

            run_manifest = {
                "run_id": run_id,
                "variant_name": variant.name,
                "seed": seed,
                "acceleration_backend": simulation.acceleration.name,
                "requested_acceleration_backend": simulation.acceleration.requested_backend,
                "agents_total": len(simulation.agents),
                "agents_evacuated": len(simulation.completed_agents),
            }
            runs_manifest.append(run_manifest)
            if ckpt_root is not None:
                write_run_checkpoint(
                    ckpt_root,
                    run_index,
                    tables,
                    {
                        "run": run_manifest,
                        "context": {
                            "scenario_name": scenario_name or Path(config.scenario_file).stem,
                            "layout_text": first_layout_text or "",
                            "bottleneck_zones": first_bottlenecks,
                            "exit_labels": first_exit_labels,
                        },
                        "interventions": len(simulation.intervention_events),
                        "agents_evacuated": len(simulation.completed_agents),
                    },
                )
            elapsed = time.perf_counter() - run_start
            print(
                f"  done in {elapsed:.1f}s; interventions={len(simulation.intervention_events)} "
                f"evacuated={len(simulation.completed_agents)}",
                flush=True,
            )

    summary = _concat(frames["summary"])
    summary = pd.concat([summary, _aggregate_summary(summary)], ignore_index=True)
    metadata = {
        "study_name": config.name,
        "description": config.description,
        "scenario_file": config.scenario_file,
        "scenario_name": scenario_name or Path(config.scenario_file).stem,
        "created_at_utc": datetime.now(timezone.utc).isoformat(),
        "export_config": config.export.model_dump(),
        "acceleration_backend": runs_manifest[0]["acceleration_backend"] if runs_manifest else "python",
        "requested_acceleration_backend": (
            runs_manifest[0]["requested_acceleration_backend"] if runs_manifest else "auto"
        ),
        "layout_text": first_layout_text or "",
        "layout_width": summary["layout_width"].dropna().iloc[0] if not summary.empty else 0,
        "layout_height": summary["layout_height"].dropna().iloc[0] if not summary.empty else 0,
        "layout_origin_x": summary["layout_origin_x"].dropna().iloc[0] if not summary.empty else 0.0,
        "layout_origin_y": summary["layout_origin_y"].dropna().iloc[0] if not summary.empty else 0.0,
        "layout_cell_size": summary["layout_cell_size"].dropna().iloc[0] if not summary.empty else 1.0,
        "bottleneck_zones": first_bottlenecks,
        "exit_labels": first_exit_labels,
        "variants": [variant.model_dump() for variant in variants],
        "runs": runs_manifest,
        "representative_run_id": runs_manifest[0]["run_id"] if runs_manifest else None,
    }
    bundle = StudyBundle(
        metadata=metadata,
        summary=summary,
        steps=_concat(frames["steps"]),
        cells=_concat(frames["cells"]),
        agent_steps=_concat(frames["agent_steps"]),
        agents=_concat(frames["agents"]),
        bottlenecks=_concat(frames["bottlenecks"]),
        dwell_samples=_concat(frames["dwell_samples"]),
        exits=_concat(frames["exits"]),
        hazards=_concat(frames["hazards"]),
        measurements=_concat(frames["measurements"]),
        gossip=_concat(frames["gossip"]),
        interventions=_concat(frames["interventions"]),
    )

    out = Path(args.out)
    bundle.export(out, table_formats=tuple(config.export.table_formats))
    if not args.no_figures and config.export.include_figures:
        print("exporting figures...", flush=True)
        from chiyoda.analysis.reports import export_figures

        export_figures(
            bundle,
            output_dir=out / "figures",
            profile=config.export.profile,
            formats=tuple(config.export.formats),
        )
    print(f"exported {out} in {time.perf_counter() - start_time:.1f}s", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
