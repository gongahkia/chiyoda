from __future__ import annotations

from typing import Optional

import numpy as np
import plotly.graph_objects as go
from plotly.subplots import make_subplots


def _wall_coordinates(layout) -> tuple[list[float], list[float]]:
    xs: list[float] = []
    ys: list[float] = []
    for y in range(layout.height):
        for x in range(layout.width):
            if not layout.is_walkable((x, y)):
                xs.append(x + 0.5)
                ys.append(y + 0.5)
    return xs, ys


def _hazard_sizes(hazards) -> list[float]:
    sizes: list[float] = []
    for hazard in hazards:
        sizes.append(max(10.0, (float(hazard["radius"]) + 0.5) * 18.0))
    return sizes


def _trail_arrays(agents) -> tuple[list[float], list[float]]:
    xs: list[float] = []
    ys: list[float] = []
    for agent in agents:
        for x, y in agent["trail"]:
            xs.append(x)
            ys.append(y)
        xs.append(None)
        ys.append(None)
    return xs, ys


def _agent_customdata(agents) -> list[list[object]]:
    customdata: list[list[object]] = []
    for agent in agents:
        exit_label = (
            f"{agent['target_exit'][0]},{agent['target_exit'][1]}"
            if agent["target_exit"] is not None
            else "n/a"
        )
        customdata.append(
            [
                agent["id"],
                agent["state"],
                round(float(agent["speed"]), 3),
                round(float(agent["local_density"]), 3),
                exit_label,
            ]
        )
    return customdata


class InteractiveVisualizer:
    """Live Plotly view that surfaces congestion, speed, and evacuation context."""

    def __init__(self) -> None:
        self.fig: Optional[go.Figure] = None

    def init(self, simulation) -> None:
        state = simulation.live_state()
        wall_x, wall_y = _wall_coordinates(simulation.layout)
        trail_x, trail_y = _trail_arrays(state["agents"])
        history = simulation.step_history
        steps = [step.step for step in history]

        self.fig = make_subplots(
            rows=2,
            cols=2,
            specs=[
                [{"type": "scatter"}, {"type": "heatmap"}],
                [{"type": "heatmap"}, {"type": "xy"}],
            ],
            subplot_titles=(
                "Replay",
                "Occupancy Heatmap",
                "Speed Heatmap",
                "Run Timeline",
            ),
            horizontal_spacing=0.08,
            vertical_spacing=0.12,
        )

        self.fig.add_trace(
            go.Scatter(
                x=wall_x,
                y=wall_y,
                mode="markers",
                marker=dict(size=10, color="#20242a", symbol="square"),
                name="Walls",
                hoverinfo="skip",
                showlegend=False,
            ),
            row=1,
            col=1,
        )
        self.fig.add_trace(
            go.Scatter(
                x=[exit_[0] + 0.5 for exit_ in state["exits"]],
                y=[exit_[1] + 0.5 for exit_ in state["exits"]],
                mode="markers",
                marker=dict(size=12, color="#1b9e77", symbol="star"),
                name="Exits",
            ),
            row=1,
            col=1,
        )
        self.fig.add_trace(
            go.Scatter(
                x=[zone["centroid"][0] for zone in state["bottlenecks"]],
                y=[zone["centroid"][1] for zone in state["bottlenecks"]],
                mode="markers",
                marker=dict(size=18, color="rgba(0,0,0,0)", line=dict(color="#e66101", width=2), symbol="square-open"),
                name="Bottlenecks",
                hovertemplate="Bottleneck %{text}<extra></extra>",
                text=[zone["id"] for zone in state["bottlenecks"]],
            ),
            row=1,
            col=1,
        )
        self.fig.add_trace(
            go.Scatter(
                x=[hazard["pos"][0] + 0.5 for hazard in state["hazards"]],
                y=[hazard["pos"][1] + 0.5 for hazard in state["hazards"]],
                mode="markers",
                marker=dict(
                    size=_hazard_sizes(state["hazards"]),
                    color="rgba(214,39,40,0.18)",
                    line=dict(color="#d62728", width=2),
                ),
                name="Hazards",
                hovertemplate="Hazard radius=%{customdata[0]:.2f}<extra></extra>",
                customdata=[[hazard["radius"]] for hazard in state["hazards"]],
            ),
            row=1,
            col=1,
        )
        self.fig.add_trace(
            go.Scatter(
                x=trail_x,
                y=trail_y,
                mode="lines",
                line=dict(color="rgba(31,119,180,0.24)", width=1),
                name="Trails",
                hoverinfo="skip",
                showlegend=False,
            ),
            row=1,
            col=1,
        )
        self.fig.add_trace(
            go.Scatter(
                x=state["positions"][:, 0] if len(state["positions"]) else [],
                y=state["positions"][:, 1] if len(state["positions"]) else [],
                mode="markers",
                marker=dict(
                    size=8,
                    color=state["speeds"] if len(state["speeds"]) else [],
                    colorscale="Turbo",
                    colorbar=dict(title="Speed"),
                    cmin=0,
                    cmax=max(1.5, float(np.max(state["speeds"])) if len(state["speeds"]) else 1.5),
                ),
                name="Agents",
                customdata=_agent_customdata(state["agents"]),
                hovertemplate=(
                    "Agent %{customdata[0]}<br>"
                    "State=%{customdata[1]}<br>"
                    "Speed=%{customdata[2]} m/s<br>"
                    "Density=%{customdata[3]} p/m²<br>"
                    "Target exit=%{customdata[4]}<extra></extra>"
                ),
            ),
            row=1,
            col=1,
        )
        self.fig.add_trace(
            go.Heatmap(
                z=state["occupancy_grid"],
                colorscale="YlOrRd",
                name="Occupancy",
                showscale=False,
            ),
            row=1,
            col=2,
        )
        self.fig.add_trace(
            go.Heatmap(
                z=state["speed_grid"],
                colorscale="Blues",
                name="Speed",
                showscale=False,
            ),
            row=2,
            col=1,
        )
        self.fig.add_trace(
            go.Scatter(
                x=steps,
                y=[step.evacuated_total for step in history],
                mode="lines",
                line=dict(color="#2ca02c", width=3),
                name="Evacuated",
            ),
            row=2,
            col=2,
        )
        self.fig.add_trace(
            go.Scatter(
                x=steps,
                y=[step.mean_density for step in history],
                mode="lines",
                line=dict(color="#d95f02", width=2),
                name="Mean density",
            ),
            row=2,
            col=2,
        )
        self.fig.add_trace(
            go.Scatter(
                x=steps,
                y=[step.mean_speed for step in history],
                mode="lines",
                line=dict(color="#1f78b4", width=2, dash="dot"),
                name="Mean speed",
            ),
            row=2,
            col=2,
        )

        self.fig.update_xaxes(range=[0, simulation.layout.width], row=1, col=1, title_text="X")
        self.fig.update_yaxes(
            range=[0, simulation.layout.height],
            row=1,
            col=1,
            title_text="Y",
            scaleanchor="x",
            scaleratio=1,
        )
        self.fig.update_xaxes(title_text="X", row=1, col=2)
        self.fig.update_yaxes(title_text="Y", row=1, col=2, scaleanchor="x2", scaleratio=1)
        self.fig.update_xaxes(title_text="X", row=2, col=1)
        self.fig.update_yaxes(title_text="Y", row=2, col=1, scaleanchor="x3", scaleratio=1)
        self.fig.update_xaxes(title_text="Step", row=2, col=2)
        self.fig.update_yaxes(title_text="Evacuated", row=2, col=2)
        self.fig.update_layout(
            title="Chiyoda Crowd Study View",
            height=900,
            template="plotly_white",
            legend=dict(orientation="h", yanchor="bottom", y=1.02, xanchor="right", x=1),
        )

    def on_step(self, simulation) -> None:
        if self.fig is None:
            self.init(simulation)

        state = simulation.live_state()
        history = simulation.step_history
        trail_x, trail_y = _trail_arrays(state["agents"])

        self.fig.data[3].x = [hazard["pos"][0] + 0.5 for hazard in state["hazards"]]
        self.fig.data[3].y = [hazard["pos"][1] + 0.5 for hazard in state["hazards"]]
        self.fig.data[3].marker.size = _hazard_sizes(state["hazards"])
        self.fig.data[3].customdata = [[hazard["radius"]] for hazard in state["hazards"]]
        self.fig.data[4].x = trail_x
        self.fig.data[4].y = trail_y
        self.fig.data[5].x = state["positions"][:, 0] if len(state["positions"]) else []
        self.fig.data[5].y = state["positions"][:, 1] if len(state["positions"]) else []
        self.fig.data[5].marker.color = state["speeds"] if len(state["speeds"]) else []
        self.fig.data[5].customdata = _agent_customdata(state["agents"])
        self.fig.data[6].z = state["occupancy_grid"]
        self.fig.data[7].z = state["speed_grid"]
        steps = [step.step for step in history]
        self.fig.data[8].x = steps
        self.fig.data[8].y = [step.evacuated_total for step in history]
        self.fig.data[9].x = steps
        self.fig.data[9].y = [step.mean_density for step in history]
        self.fig.data[10].x = steps
        self.fig.data[10].y = [step.mean_speed for step in history]

    def show(self):
        if self.fig is not None:
            self.fig.show()

    def export_html(self, path: str) -> None:
        if self.fig is not None:
            self.fig.write_html(path)
