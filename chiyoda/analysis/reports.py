from __future__ import annotations

from pathlib import Path
from typing import Iterable, Sequence

import matplotlib

matplotlib.use("Agg")

import matplotlib.pyplot as plt
from matplotlib.patches import Circle, Rectangle
import numpy as np
import pandas as pd
import seaborn as sns

from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.analysis.fundamental_diagram import weidmann_speed
from chiyoda.studies.models import ComparisonResult, StudyBundle
from chiyoda.studies.runner import _collect_run_tables


def export_figures(
    artifact: StudyBundle | ComparisonResult,
    output_dir: str | Path | None = None,
    profile: str = "paper",
    formats: Sequence[str] = ("png", "svg", "pdf"),
) -> list[Path]:
    _apply_style(profile)

    out = Path(output_dir or "figures")
    out.mkdir(parents=True, exist_ok=True)
    exported: list[Path] = []

    if isinstance(artifact, ComparisonResult):
        fig = _figure_comparison_result(artifact)
        exported.extend(_save_figure(fig, out, "06_scenario_comparison", formats))
        return exported

    figures = [
        ("01_layout_and_keyframes", _figure_layout_and_keyframes(artifact)),
        ("02_occupancy_and_slowdown", _figure_occupancy_and_slowdown(artifact)),
        ("03_bottleneck_dynamics", _figure_bottleneck_dynamics(artifact)),
        ("04_exit_and_flow", _figure_exit_and_flow(artifact)),
        ("05_distributions", _figure_distributions(artifact)),
        ("06_scenario_comparison", _figure_bundle_comparison(artifact)),
        ("07_entropy_heatmap", _figure_entropy_heatmap_series(artifact)),
        ("08_fundamental_diagram", _figure_fundamental_diagram(artifact)),
        ("09_belief_survival", _figure_belief_survival(artifact)),
        ("10_responder_timing", _figure_responder_timing(artifact)),
        ("11_info_flow_network", _figure_info_flow_network(artifact)),
    ]

    for name, fig in figures:
        exported.extend(_save_figure(fig, out, name, formats))
    return exported


def generate_report(simulation, output_path: str | Path) -> list[Path]:
    """Compatibility wrapper for older single-run workflows."""
    analytics = SimulationAnalytics()
    tables = _collect_run_tables(
        simulation=simulation,
        analytics=analytics,
        study_name="legacy_run",
        scenario_name="legacy_run",
        variant_name="baseline",
        seed=int(simulation.config.random_seed or 0),
        run_id="legacy_run",
    )
    bundle = StudyBundle(
        metadata={
            "study_name": "legacy_run",
            "scenario_name": "legacy_run",
            "acceleration_backend": simulation.acceleration.name,
            "requested_acceleration_backend": simulation.acceleration.requested_backend,
            "layout_text": "\n".join("".join(row) for row in simulation.layout.grid),
            "layout_width": simulation.layout.width,
            "layout_height": simulation.layout.height,
            "layout_origin_x": float(simulation.layout.origin[0]),
            "layout_origin_y": float(simulation.layout.origin[1]),
            "layout_cell_size": float(simulation.layout.cell_size),
            "bottleneck_zones": [
                {
                    "zone_id": zone.zone_id,
                    "cells": [list(cell) for cell in zone.cells],
                    "orientation": zone.orientation,
                    "centroid": list(zone.centroid),
                }
                for zone in simulation.bottleneck_zones
            ],
            "exit_labels": {
                f"{cell[0]},{cell[1]}": label
                for cell, label in simulation.exit_labels.items()
            },
            "runs": [{"run_id": "legacy_run", "variant_name": "baseline", "seed": simulation.config.random_seed}],
            "representative_run_id": "legacy_run",
        },
        summary=tables["summary"],
        steps=tables["steps"],
        cells=tables["cells"],
        agent_steps=tables["agent_steps"],
        agents=tables["agents"],
        bottlenecks=tables["bottlenecks"],
        dwell_samples=tables["dwell_samples"],
        exits=tables["exits"],
        hazards=tables["hazards"],
    )

    output_dir = Path(output_path)
    if output_dir.suffix:
        output_dir = output_dir.with_suffix("")
    output_dir.mkdir(parents=True, exist_ok=True)
    return export_figures(bundle, output_dir=output_dir)


def _figure_layout_and_keyframes(bundle: StudyBundle) -> plt.Figure:
    run_id = _representative_run_id(bundle)
    agent_steps = bundle.agent_steps[bundle.agent_steps["run_id"] == run_id].copy()
    available_steps = sorted(agent_steps["step"].unique().tolist()) if not agent_steps.empty else [0]
    keyframe_steps = [
        available_steps[0],
        available_steps[len(available_steps) // 2],
        available_steps[-1],
    ]

    fig, axes = plt.subplots(2, 2, figsize=(14, 11), constrained_layout=True)
    axes = axes.flatten()
    _draw_layout(axes[0], bundle)
    axes[0].set_title("Layout, exits, hazards, and bottleneck zones")

    for axis, step in zip(axes[1:], keyframe_steps):
        _draw_layout(axis, bundle, faint=True)
        frame = agent_steps[agent_steps["step"] == step]
        if not frame.empty:
            scatter = axis.scatter(
                frame["x"],
                frame["y"],
                c=frame["speed"],
                cmap="magma",
                s=22,
                alpha=0.9,
                edgecolors="none",
            )
            fig.colorbar(scatter, ax=axis, fraction=0.046, pad=0.04, label="Speed (m/s)")
        axis.set_title(f"Keyframe at step {step}")
    fig.suptitle("01 Layout and Keyframes", fontsize=16, fontweight="bold")
    return fig


def _figure_occupancy_and_slowdown(bundle: StudyBundle) -> plt.Figure:
    run_id = _representative_run_id(bundle)
    cells = bundle.cells[bundle.cells["run_id"] == run_id].copy()
    width = int(bundle.metadata.get("layout_width", 1))
    height = int(bundle.metadata.get("layout_height", 1))

    peak_occupancy = np.zeros((height, width), dtype=float)
    slowdown = np.zeros((height, width), dtype=float)
    slowdown_hits = np.zeros((height, width), dtype=float)

    if not cells.empty:
        for _, row in cells.iterrows():
            x, y = int(row["x"]), int(row["y"])
            peak_occupancy[y, x] = max(peak_occupancy[y, x], float(row["occupancy"]))
            slowdown[y, x] += max(0.0, 1.5 - float(row["speed"]))
            slowdown_hits[y, x] += 1.0
        slowdown = np.divide(
            slowdown,
            slowdown_hits,
            out=np.zeros_like(slowdown),
            where=slowdown_hits > 0,
        )

    fig, axes = plt.subplots(1, 2, figsize=(13, 5), constrained_layout=True)
    sns.heatmap(peak_occupancy, cmap="YlOrRd", ax=axes[0], cbar_kws={"label": "Peak occupancy"})
    axes[0].invert_yaxis()
    axes[0].set_title("Peak occupancy hotspot map")
    axes[0].set_xlabel("X")
    axes[0].set_ylabel("Y")

    sns.heatmap(slowdown, cmap="rocket_r", ax=axes[1], cbar_kws={"label": "Slowdown index"})
    axes[1].invert_yaxis()
    axes[1].set_title("Mean slowdown map")
    axes[1].set_xlabel("X")
    axes[1].set_ylabel("Y")
    fig.suptitle("02 Occupancy and Slowdown", fontsize=16, fontweight="bold")
    return fig


def _figure_bottleneck_dynamics(bundle: StudyBundle) -> plt.Figure:
    bottlenecks = bundle.bottlenecks.copy()
    dwell = bundle.dwell_samples.copy()

    fig, axes = plt.subplots(2, 2, figsize=(14, 10), constrained_layout=True)
    if bottlenecks.empty:
        for axis in axes.flatten():
            axis.text(0.5, 0.5, "No bottleneck telemetry available", ha="center", va="center")
            axis.axis("off")
        return fig

    mean_ts = (
        bottlenecks.groupby(["variant_name", "time_s"], as_index=False)[["queue_length", "outflow"]]
        .mean()
    )
    for variant_name, frame in mean_ts.groupby("variant_name"):
        axes[0, 0].plot(frame["time_s"], frame["queue_length"], label=variant_name)
        axes[0, 1].plot(frame["time_s"], frame["outflow"], label=variant_name)
    axes[0, 0].set_title("Queue length over time")
    axes[0, 0].set_xlabel("Time (s)")
    axes[0, 0].set_ylabel("Queue length")
    axes[0, 1].set_title("Throughput over time")
    axes[0, 1].set_xlabel("Time (s)")
    axes[0, 1].set_ylabel("Throughput")

    dwell_by_zone = (
        dwell.groupby(["variant_name", "zone_id"], as_index=False)["dwell_s"].mean()
        if not dwell.empty
        else pd.DataFrame(columns=["variant_name", "zone_id", "dwell_s"])
    )
    if not dwell_by_zone.empty:
        sns.barplot(data=dwell_by_zone, x="zone_id", y="dwell_s", hue="variant_name", ax=axes[1, 0])
        axes[1, 0].legend(loc="best")
    else:
        axes[1, 0].text(0.5, 0.5, "No dwell samples recorded", ha="center", va="center")
    axes[1, 0].set_title("Mean bottleneck dwell by zone")
    axes[1, 0].set_xlabel("Zone")
    axes[1, 0].set_ylabel("Seconds")

    peak_queue = (
        bottlenecks.groupby(["variant_name", "zone_id"], as_index=False)["queue_length"].max()
    )
    sns.barplot(data=peak_queue, x="zone_id", y="queue_length", hue="variant_name", ax=axes[1, 1])
    axes[1, 1].set_title("Peak queue by zone")
    axes[1, 1].set_xlabel("Zone")
    axes[1, 1].set_ylabel("Peak queue")
    axes[1, 1].legend(loc="best")

    for axis in axes[0]:
        axis.legend(loc="best")
    fig.suptitle("03 Bottleneck Dynamics", fontsize=16, fontweight="bold")
    return fig


def _figure_exit_and_flow(bundle: StudyBundle) -> plt.Figure:
    steps = bundle.steps.copy()
    exits = bundle.exits.copy()

    fig, axes = plt.subplots(1, 3, figsize=(15, 5), constrained_layout=True)
    evac = (
        steps.groupby(["variant_name", "time_s"], as_index=False)["evacuated_total"].mean()
        if not steps.empty
        else pd.DataFrame(columns=["variant_name", "time_s", "evacuated_total"])
    )
    for variant_name, frame in evac.groupby("variant_name"):
        axes[0].plot(frame["time_s"], frame["evacuated_total"], label=variant_name)
    axes[0].set_title("Evacuation curve")
    axes[0].set_xlabel("Time (s)")
    axes[0].set_ylabel("Evacuated")
    axes[0].legend(loc="best")

    if not exits.empty:
        final_exits = (
            exits.sort_values(["run_id", "exit_label", "time_s"])
            .groupby(["variant_name", "run_id", "exit_label"], as_index=False)
            .tail(1)
        )
        mean_exits = (
            final_exits.groupby(["variant_name", "exit_label"], as_index=False)["flow_cumulative"].mean()
        )
        sns.barplot(data=mean_exits, x="exit_label", y="flow_cumulative", hue="variant_name", ax=axes[1])
        axes[1].tick_params(axis="x", rotation=25)
        axes[1].legend(loc="best")

        imbalance_rows = []
        for (variant_name, run_id), frame in final_exits.groupby(["variant_name", "run_id"]):
            total = float(frame["flow_cumulative"].sum())
            share = 0.0 if total <= 0 else float(frame["flow_cumulative"].max()) / total
            imbalance_rows.append({"variant_name": variant_name, "run_id": run_id, "dominant_share": share})
        imbalance = pd.DataFrame(imbalance_rows)
        sns.barplot(data=imbalance, x="variant_name", y="dominant_share", ax=axes[2], color="#4c72b0")
    else:
        axes[1].text(0.5, 0.5, "No exit telemetry available", ha="center", va="center")
        axes[2].text(0.5, 0.5, "No route imbalance available", ha="center", va="center")
    axes[1].set_title("Final exit usage")
    axes[1].set_xlabel("Exit")
    axes[1].set_ylabel("Agents")
    axes[2].set_title("Route imbalance")
    axes[2].set_xlabel("Variant")
    axes[2].set_ylabel("Dominant exit share")
    fig.suptitle("04 Exit and Flow", fontsize=16, fontweight="bold")
    return fig


def _figure_distributions(bundle: StudyBundle) -> plt.Figure:
    fig, axes = plt.subplots(1, 3, figsize=(15, 5), constrained_layout=True)

    if not bundle.agents.empty:
        sns.histplot(
            data=bundle.agents,
            x="travel_time_s",
            hue="variant_name",
            element="step",
            stat="count",
            common_norm=False,
            ax=axes[0],
        )
        sns.histplot(
            data=bundle.agents,
            x="hazard_exposure",
            hue="variant_name",
            element="step",
            stat="count",
            common_norm=False,
            ax=axes[2],
        )
    else:
        axes[0].text(0.5, 0.5, "No agent outcomes available", ha="center", va="center")
        axes[2].text(0.5, 0.5, "No exposure outcomes available", ha="center", va="center")

    if not bundle.dwell_samples.empty:
        sns.histplot(
            data=bundle.dwell_samples,
            x="dwell_s",
            hue="variant_name",
            element="step",
            stat="count",
            common_norm=False,
            ax=axes[1],
        )
    else:
        axes[1].text(0.5, 0.5, "No dwell samples available", ha="center", va="center")

    axes[0].set_title("Travel time distribution")
    axes[0].set_xlabel("Seconds")
    axes[1].set_title("Bottleneck dwell distribution")
    axes[1].set_xlabel("Seconds")
    axes[2].set_title("Hazard exposure distribution")
    axes[2].set_xlabel("Exposure")
    fig.suptitle("05 Distributions", fontsize=16, fontweight="bold")
    return fig


def _figure_bundle_comparison(bundle: StudyBundle) -> plt.Figure:
    fig, axes = plt.subplots(1, 2, figsize=(13, 5), constrained_layout=True)
    variant_rows = bundle.summary[bundle.summary["record_type"] == "variant_aggregate"].copy()
    steps = bundle.steps.copy()

    if variant_rows["variant_name"].nunique() <= 1:
        axes[0].text(0.5, 0.5, "Only one variant available", ha="center", va="center")
        axes[1].text(0.5, 0.5, "Run a sweep or add variants for comparison", ha="center", va="center")
        for axis in axes:
            axis.axis("off")
        fig.suptitle("06 Scenario Comparison", fontsize=16, fontweight="bold")
        return fig

    baseline = variant_rows.iloc[0]
    comparison_rows = []
    for _, row in variant_rows.iterrows():
        if row["variant_name"] == baseline["variant_name"]:
            continue
        comparison_rows.append(
            {
                "variant_name": row["variant_name"],
                "travel_time_delta": float(row["mean_travel_time_s"]) - float(baseline["mean_travel_time_s"]),
                "queue_delta": float(row["peak_bottleneck_queue"]) - float(baseline["peak_bottleneck_queue"]),
                "exposure_delta": float(row["mean_hazard_exposure"]) - float(baseline["mean_hazard_exposure"]),
            }
        )
    comparison = pd.DataFrame(comparison_rows)
    comparison_plot = comparison.melt(id_vars="variant_name", var_name="metric", value_name="delta")
    sns.barplot(data=comparison_plot, x="metric", y="delta", hue="variant_name", ax=axes[0])
    axes[0].tick_params(axis="x", rotation=20)
    axes[0].set_title(f"Delta vs {baseline['variant_name']}")
    axes[0].set_xlabel("Metric")
    axes[0].set_ylabel("Delta")
    axes[0].legend(loc="best")

    evac = steps.groupby(["variant_name", "time_s"], as_index=False)["evacuated_total"].mean()
    for variant_name, frame in evac.groupby("variant_name"):
        axes[1].plot(frame["time_s"], frame["evacuated_total"], label=variant_name)
    axes[1].set_title("Evacuation curves by variant")
    axes[1].set_xlabel("Time (s)")
    axes[1].set_ylabel("Evacuated")
    axes[1].legend(loc="best")
    fig.suptitle("06 Scenario Comparison", fontsize=16, fontweight="bold")
    return fig


def _figure_comparison_result(result: ComparisonResult) -> plt.Figure:
    fig, axes = plt.subplots(1, 2, figsize=(13, 5), constrained_layout=True)

    if not result.metrics.empty:
        top_metrics = result.metrics.sort_values("pct_change", key=np.abs, ascending=False).head(8)
        sns.barplot(data=top_metrics, x="metric", y="pct_change", ax=axes[0], color="#55a868")
        axes[0].tick_params(axis="x", rotation=25)
        axes[0].set_title("Percentage change by metric")
        axes[0].set_xlabel("Metric")
        axes[0].set_ylabel("Percent change")
    else:
        axes[0].text(0.5, 0.5, "No comparison metrics available", ha="center", va="center")

    if not result.timeseries.empty:
        for series_name, frame in result.timeseries.groupby("series"):
            axes[1].plot(frame["time_s"], frame["evacuated_total"], label=f"{series_name} evacuated")
            axes[1].plot(frame["time_s"], frame["mean_speed"], linestyle="--", label=f"{series_name} speed")
        axes[1].legend(loc="best")
    else:
        axes[1].text(0.5, 0.5, "No comparison timeseries available", ha="center", va="center")
    axes[1].set_title("Study comparison timeline")
    axes[1].set_xlabel("Time (s)")
    axes[1].set_ylabel("Value")
    fig.suptitle("06 Scenario Comparison", fontsize=16, fontweight="bold")
    return fig


def _draw_layout(axis, bundle: StudyBundle, faint: bool = False) -> None:
    layout_text = str(bundle.metadata.get("layout_text", "")).splitlines()
    if not layout_text:
        axis.axis("off")
        return

    height = len(layout_text)
    width = max(len(line) for line in layout_text)
    alpha = 0.15 if faint else 0.9
    for y, row in enumerate(layout_text):
        for x, cell in enumerate(row):
            if cell == "X":
                axis.add_patch(Rectangle((x, y), 1, 1, facecolor="#1f2933", edgecolor="none", alpha=alpha))

    for key, label in dict(bundle.metadata.get("exit_labels", {})).items():
        x, y = [int(part) for part in key.split(",")]
        axis.scatter(x + 0.5, y + 0.5, marker="*", s=120, c="#2a9d8f")
        axis.text(x + 0.65, y + 0.55, label.split()[1], fontsize=8, color="#2a9d8f")

    for zone in bundle.metadata.get("bottleneck_zones", []):
        centroid = zone["centroid"]
        axis.add_patch(
            Rectangle(
                (centroid[0] - 0.5, centroid[1] - 0.5),
                1.0,
                1.0,
                fill=False,
                edgecolor="#e76f51",
                linewidth=1.8,
            )
        )

    representative_run = _representative_run_id(bundle)
    hazard_rows = bundle.hazards[bundle.hazards["run_id"] == representative_run]
    if not hazard_rows.empty:
        latest_hazards = hazard_rows.sort_values("time_s").groupby("hazard_id", as_index=False).tail(1)
        for _, hazard in latest_hazards.iterrows():
            axis.add_patch(
                Circle(
                    (float(hazard["x"]) + 0.5, float(hazard["y"]) + 0.5),
                    radius=float(hazard["radius"]),
                    fill=False,
                    edgecolor="#d62828",
                    linewidth=1.3,
                    alpha=0.65,
                )
            )

    axis.set_xlim(0, width)
    axis.set_ylim(height, 0)
    axis.set_aspect("equal")
    axis.set_xlabel("X")
    axis.set_ylabel("Y")


def _representative_run_id(bundle: StudyBundle) -> str:
    run_id = bundle.metadata.get("representative_run_id")
    if run_id:
        return str(run_id)
    if not bundle.steps.empty:
        return str(bundle.steps["run_id"].iloc[0])
    return "run_1"


def _save_figure(fig: plt.Figure, output_dir: Path, name: str, formats: Sequence[str]) -> list[Path]:
    exported: list[Path] = []
    for output_format in formats:
        target = output_dir / f"{name}.{output_format}"
        fig.savefig(target, dpi=200, bbox_inches="tight")
        exported.append(target)
    plt.close(fig)
    return exported


def _apply_style(profile: str) -> None:
    sns.set_theme(style="whitegrid", palette="deep")
    if profile == "paper":
        plt.rcParams.update(
            {
                "figure.facecolor": "white",
                "axes.facecolor": "white",
                "axes.edgecolor": "#c5ced8",
                "axes.titlesize": 12,
                "axes.labelsize": 10,
                "font.size": 10,
                "legend.frameon": True,
                "legend.facecolor": "white",
            }
        )

def _figure_entropy_heatmap_series(bundle: StudyBundle) -> plt.Figure:
    run_id = _representative_run_id(bundle)
    agent_steps = bundle.agent_steps[bundle.agent_steps["run_id"] == run_id].copy()
    available_steps = sorted(agent_steps["step"].unique().tolist()) if not agent_steps.empty else [0]
    
    if len(available_steps) < 3:
        keyframe_steps = available_steps * 3
    else:
        keyframe_steps = [
            available_steps[0],
            available_steps[len(available_steps) // 2],
            available_steps[-1],
        ]

    fig, axes = plt.subplots(1, 3, figsize=(18, 5), constrained_layout=True)
    
    for axis, step in zip(axes, keyframe_steps):
        _draw_layout(axis, bundle, faint=True)
        frame = agent_steps[agent_steps["step"] == step]
        if not frame.empty and "entropy" in frame.columns:
            scatter = axis.scatter(
                frame["x"],
                frame["y"],
                c=frame["entropy"],
                cmap="viridis",
                s=25,
                alpha=0.8,
                edgecolors="none",
                vmin=0.0,
                vmax=1.0,
            )
            if step == keyframe_steps[-1]:
                fig.colorbar(scatter, ax=axis, fraction=0.046, pad=0.04, label="Information Entropy (nats)")
        axis.set_title(f"Entropy at step {step}")
        
    fig.suptitle("07 Entropy Heatmap Time-Series", fontsize=16, fontweight="bold")
    return fig


def _figure_fundamental_diagram(bundle: StudyBundle) -> plt.Figure:
    fig, axis = plt.subplots(1, 1, figsize=(8, 6), constrained_layout=True)
    
    if not hasattr(bundle, 'measurements') or bundle.measurements.empty:
        axis.text(0.5, 0.5, "No MeasurementLine telemetry available", ha="center", va="center")
        axis.axis("off")
        return fig

    measurements = bundle.measurements.copy()
    run_id = _representative_run_id(bundle)
    m_run = measurements[measurements["run_id"] == run_id]
    
    if m_run.empty:
        axis.text(0.5, 0.5, "No MeasurementLine telemetry for representative run", ha="center", va="center")
        axis.axis("off")
        return fig
        
    m_run = m_run[(m_run["density"] > 0.05) & (m_run["n_in_region"] >= 2)]
    
    axis.scatter(
        m_run["density"], 
        m_run["speed"], 
        alpha=0.5, 
        c="steelblue",
        label="Simulated (Measurement Line)"
    )
    
    if not m_run.empty:
        densities = np.linspace(0.01, min(6.0, m_run["density"].max() * 1.5), 100)
        speeds = weidmann_speed(densities)
        axis.plot(densities, speeds, "r--", linewidth=2, label="Weidmann (1993) Theoretical")
        
    axis.set_xlabel("Density (ped/m²)")
    axis.set_ylabel("Speed (m/s)")
    axis.set_title("08 Fundamental Diagram Overlay")
    axis.legend()
    axis.grid(True, linestyle=":", alpha=0.6)
    
    return fig


def _figure_belief_survival(bundle: StudyBundle) -> plt.Figure:
    fig, axis = plt.subplots(1, 1, figsize=(8, 6), constrained_layout=True)
    
    agents = bundle.agents.copy()
    agent_steps = bundle.agent_steps.copy()
    
    if agents.empty or agent_steps.empty or "belief_accuracy" not in agent_steps.columns:
        axis.text(0.5, 0.5, "No belief accuracy telemetry available", ha="center", va="center")
        axis.axis("off")
        return fig
        
    run_id = _representative_run_id(bundle)
    a_run = agents[agents["run_id"] == run_id]
    s_run = agent_steps[agent_steps["run_id"] == run_id]
    
    mean_accuracy = s_run.groupby("agent_id")["belief_accuracy"].mean().reset_index()
    merged = a_run.merge(mean_accuracy, on="agent_id", how="left")
    
    evacuated = merged[merged["evacuated"] == True]
    incapacitated = merged[merged["evacuated"] == False]
    
    axis.hist(
        evacuated["belief_accuracy"].dropna(), 
        bins=20, 
        alpha=0.5, 
        label="Evacuated (Survived)",
        color="green",
        density=True
    )
    if not incapacitated.empty:
        axis.hist(
            incapacitated["belief_accuracy"].dropna(), 
            bins=20, 
            alpha=0.5, 
            label="Incapacitated",
            color="red",
            density=True
        )
        
    axis.set_xlabel("Mean Belief Accuracy")
    axis.set_ylabel("Density")
    axis.set_title("09 Belief Accuracy vs. Survival")
    axis.legend()
    
    return fig


def _figure_responder_timing(bundle: StudyBundle) -> plt.Figure:
    fig, axis = plt.subplots(1, 1, figsize=(8, 6), constrained_layout=True)
    
    steps = bundle.steps.copy()
    if steps.empty or "global_entropy" not in steps.columns:
        axis.text(0.5, 0.5, "No global entropy telemetry available", ha="center", va="center")
        axis.axis("off")
        return fig
        
    run_id = _representative_run_id(bundle)
    st = steps[steps["run_id"] == run_id].sort_values("time_s")
    
    axis.plot(st["time_s"], st["global_entropy"], linewidth=2, color="purple", label="Global Entropy")
    
    agent_steps = bundle.agent_steps.copy()
    if not agent_steps.empty:
        # Find responder insertion time (when agent with cohort_name 'responders' first appears)
        s_run = agent_steps[agent_steps["run_id"] == run_id]
        if "cohort_name" in s_run.columns:
            responders = s_run[s_run["cohort_name"].str.contains("responder", case=False, na=False)]
            if not responders.empty:
                first_t = responders["time_s"].min()
                axis.axvline(first_t, color="red", linestyle="--", label=f"Responder Insertion (t={first_t}s)")
                
    axis.set_xlabel("Time (s)")
    axis.set_ylabel("Global Information Entropy (nats)")
    axis.set_title("10 Responder Timing & Entropy Cascade")
    axis.legend()
    axis.grid(True, linestyle=":", alpha=0.6)
    
    return fig


def _figure_info_flow_network(bundle: StudyBundle) -> plt.Figure:
    fig, axis = plt.subplots(1, 1, figsize=(10, 8), constrained_layout=True)
    
    if not hasattr(bundle, 'gossip') or bundle.gossip.empty:
        axis.text(0.5, 0.5, "No Gossip telemetry available", ha="center", va="center")
        axis.axis("off")
        return fig
        
    gossip = bundle.gossip.copy()
    run_id = _representative_run_id(bundle)
    g_run = gossip[gossip["run_id"] == run_id]
    
    _draw_layout(axis, bundle, faint=True)
    
    agent_steps = bundle.agent_steps.copy()
    s_run = agent_steps[agent_steps["run_id"] == run_id]
    
    if not g_run.empty and not s_run.empty:
        # Plot only a sample to avoid extreme clutter (e.g., first 500 events)
        sample = g_run.head(500)
        
        for _, row in sample.iterrows():
            t = row["time_s"]
            sender_id = row["sender_id"]
            receiver_id = row["receiver_id"]
            
            s_pos = s_run[(s_run["agent_id"] == sender_id) & (np.isclose(s_run["time_s"], t, atol=0.5))]
            r_pos = s_run[(s_run["agent_id"] == receiver_id) & (np.isclose(s_run["time_s"], t, atol=0.5))]
            
            if not s_pos.empty and not r_pos.empty:
                sx, sy = s_pos.iloc[0]["x"], s_pos.iloc[0]["y"]
                rx, ry = r_pos.iloc[0]["x"], r_pos.iloc[0]["y"]
                
                axis.plot([sx, rx], [sy, ry], color="orange", alpha=0.3, linewidth=1)
                
    axis.set_title("11 Information Flow Network (Gossip Transfers)")
    
    return fig
