"""
Information layer for ITED framework.

Models heterogeneous information propagation, belief states,
and Shannon entropy across agent populations during evacuation.
"""

from chiyoda.information.field import BeliefVector, InformationField
from chiyoda.information.propagation import GossipModel
from chiyoda.information.entropy import (
    agent_entropy,
    global_entropy,
    belief_accuracy,
    information_efficiency,
)

__all__ = [
    "BeliefVector",
    "InformationField",
    "GossipModel",
    "agent_entropy",
    "global_entropy",
    "belief_accuracy",
    "information_efficiency",
]
