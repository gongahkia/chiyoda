from __future__ import annotations

from dataclasses import dataclass


@dataclass
class Exit:
    pos: tuple[int, int]
    capacity: int = 2  # agents per tick (abstracted)
    kind: str = "default"  # stairs/escalator/emergency
