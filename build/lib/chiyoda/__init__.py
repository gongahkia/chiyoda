"""
Chiyoda v3 — Information-Theoretic Evacuation Dynamics (ITED)

A computational framework for studying how heterogeneous information
propagation, coupled hazard dynamics, and bounded-rational decision-making
interact to shape evacuation outcomes in spatial environments.

Subpackages:
    core           Simulation engine and runtime loop
    agents         CognitiveAgent (BDI), Commuter, FirstResponder
    information    InformationField, GossipModel, Shannon entropy metrics
    environment    Layout, multi-hazard physics, exits, obstacles
    navigation     Social Force Model, belief-weighted pathfinding, spatial index
    acceleration   Python and optional Julia compute backends
    analysis       Metrics, telemetry, report/figure generation
    studies        Study schemas, bundle persistence, comparison workflows
    scenarios      YAML scenario loading and management
"""

from .__version__ import __version__

__all__ = ["__version__"]
