from __future__ import annotations

from typing import Optional
import numpy as np
import plotly.graph_objects as go


class InteractiveVisualizer:
    """Interactive Plotly visualization with agent scatter and density heatmap."""

    def __init__(self) -> None:
        self.fig: Optional[go.Figure] = None
        self._grid_shape = None

    def init(self, simulation) -> None:
        state = simulation.live_state()
        positions = state["positions"]
        h, w = simulation.layout.height, simulation.layout.width
        self._grid_shape = (h, w)

        density = np.zeros((h, w))  # initialized density grid
        scatter = go.Scatter(
            x=positions[:, 0] if len(positions) else [],
            y=positions[:, 1] if len(positions) else [],
            mode="markers",
            marker=dict(size=4, color="blue"),
            name="Agents",
        )
        heatmap = go.Heatmap(z=density, colorscale="YlOrRd", opacity=0.6, name="Density")

        self.fig = go.Figure(data=[heatmap, scatter])
        self.fig.update_layout(
            title=f"Chiyoda v2 - Live Crowd Dynamics",
            xaxis=dict(range=[0, w], title="X"),
            yaxis=dict(range=[0, h], title="Y", scaleanchor="x", scaleratio=1),
            height=700,
        )

    def _update_density(self, positions: np.ndarray) -> np.ndarray:
        h, w = self._grid_shape
        density = np.zeros((h, w))
        for p in positions:
            x, y = int(np.clip(np.round(p[0]), 0, w - 1)), int(np.clip(np.round(p[1]), 0, h - 1))
            density[y, x] += 1
        # Simple smoothing via convolution kernel
        # 3x3 smoothing kernel (heavier center weight)
        kernel = np.array(
            [[0.05, 0.1, 0.05], [0.1, 0.4, 0.1], [0.05, 0.1, 0.05]]
        )
        from scipy.signal import convolve2d

        return convolve2d(density, kernel, mode="same", boundary="symm")

    def on_step(self, simulation) -> None:
        if self.fig is None:
            self.init(simulation)

        state = simulation.live_state()
        positions = state["positions"]
        density = self._update_density(positions)
        # Update traces: heatmap is trace 0, scatter is trace 1
        self.fig.data[0].z = density
        self.fig.data[1].x = positions[:, 0] if len(positions) else []
        self.fig.data[1].y = positions[:, 1] if len(positions) else []

    def show(self):
        if self.fig is not None:
            self.fig.show()

    def export_html(self, path: str) -> None:
        if self.fig is not None:
            self.fig.write_html(path)
