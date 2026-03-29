from __future__ import annotations

from dataclasses import dataclass
import numpy as np
from .base import AgentBase


@dataclass
class Commuter(AgentBase):
    personality: str = "NORMAL"  # NORMAL, ELDERLY, WHEELCHAIR, INJURED, HELPING
    calmness: float = 0.8  # 0-1

    def __post_init__(self):
        # Adjust base speed for accessibility types
        if self.personality == "ELDERLY":
            self.base_speed *= 0.7
        elif self.personality == "WHEELCHAIR":
            self.base_speed *= 0.8
        elif self.personality == "INJURED":
            self.base_speed *= 0.4
        elif self.personality == "HELPING":
            self.base_speed *= 0.85
