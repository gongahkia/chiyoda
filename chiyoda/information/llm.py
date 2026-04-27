"""
Provider-neutral LLM message generation primitives.

The simulator treats generated language as a safety-control proposal, not as
free-form operational advice. This module therefore focuses on deterministic
replay, structured outputs, and validation hooks. Live API clients can be added
behind the same interface later without changing paper runs.
"""
from __future__ import annotations

from dataclasses import asdict, dataclass, field
import hashlib
import json
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple


Cell = Tuple[int, int]
Point = Tuple[float, float]


@dataclass(frozen=True)
class HazardSnapshot:
    position: Point
    kind: str
    radius: float
    severity: float


@dataclass(frozen=True)
class LLMMessageRequest:
    policy: str
    step: int
    target: Point
    selected_reason: str
    objective: str
    exits: Sequence[Cell]
    hazards: Sequence[HazardSnapshot]
    congested_exits: Sequence[Cell]
    recipients_estimate: int
    mean_local_density: float
    mean_hazard_load: float


@dataclass
class GeneratedEvacuationMessage:
    message_type: str = "route_guidance"
    text: str = ""
    recommended_exits: List[Cell] = field(default_factory=list)
    avoid_exits: List[Cell] = field(default_factory=list)
    hazard_positions: List[Point] = field(default_factory=list)
    radius: Optional[float] = None
    credibility: Optional[float] = None
    confidence: float = 0.0
    abstain: bool = False
    provider: str = "deterministic"
    model: str = "template"
    raw_response: Dict[str, Any] = field(default_factory=dict)


@dataclass
class ValidationResult:
    accepted: bool
    reasons: List[str] = field(default_factory=list)

    @property
    def status(self) -> str:
        return "accepted" if self.accepted else "rejected"


@dataclass
class LLMGenerationRecord:
    cache_key: str
    request: LLMMessageRequest
    message: GeneratedEvacuationMessage
    validation: ValidationResult

    def to_json_dict(self) -> Dict[str, Any]:
        return {
            "cache_key": self.cache_key,
            "request": _to_jsonable(self.request),
            "message": _to_jsonable(self.message),
            "validation": _to_jsonable(self.validation),
        }

    @classmethod
    def from_json_dict(cls, payload: Dict[str, Any]) -> "LLMGenerationRecord":
        request_payload = payload["request"]
        message_payload = payload["message"]
        validation_payload = payload["validation"]
        return cls(
            cache_key=str(payload["cache_key"]),
            request=LLMMessageRequest(
                policy=str(request_payload["policy"]),
                step=int(request_payload["step"]),
                target=tuple(request_payload["target"]),
                selected_reason=str(request_payload["selected_reason"]),
                objective=str(request_payload["objective"]),
                exits=[tuple(item) for item in request_payload["exits"]],
                hazards=[
                    HazardSnapshot(
                        position=tuple(item["position"]),
                        kind=str(item["kind"]),
                        radius=float(item["radius"]),
                        severity=float(item["severity"]),
                    )
                    for item in request_payload["hazards"]
                ],
                congested_exits=[tuple(item) for item in request_payload["congested_exits"]],
                recipients_estimate=int(request_payload["recipients_estimate"]),
                mean_local_density=float(request_payload["mean_local_density"]),
                mean_hazard_load=float(request_payload["mean_hazard_load"]),
            ),
            message=GeneratedEvacuationMessage(
                message_type=str(message_payload.get("message_type", "route_guidance")),
                text=str(message_payload.get("text", "")),
                recommended_exits=[tuple(item) for item in message_payload.get("recommended_exits", [])],
                avoid_exits=[tuple(item) for item in message_payload.get("avoid_exits", [])],
                hazard_positions=[tuple(item) for item in message_payload.get("hazard_positions", [])],
                radius=None if message_payload.get("radius") is None else float(message_payload["radius"]),
                credibility=None
                if message_payload.get("credibility") is None
                else float(message_payload["credibility"]),
                confidence=float(message_payload.get("confidence", 0.0)),
                abstain=bool(message_payload.get("abstain", False)),
                provider=str(message_payload.get("provider", "deterministic")),
                model=str(message_payload.get("model", "template")),
                raw_response=dict(message_payload.get("raw_response", {})),
            ),
            validation=ValidationResult(
                accepted=bool(validation_payload["accepted"]),
                reasons=[str(item) for item in validation_payload.get("reasons", [])],
            ),
        )


class LLMMessageCache:
    """Content-addressed JSON cache for deterministic replay."""

    def __init__(self, path: Path | str) -> None:
        self.path = Path(path)

    def key_for(self, request: LLMMessageRequest) -> str:
        payload = json.dumps(_to_jsonable(request), sort_keys=True, separators=(",", ":"))
        return hashlib.sha256(payload.encode("utf-8")).hexdigest()

    def load(self, key: str) -> Optional[LLMGenerationRecord]:
        record_path = self.path / f"{key}.json"
        if not record_path.exists():
            return None
        return LLMGenerationRecord.from_json_dict(json.loads(record_path.read_text()))

    def store(self, record: LLMGenerationRecord) -> None:
        self.path.mkdir(parents=True, exist_ok=True)
        record_path = self.path / f"{record.cache_key}.json"
        record_path.write_text(json.dumps(record.to_json_dict(), indent=2, sort_keys=True))


class LLMMessageGenerator:
    provider = "base"
    model = "base"

    def generate(self, request: LLMMessageRequest, cache_key: str) -> GeneratedEvacuationMessage:
        raise NotImplementedError


class ReplayOnlyGenerator(LLMMessageGenerator):
    provider = "cache"
    model = "replay_only"

    def __init__(self, cache: LLMMessageCache) -> None:
        self.cache = cache

    def generate(self, request: LLMMessageRequest, cache_key: str) -> GeneratedEvacuationMessage:
        record = self.cache.load(cache_key)
        if record is None:
            return GeneratedEvacuationMessage(
                text="No cached generated message is available.",
                abstain=True,
                provider=self.provider,
                model=self.model,
            )
        return record.message


class TemplateLLMGenerator(LLMMessageGenerator):
    """Deterministic stand-in used for tests and paper-safe dry runs."""

    provider = "deterministic"
    model = "template"

    def generate(self, request: LLMMessageRequest, cache_key: str) -> GeneratedEvacuationMessage:
        recommended = [exit_ for exit_ in request.exits if exit_ not in request.congested_exits]
        if not recommended:
            recommended = list(request.exits)
        hazard_positions = [hazard.position for hazard in request.hazards]
        return GeneratedEvacuationMessage(
            message_type="route_guidance",
            text="Proceed calmly to an available exit and avoid congested routes.",
            recommended_exits=list(recommended),
            avoid_exits=list(request.congested_exits),
            hazard_positions=list(hazard_positions),
            confidence=0.75,
            provider=self.provider,
            model=self.model,
        )


def validate_generated_message(
    message: GeneratedEvacuationMessage,
    *,
    known_exits: Sequence[Cell],
    known_hazards: Sequence[HazardSnapshot],
    base_radius: float,
    max_radius: float,
    base_credibility: float,
) -> ValidationResult:
    reasons: List[str] = []
    known_exit_set = {tuple(exit_) for exit_ in known_exits}

    if message.abstain:
        reasons.append("generator_abstained")
    for exit_ in message.recommended_exits:
        if tuple(exit_) not in known_exit_set:
            reasons.append(f"invented_exit:{tuple(exit_)}")
    for exit_ in message.avoid_exits:
        if tuple(exit_) not in known_exit_set:
            reasons.append(f"invented_avoid_exit:{tuple(exit_)}")

    known_hazard_positions = [hazard.position for hazard in known_hazards]
    for hazard_pos in message.hazard_positions:
        if not _near_any(hazard_pos, known_hazard_positions, tolerance=3.0):
            reasons.append(f"invented_hazard:{tuple(hazard_pos)}")

    if message.radius is not None and (message.radius <= 0.0 or message.radius > max_radius):
        reasons.append(f"unsafe_radius:{message.radius}")
    if message.credibility is not None and not 0.0 <= message.credibility <= base_credibility:
        reasons.append(f"unsafe_credibility:{message.credibility}")

    if not message.recommended_exits and not message.abstain:
        reasons.append("no_recommended_exit")

    return ValidationResult(accepted=not reasons, reasons=reasons)


def build_llm_request(simulation, target, objective: str, policy: str) -> LLMMessageRequest:
    active = [
        agent for agent in simulation._active_agents()
        if _distance((float(agent.pos[0]), float(agent.pos[1])), target.point)
        <= float(getattr(target, "radius_hint", float("inf")))
    ]
    return LLMMessageRequest(
        policy=policy,
        step=int(simulation.current_step),
        target=target.point,
        selected_reason=target.reason,
        objective=objective,
        exits=[tuple(exit_.pos) for exit_ in simulation.exits],
        hazards=[
            HazardSnapshot(
                position=(float(hazard.pos[0]), float(hazard.pos[1])),
                kind=str(hazard.kind),
                radius=float(hazard.radius),
                severity=float(hazard.severity),
            )
            for hazard in simulation.hazards
        ],
        congested_exits=[],
        recipients_estimate=len(active),
        mean_local_density=_mean([float(getattr(agent, "local_density", 0.0)) for agent in active]),
        mean_hazard_load=_mean([float(getattr(agent, "current_hazard_load", 0.0)) for agent in active]),
    )


def _to_jsonable(value: Any) -> Any:
    if hasattr(value, "__dataclass_fields__"):
        return {key: _to_jsonable(item) for key, item in asdict(value).items()}
    if isinstance(value, tuple):
        return [_to_jsonable(item) for item in value]
    if isinstance(value, list):
        return [_to_jsonable(item) for item in value]
    if isinstance(value, dict):
        return {str(key): _to_jsonable(item) for key, item in value.items()}
    return value


def _near_any(point: Point, candidates: Sequence[Point], tolerance: float) -> bool:
    return any(_distance(point, candidate) <= tolerance for candidate in candidates)


def _distance(a: Point, b: Point) -> float:
    return ((float(a[0]) - float(b[0])) ** 2 + (float(a[1]) - float(b[1])) ** 2) ** 0.5


def _mean(values: Sequence[float]) -> float:
    if not values:
        return 0.0
    return sum(values) / len(values)
