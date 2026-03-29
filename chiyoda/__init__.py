"""
Chiyoda v2 package

High-level modules for crowd dynamics simulation and research analysis.

Subpackages:
- core: Simulation orchestration and runtime
- agents: Agent definitions and behaviors
- environment: Layouts, obstacles, exits, hazards
- navigation: Pathfinding, social forces, spatial indexing
- acceleration: Python and optional Julia runtime backends
- analysis: Metrics and offline report exporters
- studies: Study schemas, bundle models, and comparison workflows
- scenarios: Scenario loading and management
"""

from .__version__ import __version__

__all__ = ["__version__"]
