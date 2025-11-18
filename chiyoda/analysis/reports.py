from __future__ import annotations

from typing import Dict
import plotly.graph_objects as go
import numpy as np


def _evacuation_timeline(simulation) -> go.Figure:
    steps = list(range(simulation.current_step + 1))
    evacuated = []
    count = 0
    evac_set = set(simulation.evacuated_at_step)
    for s in steps:
        count += list(simulation.evacuated_at_step).count(s)
        evacuated.append(count)
    fig = go.Figure()
    fig.add_trace(go.Scatter(x=steps, y=evacuated, mode="lines", name="Evacuated"))
    fig.update_layout(title="Evacuation Progress", xaxis_title="Step", yaxis_title="# Evacuated")
    return fig


def _final_density(simulation) -> go.Figure:
    h, w = simulation.layout.height, simulation.layout.width
    density = np.zeros((h, w))
    for a in simulation.agents:
        if not a.has_evacuated:
            x, y = int(np.clip(round(a.pos[0]), 0, w - 1)), int(np.clip(round(a.pos[1]), 0, h - 1))
            density[y, x] += 1
    fig = go.Figure(data=go.Heatmap(z=density, colorscale="YlOrRd"))
    fig.update_layout(title="Final Population Density")
    return fig


def generate_report(simulation, output_path: str) -> None:
    from .metrics import SimulationAnalytics

    metrics: Dict[str, float] = SimulationAnalytics().calculate_performance_metrics(simulation)
    figs = {
        "timeline": _evacuation_timeline(simulation),
        "density": _final_density(simulation),
    }

    html = [
        "<html><head><title>Chiyoda Simulation Report</title></head><body>",
        "<h1>Chiyoda Simulation Report</h1>",
        "<h2>Key Metrics</h2>",
        "<ul>",
    ]
    for k, v in metrics.items():
        html.append(f"<li><b>{k}</b>: {v}</li>")
    html.append("</ul>")
    html.append("<h2>Visualizations</h2>")
    for name, fig in figs.items():
        html.append(f"<h3>{name.title()}</h3>")
        html.append(fig.to_html(full_html=False, include_plotlyjs='cdn'))
    html.append("</body></html>")

    with open(output_path, "w") as f:
        f.write("\n".join(html))
