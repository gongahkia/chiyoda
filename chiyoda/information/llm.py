"""
Provider-neutral LLM message generation primitives.

The simulator treats generated language as a safety-control proposal, not as
free-form operational advice. This module therefore focuses on deterministic
replay, structured outputs, and validation hooks. Live API clients can be added
behind the same interface later without changing replayed runs.
"""

from __future__ import annotations

import hashlib
import json
import os
from collections.abc import Sequence
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any
from urllib import request as urlrequest
from urllib.error import HTTPError, URLError

Cell = tuple
Point = tuple
DEFAULT_OPENAI_MODEL = "gpt-5.5"
DEFAULT_ANTHROPIC_MODEL = "claude-sonnet-4-5-20250929"
DEFAULT_ANTHROPIC_VERSION = "2023-06-01"
LLM_AUDIT_CHAIN_PREV_HASH = "audit_chain_prev_hash"
LLM_AUDIT_CHAIN_HASH = "audit_chain_hash"
LLM_AUDIT_GENESIS = "chiyoda_llm_calls_genesis_v1"
LLM_AUDIT_CHAIN_COLUMNS = (LLM_AUDIT_CHAIN_PREV_HASH, LLM_AUDIT_CHAIN_HASH)
ATTACK_TEXT_MARKERS = (
    ("aitm_interception", "intercepted"),
    ("aitm_interception", "agent-in-the-middle"),
    ("aitm_interception", "man-in-the-middle"),
    ("instruction_override", "ignore previous"),
    ("instruction_override", "override safety"),
    ("instruction_override", "bypass validator"),
    ("coercive_persuasion", "do not verify"),
    ("coercive_persuasion", "trust me"),
    ("coercive_persuasion", "everyone else"),
    ("coercive_persuasion", "only safe exit"),
    ("source_spoofing", "spoofed authority"),
    ("source_spoofing", "fake dispatcher"),
)


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
    prompt_style: str = "safety"


@dataclass
class GeneratedEvacuationMessage:
    message_type: str = "route_guidance"
    text: str = ""
    recommended_exits: list[Cell] = field(default_factory=list)
    avoid_exits: list[Cell] = field(default_factory=list)
    hazard_positions: list[Point] = field(default_factory=list)
    radius: float | None = None
    credibility: float | None = None
    confidence: float = 0.0
    abstain: bool = False
    provider: str = "deterministic"
    model: str = "template"
    raw_response: dict[str, Any] = field(default_factory=dict)


@dataclass
class ValidationResult:
    accepted: bool
    reasons: list[str] = field(default_factory=list)

    @property
    def status(self) -> str:
        return "accepted" if self.accepted else "rejected"


@dataclass(frozen=True)
class ValidatorSettings:
    profile: str = "standard"
    reject_congested_recommendations: bool = True
    reject_vague_guidance: bool = True
    reject_low_confidence: bool = True
    min_confidence: float = 0.2


@dataclass
class LLMGenerationRecord:
    cache_key: str
    request: LLMMessageRequest
    message: GeneratedEvacuationMessage
    validation: ValidationResult
    judge_verdict: dict[str, Any] | None = None

    def to_json_dict(self) -> dict[str, Any]:
        payload = {
            "cache_key": self.cache_key,
            "request": _to_jsonable(self.request),
            "message": _to_jsonable(self.message),
            "validation": _to_jsonable(self.validation),
        }
        if self.judge_verdict is not None:
            payload["judge_verdict"] = _to_jsonable(self.judge_verdict)
        return payload

    @classmethod
    def from_json_dict(cls, payload: dict[str, Any]) -> LLMGenerationRecord:
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
                prompt_style=str(request_payload.get("prompt_style", "safety")),
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
                congested_exits=[
                    tuple(item) for item in request_payload["congested_exits"]
                ],
                recipients_estimate=int(request_payload["recipients_estimate"]),
                mean_local_density=float(request_payload["mean_local_density"]),
                mean_hazard_load=float(request_payload["mean_hazard_load"]),
            ),
            message=GeneratedEvacuationMessage(
                message_type=str(message_payload.get("message_type", "route_guidance")),
                text=str(message_payload.get("text", "")),
                recommended_exits=[
                    tuple(item) for item in message_payload.get("recommended_exits", [])
                ],
                avoid_exits=[
                    tuple(item) for item in message_payload.get("avoid_exits", [])
                ],
                hazard_positions=[
                    tuple(item) for item in message_payload.get("hazard_positions", [])
                ],
                radius=(
                    None
                    if message_payload.get("radius") is None
                    else float(message_payload["radius"])
                ),
                credibility=(
                    None
                    if message_payload.get("credibility") is None
                    else float(message_payload["credibility"])
                ),
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
            judge_verdict=payload.get("judge_verdict"),
        )


@dataclass(frozen=True)
class LLMAuditVerification:
    ok: bool
    row_count: int
    first_bad_row: int | None = None
    reason: str = ""
    expected: str = ""
    actual: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "ok": self.ok,
            "row_count": self.row_count,
            "first_bad_row": self.first_bad_row,
            "reason": self.reason,
            "expected": self.expected,
            "actual": self.actual,
        }


@dataclass(frozen=True)
class LLMBudgetCheck:
    allowed: bool
    reason: str
    estimated_input_tokens: int
    estimated_output_tokens: int
    estimated_total_tokens: int
    estimated_usd: float


@dataclass
class LLMBudgetGuard:
    max_calls: int | None = None
    max_estimated_tokens: int | None = None
    max_estimated_usd: float | None = None
    input_usd_per_mtok: float = 0.0
    output_usd_per_mtok: float = 0.0
    calls_used: int = 0
    estimated_tokens_used: int = 0
    estimated_usd_used: float = 0.0

    def evaluate(self, input_tokens: int, output_tokens: int) -> LLMBudgetCheck:
        total = int(input_tokens) + int(output_tokens)
        usd = estimate_llm_cost(
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            input_usd_per_mtok=self.input_usd_per_mtok,
            output_usd_per_mtok=self.output_usd_per_mtok,
        )
        if self.max_calls is not None and self.calls_used >= self.max_calls:
            allowed = False
            reason = "max_calls_exceeded"
        elif (
            self.max_estimated_tokens is not None
            and self.estimated_tokens_used + total > self.max_estimated_tokens
        ):
            allowed = False
            reason = "max_estimated_tokens_exceeded"
        elif (
            self.max_estimated_usd is not None
            and self.estimated_usd_used + usd > self.max_estimated_usd
        ):
            allowed = False
            reason = "max_estimated_usd_exceeded"
        else:
            allowed = True
            reason = "allowed"
        return LLMBudgetCheck(
            allowed=allowed,
            reason=reason,
            estimated_input_tokens=int(input_tokens),
            estimated_output_tokens=int(output_tokens),
            estimated_total_tokens=total,
            estimated_usd=float(usd),
        )

    def record(self, check: LLMBudgetCheck) -> None:
        if not check.allowed:
            return
        self.calls_used += 1
        self.estimated_tokens_used += check.estimated_total_tokens
        self.estimated_usd_used += check.estimated_usd


class LLMMessageCache:
    """Content-addressed JSON cache for deterministic replay."""

    def __init__(self, path: Path | str) -> None:
        self.path = Path(path)

    def key_for(self, request: LLMMessageRequest) -> str:
        payload = json.dumps(
            _to_jsonable(request), sort_keys=True, separators=(",", ":")
        )
        return hashlib.sha256(payload.encode("utf-8")).hexdigest()

    def load(self, key: str) -> LLMGenerationRecord | None:
        record_path = self.path / f"{key}.json"
        if not record_path.exists():
            return None
        return LLMGenerationRecord.from_json_dict(json.loads(record_path.read_text()))

    def store(self, record: LLMGenerationRecord) -> None:
        self.path.mkdir(parents=True, exist_ok=True)
        record_path = self.path / f"{record.cache_key}.json"
        record_path.write_text(
            json.dumps(record.to_json_dict(), indent=2, sort_keys=True)
        )


class LLMMessageGenerator:
    provider = "base"
    model = "base"

    def generate(
        self, request: LLMMessageRequest, cache_key: str
    ) -> GeneratedEvacuationMessage:
        raise NotImplementedError


class ReplayOnlyGenerator(LLMMessageGenerator):
    provider = "cache"
    model = "replay_only"

    def __init__(self, cache: LLMMessageCache) -> None:
        self.cache = cache

    def generate(
        self, request: LLMMessageRequest, cache_key: str
    ) -> GeneratedEvacuationMessage:
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
    """Deterministic stand-in used for tests and replay-safe dry runs."""

    provider = "deterministic"
    model = "template"

    def generate(
        self, request: LLMMessageRequest, cache_key: str
    ) -> GeneratedEvacuationMessage:
        recommended = [
            exit_ for exit_ in request.exits if exit_ not in request.congested_exits
        ]
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


class OpenAIResponsesGenerator(LLMMessageGenerator):
    """Live OpenAI Responses API generator with a structured JSON prompt."""

    provider = "openai"

    def __init__(
        self,
        model: str | None = None,
        *,
        api_key: str | None = None,
        timeout_s: float = 30.0,
        endpoint: str = "https://api.openai.com/v1/responses",
    ) -> None:
        self.model = model or load_openai_model()
        self.api_key = api_key or load_openai_api_key()
        self.timeout_s = float(timeout_s)
        self.endpoint = endpoint

    def generate(
        self, request: LLMMessageRequest, cache_key: str
    ) -> GeneratedEvacuationMessage:
        if not self.api_key:
            return GeneratedEvacuationMessage(
                text="OpenAI API key is not configured.",
                abstain=True,
                provider=self.provider,
                model=self.model,
                raw_response={"error": "missing_api_key"},
            )

        payload = {
            "model": self.model,
            "store": False,
            "max_output_tokens": 500,
            "instructions": build_prompt_instructions(request.prompt_style),
            "input": json.dumps(_prompt_payload(request), sort_keys=True),
        }
        api_request = urlrequest.Request(
            self.endpoint,
            data=json.dumps(payload).encode("utf-8"),
            headers={
                "Authorization": f"Bearer {self.api_key}",
                "Content-Type": "application/json",
            },
            method="POST",
        )
        try:
            with urlrequest.urlopen(api_request, timeout=self.timeout_s) as response:
                response_payload = json.loads(response.read().decode("utf-8"))
        except HTTPError as exc:
            return self._error_message(f"http_{exc.code}")
        except (URLError, TimeoutError) as exc:
            return self._error_message(type(exc).__name__)

        text = _extract_response_text(response_payload)
        parsed = _parse_json_object(text)
        if parsed is None:
            return GeneratedEvacuationMessage(
                text=text,
                abstain=True,
                provider=self.provider,
                model=self.model,
                raw_response={"unparsed_response": response_payload},
            )

        return GeneratedEvacuationMessage(
            message_type="route_guidance",
            text=str(parsed.get("text", "")),
            recommended_exits=_parse_cells(parsed.get("recommended_exits", [])),
            avoid_exits=_parse_cells(parsed.get("avoid_exits", [])),
            hazard_positions=_parse_points(parsed.get("hazard_positions", [])),
            confidence=float(parsed.get("confidence", 0.0) or 0.0),
            abstain=bool(parsed.get("abstain", False)),
            provider=self.provider,
            model=self.model,
            raw_response={
                "id": response_payload.get("id"),
                "usage": response_payload.get("usage"),
            },
        )

    def _error_message(self, error: str) -> GeneratedEvacuationMessage:
        return GeneratedEvacuationMessage(
            text="OpenAI generation failed; abstaining.",
            abstain=True,
            provider=self.provider,
            model=self.model,
            raw_response={"error": error},
        )


class AnthropicMessagesGenerator(LLMMessageGenerator):
    provider = "anthropic"

    def __init__(
        self,
        model: str | None = None,
        *,
        api_key: str | None = None,
        timeout_s: float = 30.0,
        endpoint: str = "https://api.anthropic.com/v1/messages",
        api_version: str = DEFAULT_ANTHROPIC_VERSION,
    ) -> None:
        self.model = model or load_anthropic_model()
        self.api_key = api_key or load_anthropic_api_key()
        self.timeout_s = float(timeout_s)
        self.endpoint = endpoint
        self.api_version = api_version

    def generate(
        self, request: LLMMessageRequest, cache_key: str
    ) -> GeneratedEvacuationMessage:
        if not self.api_key:
            return GeneratedEvacuationMessage(
                text="Anthropic API key is not configured.",
                abstain=True,
                provider=self.provider,
                model=self.model,
                raw_response={"error": "missing_api_key"},
            )

        payload = {
            "model": self.model,
            "max_tokens": 500,
            "system": build_prompt_instructions(request.prompt_style),
            "messages": [
                {
                    "role": "user",
                    "content": json.dumps(_prompt_payload(request), sort_keys=True),
                }
            ],
        }
        api_request = urlrequest.Request(
            self.endpoint,
            data=json.dumps(payload).encode("utf-8"),
            headers={
                "x-api-key": self.api_key,
                "anthropic-version": self.api_version,
                "content-type": "application/json",
            },
            method="POST",
        )
        try:
            with urlrequest.urlopen(api_request, timeout=self.timeout_s) as response:
                response_payload = json.loads(response.read().decode("utf-8"))
        except HTTPError as exc:
            return self._error_message(f"http_{exc.code}")
        except (URLError, TimeoutError) as exc:
            return self._error_message(type(exc).__name__)

        text = _extract_anthropic_text(response_payload)
        parsed = _parse_json_object(text)
        if parsed is None:
            return GeneratedEvacuationMessage(
                text=text,
                abstain=True,
                provider=self.provider,
                model=self.model,
                raw_response={"unparsed_response": response_payload},
            )
        return GeneratedEvacuationMessage(
            message_type="route_guidance",
            text=str(parsed.get("text", "")),
            recommended_exits=_parse_cells(parsed.get("recommended_exits", [])),
            avoid_exits=_parse_cells(parsed.get("avoid_exits", [])),
            hazard_positions=_parse_points(parsed.get("hazard_positions", [])),
            confidence=float(parsed.get("confidence", 0.0) or 0.0),
            abstain=bool(parsed.get("abstain", False)),
            provider=self.provider,
            model=self.model,
            raw_response={
                "id": response_payload.get("id"),
                "usage": response_payload.get("usage"),
            },
        )

    def _error_message(self, error: str) -> GeneratedEvacuationMessage:
        return GeneratedEvacuationMessage(
            text="Anthropic generation failed; abstaining.",
            abstain=True,
            provider=self.provider,
            model=self.model,
            raw_response={"error": error},
        )


def validate_generated_message(
    message: GeneratedEvacuationMessage,
    *,
    known_exits: Sequence[Cell],
    known_hazards: Sequence[HazardSnapshot],
    base_radius: float,
    max_radius: float,
    base_credibility: float,
    congested_exits: Sequence[Cell] = (),
    settings: ValidatorSettings | None = None,
) -> ValidationResult:
    settings = settings or ValidatorSettings()
    reasons: list[str] = []
    known_exit_set = {tuple(exit_) for exit_ in known_exits}
    recommended_set = {tuple(exit_) for exit_ in message.recommended_exits}
    avoid_set = {tuple(exit_) for exit_ in message.avoid_exits}
    congested_set = {tuple(exit_) for exit_ in congested_exits}

    if message.abstain:
        reasons.append("generator_abstained")
    for exit_ in message.recommended_exits:
        if tuple(exit_) not in known_exit_set:
            reasons.append(f"invented_exit:{tuple(exit_)}")
    for exit_ in message.avoid_exits:
        if tuple(exit_) not in known_exit_set:
            reasons.append(f"invented_avoid_exit:{tuple(exit_)}")
    for exit_ in sorted(recommended_set & avoid_set):
        reasons.append(f"conflicting_exit:{exit_}")
    if settings.reject_congested_recommendations:
        for exit_ in sorted(recommended_set & congested_set):
            reasons.append(f"congested_recommendation:{exit_}")

    known_hazard_positions = [hazard.position for hazard in known_hazards]
    for hazard_pos in message.hazard_positions:
        if not _near_any(hazard_pos, known_hazard_positions, tolerance=3.0):
            reasons.append(f"invented_hazard:{tuple(hazard_pos)}")

    if message.radius is not None and (
        message.radius <= 0.0 or message.radius > max_radius
    ):
        reasons.append(f"unsafe_radius:{message.radius}")
    if (
        message.credibility is not None
        and not 0.0 <= message.credibility <= base_credibility
    ):
        reasons.append(f"unsafe_credibility:{message.credibility}")

    if not message.recommended_exits and not message.abstain:
        reasons.append("no_recommended_exit")
    if not message.text.strip() and not message.abstain:
        reasons.append("empty_guidance")
    if (
        settings.reject_vague_guidance
        and _is_vague_guidance(message.text)
        and not message.abstain
    ):
        reasons.append("vague_guidance")
    reasons.extend(attack_pattern_reasons(message.text))
    if (
        settings.reject_low_confidence
        and message.confidence < settings.min_confidence
        and not message.abstain
    ):
        reasons.append(f"low_confidence:{message.confidence:.2f}")

    return ValidationResult(accepted=not reasons, reasons=reasons)


def attack_pattern_reasons(text: str) -> list[str]:
    lowered = (text or "").lower()
    reasons = []
    seen = set()
    for code, marker in ATTACK_TEXT_MARKERS:
        if marker in lowered and code not in seen:
            reasons.append(f"attack_pattern:{code}")
            seen.add(code)
    return reasons


def with_llm_audit_chain(frame) -> Any:
    result = frame.copy()
    result = result.drop(
        columns=[
            column
            for column in LLM_AUDIT_CHAIN_COLUMNS
            if column in result.columns
        ]
    )
    previous = LLM_AUDIT_GENESIS
    previous_hashes: list[str] = []
    current_hashes: list[str] = []
    for record in result.to_dict(orient="records"):
        current = _llm_audit_row_hash(previous, record)
        previous_hashes.append(previous)
        current_hashes.append(current)
        previous = current
    result[LLM_AUDIT_CHAIN_PREV_HASH] = previous_hashes
    result[LLM_AUDIT_CHAIN_HASH] = current_hashes
    return result


def verify_llm_audit_chain(frame) -> LLMAuditVerification:
    row_count = int(len(frame))
    missing = [
        column for column in LLM_AUDIT_CHAIN_COLUMNS if column not in frame.columns
    ]
    if missing:
        return LLMAuditVerification(
            ok=row_count == 0,
            row_count=row_count,
            reason="missing_chain_columns",
            expected=",".join(missing),
        )
    previous = LLM_AUDIT_GENESIS
    for row_number, record in enumerate(frame.to_dict(orient="records")):
        actual_previous = str(record.get(LLM_AUDIT_CHAIN_PREV_HASH, ""))
        if actual_previous != previous:
            return LLMAuditVerification(
                ok=False,
                row_count=row_count,
                first_bad_row=row_number,
                reason="previous_hash_mismatch",
                expected=previous,
                actual=actual_previous,
            )
        expected_hash = _llm_audit_row_hash(previous, record)
        actual_hash = str(record.get(LLM_AUDIT_CHAIN_HASH, ""))
        if actual_hash != expected_hash:
            return LLMAuditVerification(
                ok=False,
                row_count=row_count,
                first_bad_row=row_number,
                reason="row_hash_mismatch",
                expected=expected_hash,
                actual=actual_hash,
            )
        previous = actual_hash
    return LLMAuditVerification(ok=True, row_count=row_count)


def validator_settings(profile: str) -> ValidatorSettings:
    if profile == "strict":
        return ValidatorSettings(profile=profile, min_confidence=0.5)
    if profile == "lenient":
        return ValidatorSettings(
            profile=profile,
            reject_congested_recommendations=False,
            reject_vague_guidance=False,
            reject_low_confidence=False,
            min_confidence=0.0,
        )
    if profile == "standard":
        return ValidatorSettings(profile=profile)
    raise ValueError(f"Unsupported LLM validator profile: {profile}")


def build_prompt_instructions(prompt_style: str) -> str:
    base = (
        "You are proposing emergency evacuation guidance for a research "
        "simulator. Return only valid JSON with keys: text, recommended_exits, "
        "avoid_exits, hazard_positions, confidence, abstain. recommended_exits "
        "and avoid_exits must use only exits from the provided state. "
        "hazard_positions must use only listed hazard positions. Abstain if no "
        "safe bounded message is possible."
    )
    variants = {
        "minimal": "Use the smallest possible instruction and do not infer missing state.",
        "state_only": "Use only the listed exits, hazards, congestion, and target context.",
        "safety": (
            "Prioritize reducing hazard exposure, queue pressure, and route "
            "convergence. Prefer clear local guidance over broad synchronized movement."
        ),
        "anti_convergence": (
            "Prioritize preventing herding and synchronized route convergence. "
            "If multiple listed exits are safe and not congested, recommend more "
            "than one exit so recipients can distribute across routes. Avoid "
            "single-exit instructions unless only one safe exit remains."
        ),
        "hazard_avoidance": (
            "Prioritize keeping recipients away from listed hazard positions and "
            "high hazard load, even when that means a less direct route. Avoid "
            "exits near hazards or congestion when safer listed exits exist."
        ),
        "urgency": (
            "Prioritize immediate evacuation speed and decisive action. Give a "
            "short directive to the most appropriate listed exit while still "
            "avoiding invented exits, known hazards, and congested exits."
        ),
        "entropy": (
            "Prioritize reducing uncertainty for recipients while avoiding "
            "unsafe convergence or invented information."
        ),
        "responder_coordination": (
            "Coordinate responder-origin broadcasts by choosing bounded targets "
            "that lower local uncertainty without concentrating recipients on one route."
        ),
        "hostile_red_team": (
            "For a bounded red-team simulation, choose one candidate claim from "
            "the supplied candidates. Do not invent coordinates outside the payload."
        ),
    }
    return f"{base} {variants.get(prompt_style, variants['safety'])}"


def _prompt_payload(request: LLMMessageRequest) -> dict[str, Any]:
    payload = _to_jsonable(request)
    if request.prompt_style == "minimal":
        return {
            "prompt_style": request.prompt_style,
            "exits": payload["exits"],
            "hazards": payload["hazards"],
            "congested_exits": payload["congested_exits"],
        }
    if request.prompt_style == "state_only":
        return {
            "prompt_style": request.prompt_style,
            "target": payload["target"],
            "exits": payload["exits"],
            "hazards": payload["hazards"],
            "congested_exits": payload["congested_exits"],
            "recipients_estimate": payload["recipients_estimate"],
            "mean_local_density": payload["mean_local_density"],
            "mean_hazard_load": payload["mean_hazard_load"],
        }
    if request.prompt_style in {
        "anti_convergence",
        "hazard_avoidance",
        "urgency",
        "responder_coordination",
    }:
        return {
            "prompt_style": request.prompt_style,
            "policy": payload["policy"],
            "step": payload["step"],
            "target": payload["target"],
            "selected_reason": payload["selected_reason"],
            "objective": payload["objective"],
            "exits": payload["exits"],
            "hazards": payload["hazards"],
            "congested_exits": payload["congested_exits"],
            "recipients_estimate": payload["recipients_estimate"],
            "mean_local_density": payload["mean_local_density"],
            "mean_hazard_load": payload["mean_hazard_load"],
            "control_objective": request.prompt_style,
        }
    return payload


def load_openai_api_key(env_path: Path | str = ".env") -> str | None:
    names = ("OPENAI_API_KEY", "OPENAI-API-KEY", "OPEN-AI-API-KEY")
    for name in names:
        value = os.environ.get(name)
        if value:
            return value

    return _load_env_file_value(env_path, names)


def load_openai_model(
    env_path: Path | str = ".env",
    default: str = DEFAULT_OPENAI_MODEL,
) -> str:
    value = os.environ.get("OPENAI_MODEL")
    if value:
        return value

    return _load_env_file_value(env_path, ("OPENAI_MODEL",)) or default


def load_anthropic_api_key(env_path: Path | str = ".env") -> str | None:
    names = ("ANTHROPIC_API_KEY", "ANTHROPIC-API-KEY")
    for name in names:
        value = os.environ.get(name)
        if value:
            return value
    return _load_env_file_value(env_path, names)


def load_anthropic_model(
    env_path: Path | str = ".env",
    default: str = DEFAULT_ANTHROPIC_MODEL,
) -> str:
    value = os.environ.get("ANTHROPIC_MODEL")
    if value:
        return value
    return _load_env_file_value(env_path, ("ANTHROPIC_MODEL",)) or default


def _load_env_file_value(env_path: Path | str, names: Sequence[str]) -> str | None:
    path = Path(env_path)
    if not path.exists():
        return None
    for line in path.read_text().splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or "=" not in stripped:
            continue
        key, value = stripped.split("=", 1)
        key = key.strip()
        if key.startswith("export "):
            key = key.removeprefix("export ").strip()
        if key in names:
            return value.strip().strip('"').strip("'")
    return None


def build_llm_request(
    simulation, target, objective: str, policy: str
) -> LLMMessageRequest:
    active = [
        agent
        for agent in simulation._active_agents()
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
                position=_point3(hazard.pos),
                kind=str(hazard.kind),
                radius=float(hazard.radius),
                severity=float(hazard.severity),
            )
            for hazard in simulation.hazards
        ],
        congested_exits=[],
        recipients_estimate=len(active),
        mean_local_density=_mean(
            [float(getattr(agent, "local_density", 0.0)) for agent in active]
        ),
        mean_hazard_load=_mean(
            [float(getattr(agent, "current_hazard_load", 0.0)) for agent in active]
        ),
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


def _llm_audit_row_hash(previous_hash: str, record: dict[str, Any]) -> str:
    payload = {
        str(key): _canonical_llm_audit_value(value)
        for key, value in record.items()
        if key not in LLM_AUDIT_CHAIN_COLUMNS
    }
    text = json.dumps(
        {"previous_hash": previous_hash, "row": payload},
        sort_keys=True,
        separators=(",", ":"),
    )
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def _canonical_llm_audit_value(value: Any) -> Any:
    if value is None:
        return None
    if isinstance(value, bool):
        return value
    if hasattr(value, "item"):
        try:
            return _canonical_llm_audit_value(value.item())
        except (AttributeError, TypeError, ValueError):
            pass
    if isinstance(value, int):
        return int(value)
    if isinstance(value, float):
        if value != value:
            return None
        if value.is_integer():
            return int(value)
        return float(value)
    if isinstance(value, tuple):
        return [_canonical_llm_audit_value(item) for item in value]
    if isinstance(value, list):
        return [_canonical_llm_audit_value(item) for item in value]
    if isinstance(value, dict):
        return {
            str(key): _canonical_llm_audit_value(item)
            for key, item in value.items()
        }
    return str(value) if value.__class__.__module__.startswith("pandas") else value


def _extract_response_text(payload: dict[str, Any]) -> str:
    if isinstance(payload.get("output_text"), str):
        return payload["output_text"]
    chunks: list[str] = []
    for item in payload.get("output", []):
        for content in item.get("content", []):
            if isinstance(content.get("text"), str):
                chunks.append(content["text"])
    return "\n".join(chunks)


def _extract_anthropic_text(payload: dict[str, Any]) -> str:
    chunks: list[str] = []
    for item in payload.get("content", []):
        if isinstance(item.get("text"), str):
            chunks.append(item["text"])
    return "\n".join(chunks)


def estimate_llm_tokens(value: Any, *, output_tokens: int = 0) -> int:
    try:
        text = json.dumps(_to_jsonable(value), sort_keys=True, separators=(",", ":"))
    except TypeError:
        text = str(value)
    return max(1, (len(text) + 3) // 4) + int(output_tokens)


def estimate_llm_cost(
    *,
    input_tokens: int,
    output_tokens: int,
    input_usd_per_mtok: float = 0.0,
    output_usd_per_mtok: float = 0.0,
) -> float:
    return (float(input_tokens) / 1_000_000.0) * float(input_usd_per_mtok) + (
        float(output_tokens) / 1_000_000.0
    ) * float(output_usd_per_mtok)


def raw_usage_tokens(raw_response: dict[str, Any]) -> dict[str, int]:
    usage = raw_response.get("usage") if isinstance(raw_response, dict) else {}
    usage = usage if isinstance(usage, dict) else {}
    input_tokens = int(
        usage.get("input_tokens")
        or usage.get("prompt_tokens")
        or usage.get("input_token_count")
        or 0
    )
    output_tokens = int(
        usage.get("output_tokens")
        or usage.get("completion_tokens")
        or usage.get("output_token_count")
        or 0
    )
    total_tokens = int(usage.get("total_tokens") or input_tokens + output_tokens)
    return {
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "total_tokens": total_tokens,
    }


def _parse_json_object(text: str) -> dict[str, Any] | None:
    stripped = text.strip()
    if stripped.startswith("```"):
        stripped = stripped.strip("`")
        if stripped.startswith("json"):
            stripped = stripped[4:].strip()
    try:
        payload = json.loads(stripped)
    except json.JSONDecodeError:
        return None
    return payload if isinstance(payload, dict) else None


def _parse_cells(value: Any) -> list[Cell]:
    cells: list[Cell] = []
    if not isinstance(value, list):
        return cells
    for item in value:
        if isinstance(item, dict):
            if item.get("floor") is not None:
                item = [item.get("floor"), item.get("x"), item.get("y")]
            else:
                item = [item.get("x"), item.get("y")]
        if (
            isinstance(item, (list, tuple))
            and len(item) >= 3
            and isinstance(item[0], str)
        ):
            try:
                cells.append((str(item[0]), int(item[1]), int(item[2])))
            except (TypeError, ValueError):
                continue
            continue
        if isinstance(item, (list, tuple)) and len(item) >= 2:
            try:
                cells.append((int(item[0]), int(item[1])))
            except (TypeError, ValueError):
                continue
    return cells


def _parse_points(value: Any) -> list[Point]:
    points: list[Point] = []
    if not isinstance(value, list):
        return points
    for item in value:
        if isinstance(item, dict):
            item = [item.get("x"), item.get("y"), item.get("z", 0.0)]
        if isinstance(item, (list, tuple)) and len(item) >= 2:
            try:
                z = float(item[2]) if len(item) >= 3 else 0.0
                points.append((float(item[0]), float(item[1]), z))
            except (TypeError, ValueError):
                continue
    return points


def _is_vague_guidance(text: str) -> bool:
    normalized = " ".join(text.lower().split())
    if not normalized:
        return False
    vague_messages = {
        "stay calm",
        "proceed calmly",
        "evacuate safely",
        "follow instructions",
        "move to safety",
    }
    if normalized in vague_messages:
        return True
    directional_terms = (
        "exit",
        "avoid",
        "left",
        "right",
        "north",
        "south",
        "east",
        "west",
    )
    return len(normalized.split()) < 6 and not any(
        term in normalized for term in directional_terms
    )


def _near_any(point: Point, candidates: Sequence[Point], tolerance: float) -> bool:
    return any(_distance(point, candidate) <= tolerance for candidate in candidates)


def _distance(a: Point, b: Point) -> float:
    ax, ay, az = _point3(a)
    bx, by, bz = _point3(b)
    return ((ax - bx) ** 2 + (ay - by) ** 2 + (az - bz) ** 2) ** 0.5


def _point3(value: Any) -> tuple[float, float, float]:
    if len(value) >= 3:
        return (float(value[0]), float(value[1]), float(value[2]))
    return (float(value[0]), float(value[1]), 0.0)


def _mean(values: Sequence[float]) -> float:
    if not values:
        return 0.0
    return sum(values) / len(values)
