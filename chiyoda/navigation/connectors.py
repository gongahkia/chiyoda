"""Queued connector traversal for multi-floor evacuation links.

Default stair flow is anchored to NIST Technical Note 1839, "Movement on Stairs
During Building Evacuations", which reports stair evacuation data from 14
buildings and more than 22,000 individual measurements:
https://nvlpubs.nist.gov/nistpubs/TechnicalNotes/NIST.TN.1839.pdf
"""
from __future__ import annotations

from dataclasses import dataclass, field
from heapq import heappop, heappush
from itertools import count
from typing import Any, Dict, Iterable, Optional

import numpy as np


@dataclass(frozen=True)
class ConnectorQueueEvent:
    phase: str
    agent_id: int
    connector_id: str
    connector_type: str
    source: tuple
    target: tuple
    queue_length: int
    flow_rate: float
    capacity_used: int


@dataclass
class ConnectorTransfer:
    agent_id: int
    source: tuple
    target: tuple
    arrival_s: float


@dataclass
class ConnectorQueue:
    connector: Any
    flow_rate: float
    capacity: int
    queue_mode: str = "fifo"
    panic_jam_density: Optional[float] = None
    jam_flow_multiplier: float = 0.35
    service_credit: float = 1.0
    _sequence: Iterable[int] = field(default_factory=count)
    _waiting: list[tuple[float, int, int, tuple, tuple]] = field(default_factory=list)
    _waiting_ids: set[int] = field(default_factory=set)
    _active: Dict[int, ConnectorTransfer] = field(default_factory=dict)
    cumulative_started: int = 0
    cumulative_finished: int = 0
    flow_step: int = 0
    last_effective_flow_rate: float = 0.0

    @classmethod
    def from_connector(cls, connector: Any) -> "ConnectorQueue":
        width = max(float(getattr(connector, "width", 1.0)), 0.1)
        flow_rate = getattr(connector, "flow_rate", None)
        if flow_rate is None:
            flow_rate = _default_flow_rate(connector.type, width)
        capacity = getattr(connector, "capacity", None)
        if capacity is None:
            capacity = _default_capacity(connector.type, width)
        return cls(
            connector=connector,
            flow_rate=max(0.0, float(flow_rate)),
            capacity=max(1, int(capacity)),
            queue_mode=str(getattr(connector, "queue_mode", "fifo")),
            panic_jam_density=getattr(connector, "panic_jam_density", None),
            jam_flow_multiplier=float(getattr(connector, "jam_flow_multiplier", 0.35)),
        )

    def reset_step(self) -> None:
        self.flow_step = 0

    def enqueue(self, agent_id: int, source: tuple, target: tuple, *, priority: float = 0.0) -> bool:
        if agent_id in self._waiting_ids or agent_id in self._active:
            return False
        seq = next(self._sequence)
        order = -float(priority) if self.queue_mode == "priority" else float(seq)
        heappush(self._waiting, (order, seq, int(agent_id), tuple(source), tuple(target)))
        self._waiting_ids.add(int(agent_id))
        return True

    def step(self, *, time_s: float, dt: float, density: float) -> list[ConnectorQueueEvent]:
        events: list[ConnectorQueueEvent] = []
        effective_flow = self.effective_flow_rate(density)
        self.last_effective_flow_rate = effective_flow
        self.service_credit = min(self.capacity, self.service_credit + effective_flow * dt)
        while self._waiting and len(self._active) < self.capacity and self.service_credit >= 1.0:
            _order, _seq, agent_id, source, target = heappop(self._waiting)
            self._waiting_ids.remove(agent_id)
            self.service_credit -= 1.0
            duration = self.transfer_duration(source, target)
            self._active[agent_id] = ConnectorTransfer(
                agent_id=agent_id,
                source=source,
                target=target,
                arrival_s=time_s + duration,
            )
            self.cumulative_started += 1
            events.append(self._event("start", agent_id, source, target))
        return events

    def finish_ready(self, *, time_s: float) -> list[ConnectorQueueEvent]:
        events: list[ConnectorQueueEvent] = []
        ready = [
            agent_id for agent_id, transfer in self._active.items()
            if time_s >= transfer.arrival_s
        ]
        for agent_id in ready:
            transfer = self._active.pop(agent_id)
            self.cumulative_finished += 1
            self.flow_step += 1
            events.append(self._event("finish", agent_id, transfer.source, transfer.target))
        return events

    def has_agent(self, agent_id: int) -> bool:
        return int(agent_id) in self._waiting_ids or int(agent_id) in self._active

    def active_transfer(self, agent_id: int) -> Optional[ConnectorTransfer]:
        return self._active.get(int(agent_id))

    def waiting_count(self) -> int:
        return len(self._waiting)

    def active_count(self) -> int:
        return len(self._active)

    def effective_flow_rate(self, density: float) -> float:
        base = float(self.flow_rate)
        threshold = self.panic_jam_density
        if threshold is None or threshold <= 0:
            return base
        if density < threshold:
            return base
        return base * max(0.0, min(1.0, self.jam_flow_multiplier))

    def transfer_duration(self, source: tuple, target: tuple) -> float:
        travel_s = float(getattr(self.connector, "travel_s", 0.0))
        dwell_s = float(getattr(self.connector, "dwell_s", 0.0))
        if travel_s > 0 or self.connector.type == "elevator":
            return max(0.01, dwell_s + travel_s)
        source_point = _point3(source)
        target_point = _point3(target)
        distance = float(np.linalg.norm(target_point - source_point))
        if len(source) >= 3 and len(target) >= 3 and source[0] != target[0] and distance < 1e-6:
            distance = 3.0
        speed = 1.34 * max(float(getattr(self.connector, "speed_multiplier", 1.0)), 1e-6)
        return max(0.01, dwell_s + distance / speed)

    def telemetry(self) -> dict[str, float | int]:
        return {
            "flow_rate": float(self.last_effective_flow_rate),
            "base_flow_rate": float(self.flow_rate),
            "flow_step": int(self.flow_step),
            "capacity": int(self.capacity),
            "capacity_used": int(len(self._active)),
            "queue_length": int(len(self._waiting)),
            "cumulative_started": int(self.cumulative_started),
            "cumulative_finished": int(self.cumulative_finished),
        }

    def _event(self, phase: str, agent_id: int, source: tuple, target: tuple) -> ConnectorQueueEvent:
        return ConnectorQueueEvent(
            phase=phase,
            agent_id=int(agent_id),
            connector_id=self.connector.id,
            connector_type=self.connector.type,
            source=tuple(source),
            target=tuple(target),
            queue_length=self.waiting_count(),
            flow_rate=float(self.last_effective_flow_rate),
            capacity_used=self.active_count(),
        )


def _default_flow_rate(connector_type: str, width: float) -> float:
    if connector_type == "elevator":
        return 1.0
    if connector_type == "escalator":
        return 1.25 * width
    if connector_type == "ramp":
        return 0.9 * width
    return 1.0 * width


def _default_capacity(connector_type: str, width: float) -> int:
    if connector_type == "elevator":
        return 1
    return max(1, int(round(width * 2.0)))


def _point3(cell: tuple) -> np.ndarray:
    if len(cell) >= 3 and isinstance(cell[0], str):
        return np.array([float(cell[1]) + 0.5, float(cell[2]) + 0.5, 0.0], dtype=float)
    if len(cell) >= 3:
        return np.array([float(cell[0]), float(cell[1]), float(cell[2])], dtype=float)
    return np.array([float(cell[0]), float(cell[1]), 0.0], dtype=float)
