from __future__ import annotations

from dataclasses import dataclass
from time import perf_counter
from typing import Any

import math


@dataclass(frozen=True)
class DispatchProjectionRequest:
    message_type: str = "route_guidance"
    target: tuple[float, float, float] = (0.0, 0.0, 0.0)
    radius: float = 8.0
    credibility: float = 0.9
    horizon_steps: int = 30


@dataclass(frozen=True)
class DispatchProjectionResult:
    recipients: int
    affected_agents: int
    mean_belief_delta: float
    harmful_convergence_delta: float
    exposure_delta: float
    latency_ms: float
    horizon_steps: int

    def to_dict(self) -> dict[str, Any]:
        return {
            "recipients": self.recipients,
            "affected_agents": self.affected_agents,
            "mean_belief_delta": self.mean_belief_delta,
            "harmful_convergence_delta": self.harmful_convergence_delta,
            "exposure_delta": self.exposure_delta,
            "latency_ms": self.latency_ms,
            "horizon_steps": self.horizon_steps,
        }


def project_dispatch_message(
    agents: list[dict[str, Any]],
    hazards: list[dict[str, Any]],
    request: DispatchProjectionRequest,
) -> DispatchProjectionResult:
    started = perf_counter()
    radius = max(0.1, float(request.radius))
    credibility = min(1.0, max(0.0, float(request.credibility)))
    horizon_scale = min(1.0, max(1, int(request.horizon_steps)) / 60.0)
    recipients = [
        agent
        for agent in agents
        if _distance(_agent_point(agent), request.target) <= radius
    ]
    affected = len(recipients)
    if not agents or not recipients:
        return DispatchProjectionResult(0, affected, 0.0, 0.0, 0.0, _elapsed(started), int(request.horizon_steps))

    pressure = [_hazard_pressure(agent, hazards) for agent in recipients]
    mean_pressure = sum(pressure) / max(1, len(pressure))
    recipient_share = len(recipients) / max(1, len(agents))
    uncertainty = sum(float(agent.get("entropy", 0.0) or 0.0) for agent in recipients) / max(1, len(recipients))
    kind = str(request.message_type or "route_guidance")
    belief_factor, hci_factor, exposure_factor = _message_coefficients(kind, bool(hazards))
    effect = credibility * horizon_scale * recipient_share
    belief_delta = belief_factor * credibility * horizon_scale * (0.5 + uncertainty)
    hci_delta = hci_factor * effect * (1.0 + mean_pressure)
    exposure_delta = exposure_factor * effect * (0.25 + mean_pressure)
    return DispatchProjectionResult(
        recipients=len(recipients),
        affected_agents=affected,
        mean_belief_delta=round(belief_delta, 6),
        harmful_convergence_delta=round(hci_delta, 6),
        exposure_delta=round(exposure_delta, 6),
        latency_ms=_elapsed(started),
        horizon_steps=int(request.horizon_steps),
    )


def project_from_viewer_frame(
    frame: dict[str, Any],
    hazards: list[dict[str, Any]],
    request: DispatchProjectionRequest,
) -> DispatchProjectionResult:
    agents = list(frame.get("agents", []) or [])
    return project_dispatch_message(agents, hazards, request)


def _message_coefficients(message_type: str, has_hazard: bool) -> tuple[float, float, float]:
    if message_type == "avoid_hazard":
        return 0.14, -0.06, -0.22
    if message_type == "shelter_hold":
        return 0.05, 0.05, 0.10
    if message_type == "all_clear":
        return 0.08, 0.02 if has_hazard else -0.02, 0.08 if has_hazard else -0.03
    return 0.18, -0.10, -0.12


def _agent_point(agent: dict[str, Any]) -> tuple[float, float, float]:
    return (
        float(agent.get("x", 0.0) or 0.0),
        float(agent.get("y", 0.0) or 0.0),
        float(agent.get("z", 0.0) or 0.0),
    )


def _hazard_pressure(agent: dict[str, Any], hazards: list[dict[str, Any]]) -> float:
    point = _agent_point(agent)
    pressure = 0.0
    for hazard in hazards:
        radius = max(0.1, float(hazard.get("radius", 1.0) or 1.0))
        severity = float(hazard.get("severity", 1.0) or 1.0)
        center = (
            float(hazard.get("x", 0.0) or 0.0),
            float(hazard.get("y", 0.0) or 0.0),
            float(hazard.get("z", 0.0) or 0.0),
        )
        pressure = max(pressure, max(0.0, 1.0 - (_distance(point, center) / radius)) * severity)
    return pressure


def _distance(a: tuple[float, float, float], b: tuple[float, float, float]) -> float:
    return math.sqrt(
        ((a[0] - b[0]) ** 2)
        + ((a[1] - b[1]) ** 2)
        + ((a[2] - b[2]) ** 2)
    )


def _elapsed(started: float) -> float:
    return round((perf_counter() - started) * 1000.0, 3)
