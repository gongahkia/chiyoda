from __future__ import annotations

from dataclasses import dataclass
from typing import Tuple


@dataclass
class Exit:
    pos: Tuple[int, int]
    capacity: int = 2  # agents per tick (abstracted)
    kind: str = "default"  # stairs/escalator/emergency
