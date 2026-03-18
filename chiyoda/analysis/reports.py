from __future__ import annotations

from typing import Dict, Iterable, List, Optional

import numpy as np
import plotly.graph_objects as go
from plotly.subplots import make_subplots

from chiyoda.visualization.plotly_viz import (
    _agent_customdata,
    _hazard_sizes,
    _trail_arrays,
    _wall_coordinates,
)


def _step_agents(step) -> List[dict]:
    return [
        {
            "id": agent.agent_id,
            "position": agent.position,
            "cell": agent.cell,
            "state": agent.state,
            "speed": agent.speed,
            "local_density": agent.local_density,
            "target_exit": agent.target_exit,
            "trail": list(agent.trail),
        }
        for agent in step.agents
    ]


def _time_axis(history) -> List[float]:
    return [step.time_s for step in history]


def _bottleneck_targets(simulation) -> List[Optional[object]]:
    return simulation.bottleneck_zones or [None]


def _bottleneck_label(zone) -> str:
    return zone.zone_id if zone is not None else "No bottleneck"


def _bottleneck_series(history, zone, attribute: str) -> List[float]:
    if zone is None:
        return [0.0 for _ in history]
    return [getattr(step.bottlenecks[zone.zone_id], attribute) for step in history]


def _exit_series(history, label: str) -> List[int]:
    return [step.exit_flow_cumulative.get(label, 0) for step in history]


def _summary_cards(metrics: Dict[str, object]) -> str:
    ordered = [
        ("Total time (s)", f"{metrics['total_time_s']:.2f}"),
        ("Evacuated", str(metrics["agents_evacuated"])),
        ("Remaining", str(metrics["agents_remaining"])),
        ("Mean travel (s)", f"{metrics['mean_travel_time_s']:.2f}"),
        ("P95 travel (s)", f"{metrics['p95_travel_time_s']:.2f}"),
        ("Peak mean density", f"{metrics['peak_mean_density']:.2f}"),
        ("Peak cell occupancy", str(metrics["peak_cell_occupancy"])),
        ("Peak bottleneck queue", str(metrics["peak_bottleneck_queue"])),
        ("Peak throughput", str(metrics["peak_bottleneck_throughput"])),
        ("Dominant exit", str(metrics["dominant_exit"])),
    ]
    cards = []
    for label, value in ordered:
        cards.append(
            f"<div class='card'><div class='card-label'>{label}</div><div class='card-value'>{value}</div></div>"
        )
    return "".join(cards)


def _build_dashboard_figure(simulation) -> go.Figure:
    history = simulation.step_history
    first = history[0]
    first_agents = _step_agents(first)
    wall_x, wall_y = _wall_coordinates(simulation.layout)
    times = _time_axis(history)
    exit_labels = list(simulation.exit_labels.values())
    bottleneck_targets = _bottleneck_targets(simulation)
    frame_stride = max(1, len(history) // 120)
    frame_indices = list(range(0, len(history), frame_stride))
    if frame_indices[-1] != len(history) - 1:
        frame_indices.append(len(history) - 1)

    fig = make_subplots(
        rows=2,
        cols=3,
        specs=[
            [{"type": "scatter"}, {"type": "heatmap"}, {"type": "heatmap"}],
            [{"type": "xy"}, {"type": "xy"}, {"type": "xy"}],
        ],
        subplot_titles=(
            "Replay",
            "Occupancy Heatmap",
            "Speed Heatmap",
            "Bottleneck Queue / Throughput",
            "Exit Usage / Evacuation",
            "Density / Speed Timeline",
        ),
        horizontal_spacing=0.08,
        vertical_spacing=0.12,
    )

    fig.add_trace(
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
    fig.add_trace(
        go.Scatter(
            x=[exit_[0] + 0.5 for exit_ in simulation.exit_labels],
            y=[exit_[1] + 0.5 for exit_ in simulation.exit_labels],
            mode="markers",
            marker=dict(size=12, color="#1b9e77", symbol="star"),
            name="Exits",
        ),
        row=1,
        col=1,
    )
    fig.add_trace(
        go.Scatter(
            x=[zone.centroid[0] for zone in simulation.bottleneck_zones],
            y=[zone.centroid[1] for zone in simulation.bottleneck_zones],
            mode="markers",
            marker=dict(
                size=18,
                color="rgba(0,0,0,0)",
                line=dict(color="#e66101", width=2),
                symbol="square-open",
            ),
            name="Bottlenecks",
            text=[zone.zone_id for zone in simulation.bottleneck_zones],
            hovertemplate="Bottleneck %{text}<extra></extra>",
        ),
        row=1,
        col=1,
    )
    fig.add_trace(
        go.Scatter(
            x=[hazard["pos"][0] + 0.5 for hazard in first.hazards],
            y=[hazard["pos"][1] + 0.5 for hazard in first.hazards],
            mode="markers",
            marker=dict(
                size=_hazard_sizes(first.hazards),
                color="rgba(214,39,40,0.18)",
                line=dict(color="#d62728", width=2),
            ),
            name="Hazards",
            customdata=[[hazard["radius"]] for hazard in first.hazards],
            hovertemplate="Hazard radius=%{customdata[0]:.2f}<extra></extra>",
        ),
        row=1,
        col=1,
    )
    trail_x, trail_y = _trail_arrays(first_agents)
    fig.add_trace(
        go.Scatter(
            x=trail_x,
            y=trail_y,
            mode="lines",
            line=dict(color="rgba(31,119,180,0.22)", width=1),
            name="Trails",
            hoverinfo="skip",
            showlegend=False,
        ),
        row=1,
        col=1,
    )
    fig.add_trace(
        go.Scatter(
            x=[agent["position"][0] for agent in first_agents],
            y=[agent["position"][1] for agent in first_agents],
            mode="markers",
            marker=dict(
                size=8,
                color=[agent["speed"] for agent in first_agents],
                colorscale="Turbo",
                cmin=0,
                cmax=max(1.5, max([agent["speed"] for agent in first_agents], default=1.5)),
                colorbar=dict(title="Speed"),
            ),
            name="Agents",
            customdata=_agent_customdata(first_agents),
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
    fig.add_trace(
        go.Heatmap(z=first.occupancy_grid, colorscale="YlOrRd", showscale=False, name="Occupancy"),
        row=1,
        col=2,
    )
    fig.add_trace(
        go.Heatmap(z=first.speed_grid, colorscale="Blues", showscale=False, name="Speed"),
        row=1,
        col=3,
    )

    for zone in bottleneck_targets:
        label = _bottleneck_label(zone)
        fig.add_trace(
            go.Scatter(
                x=[times[0]],
                y=[_bottleneck_series(history, zone, "queue_length")[0]],
                mode="lines",
                line=dict(width=2),
                name=f"{label} queue",
            ),
            row=2,
            col=1,
        )
    for zone in bottleneck_targets:
        label = _bottleneck_label(zone)
        fig.add_trace(
            go.Scatter(
                x=[times[0]],
                y=[_bottleneck_series(history, zone, "outflow")[0]],
                mode="lines",
                line=dict(width=2, dash="dot"),
                name=f"{label} throughput",
            ),
            row=2,
            col=1,
        )

    for label in exit_labels:
        fig.add_trace(
            go.Scatter(
                x=[times[0]],
                y=[_exit_series(history, label)[0]],
                mode="lines",
                line=dict(width=2),
                name=label,
            ),
            row=2,
            col=2,
        )

    fig.add_trace(
        go.Scatter(
            x=[times[0]],
            y=[first.evacuated_total],
            mode="lines",
            line=dict(color="#2ca02c", width=3, dash="dash"),
            name="Evacuated total",
        ),
        row=2,
        col=2,
    )
    fig.add_trace(
        go.Scatter(
            x=[times[0]],
            y=[first.mean_density],
            mode="lines",
            line=dict(color="#d95f02", width=2),
            name="Mean density",
        ),
        row=2,
        col=3,
    )
    fig.add_trace(
        go.Scatter(
            x=[times[0]],
            y=[first.mean_speed],
            mode="lines",
            line=dict(color="#1f78b4", width=2, dash="dot"),
            name="Mean speed",
        ),
        row=2,
        col=3,
    )

    dynamic_traces = list(range(3, len(fig.data)))
    frames = []
    for full_index in frame_indices:
        step = history[full_index]
        agents = _step_agents(step)
        trail_x, trail_y = _trail_arrays(agents)
        frame_data: List[go.BaseTraceType] = [
            go.Scatter(
                x=[hazard["pos"][0] + 0.5 for hazard in step.hazards],
                y=[hazard["pos"][1] + 0.5 for hazard in step.hazards],
                mode="markers",
                marker=dict(
                    size=_hazard_sizes(step.hazards),
                    color="rgba(214,39,40,0.18)",
                    line=dict(color="#d62728", width=2),
                ),
                customdata=[[hazard["radius"]] for hazard in step.hazards],
            ),
            go.Scatter(x=trail_x, y=trail_y, mode="lines"),
            go.Scatter(
                x=[agent["position"][0] for agent in agents],
                y=[agent["position"][1] for agent in agents],
                mode="markers",
                marker=dict(
                    size=8,
                    color=[agent["speed"] for agent in agents],
                    colorscale="Turbo",
                    cmin=0,
                    cmax=max(1.5, max([agent["speed"] for agent in agents], default=1.5)),
                ),
                customdata=_agent_customdata(agents),
            ),
            go.Heatmap(z=step.occupancy_grid, colorscale="YlOrRd", showscale=False),
            go.Heatmap(z=step.speed_grid, colorscale="Blues", showscale=False),
        ]

        upto_times = times[: full_index + 1]
        for zone in bottleneck_targets:
            frame_data.append(
                go.Scatter(
                    x=upto_times,
                    y=_bottleneck_series(history, zone, "queue_length")[: full_index + 1],
                    mode="lines",
                )
            )
        for zone in bottleneck_targets:
            frame_data.append(
                go.Scatter(
                    x=upto_times,
                    y=_bottleneck_series(history, zone, "outflow")[: full_index + 1],
                    mode="lines",
                )
            )
        for label in exit_labels:
            frame_data.append(
                go.Scatter(
                    x=upto_times,
                    y=_exit_series(history, label)[: full_index + 1],
                    mode="lines",
                )
            )
        frame_data.append(
            go.Scatter(
                x=upto_times,
                y=[item.evacuated_total for item in history[: full_index + 1]],
                mode="lines",
            )
        )
        frame_data.append(
            go.Scatter(
                x=upto_times,
                y=[item.mean_density for item in history[: full_index + 1]],
                mode="lines",
            )
        )
        frame_data.append(
            go.Scatter(
                x=upto_times,
                y=[item.mean_speed for item in history[: full_index + 1]],
                mode="lines",
            )
        )
        frames.append(
            go.Frame(
                name=str(step.step),
                data=frame_data,
                traces=dynamic_traces,
            )
        )

    fig.frames = frames
    fig.update_xaxes(range=[0, simulation.layout.width], row=1, col=1, title_text="X")
    fig.update_yaxes(
        range=[0, simulation.layout.height],
        row=1,
        col=1,
        title_text="Y",
        scaleanchor="x",
        scaleratio=1,
    )
    fig.update_xaxes(title_text="X", row=1, col=2)
    fig.update_yaxes(title_text="Y", row=1, col=2, scaleanchor="x2", scaleratio=1)
    fig.update_xaxes(title_text="X", row=1, col=3)
    fig.update_yaxes(title_text="Y", row=1, col=3, scaleanchor="x3", scaleratio=1)
    fig.update_xaxes(title_text="Time (s)", row=2, col=1)
    fig.update_yaxes(title_text="Queue / throughput", row=2, col=1)
    fig.update_xaxes(title_text="Time (s)", row=2, col=2)
    fig.update_yaxes(title_text="Count", row=2, col=2)
    fig.update_xaxes(title_text="Time (s)", row=2, col=3)
    fig.update_yaxes(title_text="Value", row=2, col=3)
    fig.update_layout(
        title="Chiyoda Congestion Study Dashboard",
        height=920,
        template="plotly_white",
        legend=dict(orientation="h", yanchor="bottom", y=1.02, xanchor="right", x=1),
        updatemenus=[
            {
                "type": "buttons",
                "showactive": False,
                "x": 0.02,
                "y": 1.12,
                "buttons": [
                    {
                        "label": "Play",
                        "method": "animate",
                        "args": [
                            None,
                            {
                                "frame": {"duration": 80, "redraw": True},
                                "transition": {"duration": 0},
                                "fromcurrent": True,
                            },
                        ],
                    },
                    {
                        "label": "Pause",
                        "method": "animate",
                        "args": [[None], {"frame": {"duration": 0, "redraw": False}, "mode": "immediate"}],
                    },
                ],
            }
        ],
        sliders=[
            {
                "active": 0,
                "currentvalue": {"prefix": "Step "},
                "pad": {"t": 40},
                "steps": [
                    {
                        "label": str(step.step),
                        "method": "animate",
                        "args": [
                            [str(step.step)],
                            {"frame": {"duration": 0, "redraw": True}, "mode": "immediate"},
                        ],
                    }
                    for step in [history[index] for index in frame_indices]
                ],
            }
        ],
    )
    return fig


def _build_distribution_figure(simulation) -> go.Figure:
    dwell_samples = list(
        np.concatenate(
            [
                np.array(samples, dtype=float)
                for samples in simulation.bottleneck_dwell_samples.values()
                if samples
            ]
        )
    ) if any(simulation.bottleneck_dwell_samples.values()) else []

    fig = make_subplots(
        rows=1,
        cols=3,
        specs=[[{"type": "heatmap"}, {"type": "xy"}, {"type": "xy"}]],
        subplot_titles=(
            "Cumulative Path Usage",
            "Travel Time Distribution",
            "Bottleneck Dwell Distribution",
        ),
        horizontal_spacing=0.08,
    )
    fig.add_trace(
        go.Heatmap(
            z=simulation.step_history[-1].path_usage_grid,
            colorscale="Viridis",
            name="Path usage",
        ),
        row=1,
        col=1,
    )
    fig.add_trace(
        go.Histogram(
            x=simulation.travel_times_s or [0.0],
            marker=dict(color="#2ca02c"),
            name="Travel time",
            nbinsx=20,
        ),
        row=1,
        col=2,
    )
    fig.add_trace(
        go.Histogram(
            x=dwell_samples or [0.0],
            marker=dict(color="#e66101"),
            name="Dwell time",
            nbinsx=20,
        ),
        row=1,
        col=3,
    )
    fig.update_xaxes(title_text="X", row=1, col=1)
    fig.update_yaxes(title_text="Y", row=1, col=1)
    fig.update_xaxes(title_text="Seconds", row=1, col=2)
    fig.update_yaxes(title_text="Agents", row=1, col=2)
    fig.update_xaxes(title_text="Seconds", row=1, col=3)
    fig.update_yaxes(title_text="Samples", row=1, col=3)
    fig.update_layout(
        height=360,
        template="plotly_white",
        bargap=0.1,
        showlegend=False,
    )
    return fig


def generate_report(simulation, output_path: str) -> None:
    from .metrics import SimulationAnalytics

    metrics = SimulationAnalytics().calculate_performance_metrics(simulation)
    dashboard = _build_dashboard_figure(simulation)
    distributions = _build_distribution_figure(simulation)

    html = f"""
<html>
  <head>
    <title>Chiyoda Congestion Study Dashboard</title>
    <style>
      body {{
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        background: #f5f7fb;
        color: #16202a;
        margin: 0;
        padding: 24px;
      }}
      h1 {{
        margin-bottom: 8px;
      }}
      p.lead {{
        margin-top: 0;
        color: #4a5563;
      }}
      .cards {{
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
        gap: 12px;
        margin: 20px 0 28px;
      }}
      .card {{
        background: white;
        border-radius: 12px;
        padding: 14px 16px;
        box-shadow: 0 8px 24px rgba(15, 23, 42, 0.06);
      }}
      .card-label {{
        font-size: 12px;
        color: #637083;
        text-transform: uppercase;
        letter-spacing: 0.04em;
      }}
      .card-value {{
        margin-top: 6px;
        font-size: 24px;
        font-weight: 700;
      }}
      .panel {{
        background: white;
        border-radius: 16px;
        padding: 12px;
        box-shadow: 0 10px 28px rgba(15, 23, 42, 0.07);
        margin-bottom: 18px;
      }}
    </style>
  </head>
  <body>
    <h1>Chiyoda Congestion Study Dashboard</h1>
    <p class="lead">Replay the run, inspect occupancy and speed heatmaps, and track bottleneck performance from the same simulation timeline.</p>
    <div class="cards">{_summary_cards(metrics)}</div>
    <div class="panel">{dashboard.to_html(full_html=False, include_plotlyjs='cdn')}</div>
    <div class="panel">{distributions.to_html(full_html=False, include_plotlyjs=False)}</div>
  </body>
</html>
"""

    with open(output_path, "w") as handle:
        handle.write(html)
