"""Commuter — default evacuating agent with familiarity parameter."""
from __future__ import annotations
from dataclasses import dataclass
from chiyoda.agents.base import CognitiveAgent

@dataclass
class Commuter(CognitiveAgent):
    personality: str = "NORMAL"
    calmness: float = 0.8

    def __post_init__(self):
        self.base_rationality = self.calmness
        self.rationality = self.calmness
        if self.personality == "NERVOUS":
            self.base_rationality *= 0.7
            self.rationality *= 0.7
