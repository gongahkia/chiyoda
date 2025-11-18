"""
Chiyoda v2 package

High-level modules for crowd dynamics simulation, visualization, and analysis.

Subpackages:
- core: Simulation orchestration and runtime
- agents: Agent definitions and behaviors
- environment: Layouts, obstacles, exits, hazards
- navigation: Pathfinding, social forces, spatial indexing
- visualization: Plotly/Matplotlib visualizers
- analysis: Metrics and reporting
- scenarios: Scenario loading and management
- utils: Config and helpers
"""

from .__version__ import __version__

__all__ = ["__version__"]
