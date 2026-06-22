from __future__ import annotations

from dataclasses import asdict, dataclass, field
import hashlib
import json
from pathlib import Path
from urllib import request as urlrequest
from urllib.error import HTTPError, URLError
from typing import Any, Optional, Sequence, Tuple

from chiyoda.agents.base import INTENTION_EVACUATE, INTENTION_EXPLORE, INTENTION_FOLLOW
from chiyoda.information.entropy import agent_entropy
from chiyoda.information.llm import (
    LLMBudgetGuard,
    HazardSnapshot,
    ValidationResult,
    _extract_anthropic_text,
    _extract_response_text,
    _parse_json_object,
    estimate_llm_cost,
    estimate_llm_tokens,
    load_anthropic_api_key,
    load_anthropic_model,
    load_openai_api_key,
    load_openai_model,
    raw_usage_tokens,
    validator_settings,
)


Cell = tuple
Point = tuple
ALLOWED_INTENTS = {INTENTION_EVACUATE, INTENTION_EXPLORE, INTENTION_FOLLOW}


@dataclass(frozen=True)
class LLMDecisionRequest:
    step: int
    agent_id: int
    current_intent: str
    objective: str
    known_exits: Sequence[Cell]
    congested_exits: Sequence[Cell]
    hazards: Sequence[HazardSnapshot]
    local_density: float
    hazard_load: float
    entropy: float
    prompt_style: str = "bounded"


@dataclass
class GeneratedAgentDecision:
    intent: str = INTENTION_EVACUATE
    target_exit: Optional[Cell] = None
    trust_delta: float = 0.0
    avoid_congested: bool = True
    rationale: str = ""
    confidence: float = 0.0
    abstain: bool = False
    provider: str = "deterministic"
    model: str = "template"
    raw_response: dict[str, Any] = field(default_factory=dict)


@dataclass
class LLMDecisionRecord:
    cache_key: str
    request: LLMDecisionRequest
    decision: GeneratedAgentDecision
    validation: ValidationResult

    def to_json_dict(self) -> dict[str, Any]:
        return {
            "cache_key": self.cache_key,
            "request": _to_jsonable(self.request),
            "decision": _to_jsonable(self.decision),
            "validation": _to_jsonable(self.validation),
        }

    @classmethod
    def from_json_dict(cls, payload: dict[str, Any]) -> "LLMDecisionRecord":
        req = payload["request"]
        dec = payload["decision"]
        val = payload["validation"]
        return cls(
            cache_key=str(payload["cache_key"]),
            request=LLMDecisionRequest(
                step=int(req["step"]),
                agent_id=int(req["agent_id"]),
                current_intent=str(req["current_intent"]),
                objective=str(req["objective"]),
                known_exits=[tuple(item) for item in req.get("known_exits", [])],
                congested_exits=[
                    tuple(item) for item in req.get("congested_exits", [])
                ],
                hazards=[
                    HazardSnapshot(
                        position=tuple(item["position"]),
                        kind=str(item["kind"]),
                        radius=float(item["radius"]),
                        severity=float(item["severity"]),
                    )
                    for item in req.get("hazards", [])
                ],
                local_density=float(req.get("local_density", 0.0)),
                hazard_load=float(req.get("hazard_load", 0.0)),
                entropy=float(req.get("entropy", 0.0)),
                prompt_style=str(req.get("prompt_style", "bounded")),
            ),
            decision=GeneratedAgentDecision(
                intent=str(dec.get("intent", INTENTION_EVACUATE)),
                target_exit=(
                    None
                    if dec.get("target_exit") is None
                    else tuple(dec["target_exit"])
                ),
                trust_delta=float(dec.get("trust_delta", 0.0) or 0.0),
                avoid_congested=bool(dec.get("avoid_congested", True)),
                rationale=str(dec.get("rationale", "")),
                confidence=float(dec.get("confidence", 0.0) or 0.0),
                abstain=bool(dec.get("abstain", False)),
                provider=str(dec.get("provider", "deterministic")),
                model=str(dec.get("model", "template")),
                raw_response=dict(dec.get("raw_response", {})),
            ),
            validation=ValidationResult(
                accepted=bool(val["accepted"]),
                reasons=[str(item) for item in val.get("reasons", [])],
            ),
        )


@dataclass
class LLMDecisionEvent:
    step: int
    time_s: float
    agent_id: int
    provider: str
    model: str
    cache_key: str
    cache_status: str
    validation_status: str
    validation_reasons: str
    selected_intent: str
    target_exit_floor: Optional[str]
    target_exit_x: Optional[int]
    target_exit_y: Optional[int]
    trust_delta: float
    avoid_congested: bool
    confidence: float
    rationale: str
    used_fallback: bool
    objective: str


@dataclass
class AgentDecisionConfig:
    enabled: bool = False
    provider: str = "template"
    model: str = "template"
    cache_path: Optional[str] = None
    cache_mode: str = "cache_first"
    store_cache: bool = True
    start_step: int = 0
    end_step: Optional[int] = None
    interval_steps: int = 20
    agent_budget_per_interval: int = 4
    objective: str = "bounded_agent_decision"
    prompt_style: str = "bounded"
    validator_profile: str = "standard"
    max_trust_delta: float = 0.2
    max_calls_per_run: Optional[int] = None
    max_estimated_tokens_per_run: Optional[int] = None
    max_estimated_usd_per_run: Optional[float] = None
    input_usd_per_mtok: float = 0.0
    output_usd_per_mtok: float = 0.0

    @classmethod
    def from_mapping(cls, payload: Optional[dict[str, Any]]) -> "AgentDecisionConfig":
        data = dict(payload or {})
        return cls(
            enabled=bool(data.get("enabled", False)),
            provider=str(data.get("provider", "template")),
            model=str(data.get("model", "template")),
            cache_path=(
                None if data.get("cache_path") is None else str(data["cache_path"])
            ),
            cache_mode=str(data.get("cache_mode", "cache_first")),
            store_cache=bool(data.get("store_cache", True)),
            start_step=int(data.get("start_step", 0)),
            end_step=None if data.get("end_step") is None else int(data["end_step"]),
            interval_steps=max(1, int(data.get("interval_steps", 20))),
            agent_budget_per_interval=max(
                1, int(data.get("agent_budget_per_interval", 4))
            ),
            objective=str(data.get("objective", "bounded_agent_decision")),
            prompt_style=str(data.get("prompt_style", "bounded")),
            validator_profile=str(data.get("validator_profile", "standard")),
            max_trust_delta=float(data.get("max_trust_delta", 0.2)),
            max_calls_per_run=(
                None
                if data.get("max_calls_per_run") is None
                else int(data["max_calls_per_run"])
            ),
            max_estimated_tokens_per_run=(
                None
                if data.get("max_estimated_tokens_per_run") is None
                else int(data["max_estimated_tokens_per_run"])
            ),
            max_estimated_usd_per_run=(
                None
                if data.get("max_estimated_usd_per_run") is None
                else float(data["max_estimated_usd_per_run"])
            ),
            input_usd_per_mtok=float(data.get("input_usd_per_mtok", 0.0)),
            output_usd_per_mtok=float(data.get("output_usd_per_mtok", 0.0)),
        )


class LLMDecisionCache:
    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)

    def key_for(self, request: LLMDecisionRequest) -> str:
        payload = json.dumps(
            _to_jsonable(request), sort_keys=True, separators=(",", ":")
        )
        return hashlib.sha256(payload.encode("utf-8")).hexdigest()

    def load(self, key: str) -> Optional[LLMDecisionRecord]:
        record_path = self.path / f"{key}.json"
        if not record_path.exists():
            return None
        return LLMDecisionRecord.from_json_dict(json.loads(record_path.read_text()))

    def store(self, record: LLMDecisionRecord) -> None:
        self.path.mkdir(parents=True, exist_ok=True)
        (self.path / f"{record.cache_key}.json").write_text(
            json.dumps(record.to_json_dict(), indent=2, sort_keys=True) + "\n"
        )


class LLMDecisionGenerator:
    provider = "base"
    model = "base"

    def generate(
        self, request: LLMDecisionRequest, cache_key: str
    ) -> GeneratedAgentDecision:
        raise NotImplementedError


class TemplateDecisionGenerator(LLMDecisionGenerator):
    provider = "deterministic"
    model = "template"

    def generate(
        self, request: LLMDecisionRequest, cache_key: str
    ) -> GeneratedAgentDecision:
        exits = [
            exit_
            for exit_ in request.known_exits
            if exit_ not in request.congested_exits
        ]
        if request.hazard_load > 0.05 and exits:
            return GeneratedAgentDecision(
                intent=INTENTION_EVACUATE,
                target_exit=exits[0],
                trust_delta=0.05,
                rationale="hazard_load_requires_bounded_evacuation",
                confidence=0.75,
                provider=self.provider,
                model=self.model,
            )
        if request.local_density > 0.6:
            return GeneratedAgentDecision(
                intent=INTENTION_FOLLOW,
                target_exit=exits[0] if exits else None,
                rationale="local_density_requires_conservative_following",
                confidence=0.65,
                provider=self.provider,
                model=self.model,
            )
        if exits:
            return GeneratedAgentDecision(
                intent=INTENTION_EVACUATE,
                target_exit=exits[0],
                rationale="known_exit_available",
                confidence=0.7,
                provider=self.provider,
                model=self.model,
            )
        return GeneratedAgentDecision(
            intent=INTENTION_EXPLORE,
            rationale="no_known_exit_available",
            confidence=0.6,
            provider=self.provider,
            model=self.model,
        )


class ReplayDecisionGenerator(LLMDecisionGenerator):
    provider = "cache"
    model = "replay_only"

    def __init__(self, cache: LLMDecisionCache) -> None:
        self.cache = cache

    def generate(
        self, request: LLMDecisionRequest, cache_key: str
    ) -> GeneratedAgentDecision:
        record = self.cache.load(cache_key)
        if record is None:
            return GeneratedAgentDecision(
                intent=request.current_intent,
                abstain=True,
                rationale="missing_replay_record",
                provider=self.provider,
                model=self.model,
            )
        return record.decision


class OpenAIDecisionGenerator(LLMDecisionGenerator):
    provider = "openai"

    def __init__(
        self,
        model: Optional[str] = None,
        *,
        api_key: Optional[str] = None,
        timeout_s: float = 30.0,
        endpoint: str = "https://api.openai.com/v1/responses",
    ) -> None:
        self.model = model or load_openai_model()
        self.api_key = api_key or load_openai_api_key()
        self.timeout_s = float(timeout_s)
        self.endpoint = endpoint

    def generate(
        self, request: LLMDecisionRequest, cache_key: str
    ) -> GeneratedAgentDecision:
        if not self.api_key:
            return GeneratedAgentDecision(
                intent=request.current_intent,
                abstain=True,
                rationale="missing_api_key",
                provider=self.provider,
                model=self.model,
                raw_response={"error": "missing_api_key"},
            )
        payload = {
            "model": self.model,
            "store": False,
            "max_output_tokens": 300,
            "instructions": _decision_instructions(request.prompt_style),
            "input": json.dumps(_to_jsonable(request), sort_keys=True),
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
            return self._error_decision(request, f"http_{exc.code}")
        except (URLError, TimeoutError) as exc:
            return self._error_decision(request, type(exc).__name__)

        parsed = _parse_json_object(_extract_response_text(response_payload))
        if parsed is None:
            return self._error_decision(request, "unparsed_response", response_payload)
        return GeneratedAgentDecision(
            intent=str(parsed.get("intent", request.current_intent)),
            target_exit=_parse_optional_cell(parsed.get("target_exit")),
            trust_delta=float(parsed.get("trust_delta", 0.0) or 0.0),
            avoid_congested=bool(parsed.get("avoid_congested", True)),
            rationale=str(parsed.get("rationale", "")),
            confidence=float(parsed.get("confidence", 0.0) or 0.0),
            abstain=bool(parsed.get("abstain", False)),
            provider=self.provider,
            model=self.model,
            raw_response={
                "id": response_payload.get("id"),
                "usage": response_payload.get("usage"),
            },
        )

    def _error_decision(
        self,
        request: LLMDecisionRequest,
        error: str,
        payload: Optional[dict[str, Any]] = None,
    ) -> GeneratedAgentDecision:
        raw = {"error": error}
        if payload is not None:
            raw["response"] = payload
        return GeneratedAgentDecision(
            intent=request.current_intent,
            abstain=True,
            rationale=error,
            provider=self.provider,
            model=self.model,
            raw_response=raw,
        )


class AnthropicDecisionGenerator(LLMDecisionGenerator):
    provider = "anthropic"

    def __init__(
        self,
        model: Optional[str] = None,
        *,
        api_key: Optional[str] = None,
        timeout_s: float = 30.0,
        endpoint: str = "https://api.anthropic.com/v1/messages",
        api_version: str = "2023-06-01",
    ) -> None:
        self.model = model or load_anthropic_model()
        self.api_key = api_key or load_anthropic_api_key()
        self.timeout_s = float(timeout_s)
        self.endpoint = endpoint
        self.api_version = api_version

    def generate(
        self, request: LLMDecisionRequest, cache_key: str
    ) -> GeneratedAgentDecision:
        if not self.api_key:
            return GeneratedAgentDecision(
                intent=request.current_intent,
                abstain=True,
                rationale="missing_api_key",
                provider=self.provider,
                model=self.model,
                raw_response={"error": "missing_api_key"},
            )
        payload = {
            "model": self.model,
            "max_tokens": 300,
            "system": _decision_instructions(request.prompt_style),
            "messages": [
                {
                    "role": "user",
                    "content": json.dumps(_to_jsonable(request), sort_keys=True),
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
            return self._error_decision(request, f"http_{exc.code}")
        except (URLError, TimeoutError) as exc:
            return self._error_decision(request, type(exc).__name__)

        parsed = _parse_json_object(_extract_anthropic_text(response_payload))
        if parsed is None:
            return self._error_decision(request, "unparsed_response", response_payload)
        return GeneratedAgentDecision(
            intent=str(parsed.get("intent", request.current_intent)),
            target_exit=_parse_optional_cell(parsed.get("target_exit")),
            trust_delta=float(parsed.get("trust_delta", 0.0) or 0.0),
            avoid_congested=bool(parsed.get("avoid_congested", True)),
            rationale=str(parsed.get("rationale", "")),
            confidence=float(parsed.get("confidence", 0.0) or 0.0),
            abstain=bool(parsed.get("abstain", False)),
            provider=self.provider,
            model=self.model,
            raw_response={
                "id": response_payload.get("id"),
                "usage": response_payload.get("usage"),
            },
        )

    def _error_decision(
        self,
        request: LLMDecisionRequest,
        error: str,
        payload: Optional[dict[str, Any]] = None,
    ) -> GeneratedAgentDecision:
        raw = {"error": error}
        if payload is not None:
            raw["response"] = payload
        return GeneratedAgentDecision(
            intent=request.current_intent,
            abstain=True,
            rationale=error,
            provider=self.provider,
            model=self.model,
            raw_response=raw,
        )


class AgentDecisionPolicy:
    def __init__(self, config: AgentDecisionConfig) -> None:
        self.config = config
        self.cache = LLMDecisionCache(config.cache_path) if config.cache_path else None
        provider = config.provider.lower()
        config.provider = provider
        self.budget_guard = LLMBudgetGuard(
            max_calls=config.max_calls_per_run,
            max_estimated_tokens=config.max_estimated_tokens_per_run,
            max_estimated_usd=config.max_estimated_usd_per_run,
            input_usd_per_mtok=config.input_usd_per_mtok,
            output_usd_per_mtok=config.output_usd_per_mtok,
        )
        if provider == "template":
            self.generator = TemplateDecisionGenerator()
        elif provider in {"replay", "local_replay"}:
            if self.cache is None:
                raise ValueError("llm_decisions replay provider requires cache_path")
            self.config.cache_mode = "replay_only"
            self.generator = ReplayDecisionGenerator(self.cache)
        elif provider == "openai":
            if self.cache is None:
                raise ValueError(
                    "llm_decisions openai provider requires cache_path for replayability"
                )
            model = (
                config.model
                if config.model and config.model != "template"
                else load_openai_model()
            )
            self.generator = OpenAIDecisionGenerator(model=model)
        elif provider == "anthropic":
            if self.cache is None:
                raise ValueError(
                    "llm_decisions anthropic provider requires cache_path for replayability"
                )
            model = (
                config.model
                if config.model and config.model != "template"
                else load_anthropic_model()
            )
            self.generator = AnthropicDecisionGenerator(model=model)
        else:
            raise ValueError(f"Unsupported llm_decisions provider: {config.provider}")

    def should_fire(self, simulation) -> bool:
        cfg = self.config
        if not cfg.enabled:
            return False
        if simulation.current_step < cfg.start_step:
            return False
        if cfg.end_step is not None and simulation.current_step > cfg.end_step:
            return False
        return (simulation.current_step - cfg.start_step) % cfg.interval_steps == 0

    def execute(self, simulation) -> list[LLMDecisionEvent]:
        if not self.should_fire(simulation):
            return []
        candidates = self._select_agents(simulation)[
            : self.config.agent_budget_per_interval
        ]
        return [self._decide(simulation, agent) for agent in candidates]

    def _select_agents(self, simulation) -> list:
        total_exits = len(simulation.exits)
        total_hazards = len(simulation.hazards)
        rows = []
        for agent in simulation._active_agents():
            entropy = (
                agent_entropy(agent.beliefs, total_exits, total_hazards)
                if hasattr(agent, "beliefs")
                else 0.0
            )
            pressure = (
                entropy
                + float(getattr(agent, "local_density", 0.0))
                + float(getattr(agent, "current_hazard_load", 0.0))
            )
            rows.append((pressure, agent))
        rows.sort(key=lambda item: item[0], reverse=True)
        return [agent for _, agent in rows]

    def _decide(self, simulation, agent) -> LLMDecisionEvent:
        request = _build_decision_request(simulation, agent, self.config)
        key = self.cache.key_for(request) if self.cache is not None else ""
        cache_status = "disabled"
        audit = _empty_decision_budget_audit()
        cached = self.cache.load(key) if self.cache is not None else None
        if cached is not None and self.config.cache_mode in {
            "cache_first",
            "replay_only",
        }:
            decision = cached.decision
            validation = cached.validation
            cache_status = "hit"
            audit = self._cached_audit(decision)
        else:
            if self.config.cache_mode == "replay_only":
                decision = self.generator.generate(request, key)
            else:
                audit = self._budget_check(request)
                if audit["allowed"]:
                    self.budget_guard.record(audit["check"])
                    decision = self.generator.generate(request, key)
                else:
                    decision = _budget_exceeded_decision(request, audit)
            validation = validate_agent_decision(
                decision,
                request=request,
                min_confidence=validator_settings(
                    self.config.validator_profile
                ).min_confidence,
                max_trust_delta=self.config.max_trust_delta,
            )
            if not audit.get("allowed", True):
                cache_status = "budget_exceeded"
            else:
                cache_status = "miss" if self.cache is not None else "disabled"
            if (
                self.cache is not None
                and self.config.store_cache
                and self.config.cache_mode != "replay_only"
                and cache_status != "budget_exceeded"
            ):
                self.cache.store(LLMDecisionRecord(key, request, decision, validation))

        if validation.accepted:
            _apply_agent_decision(agent, decision)
        target = decision.target_exit
        event = LLMDecisionEvent(
            step=int(simulation.current_step),
            time_s=float(simulation.time_s),
            agent_id=int(agent.id),
            provider=str(decision.provider),
            model=str(decision.model),
            cache_key=key,
            cache_status=cache_status,
            validation_status=validation.status,
            validation_reasons=";".join(validation.reasons),
            selected_intent=str(decision.intent),
            target_exit_floor=_cell_floor(target),
            target_exit_x=None if target is None else _cell_xy(target)[0],
            target_exit_y=None if target is None else _cell_xy(target)[1],
            trust_delta=float(decision.trust_delta),
            avoid_congested=bool(decision.avoid_congested),
            confidence=float(decision.confidence),
            rationale=str(decision.rationale),
            used_fallback=not validation.accepted,
            objective=self.config.objective,
        )
        _append_decision_audit(simulation, event, request, decision, validation, audit)
        return event

    def _budget_check(self, request: LLMDecisionRequest) -> dict[str, Any]:
        input_tokens = estimate_llm_tokens(
            {
                "instructions": _decision_instructions(request.prompt_style),
                "input": _to_jsonable(request),
            },
            output_tokens=0,
        )
        output_tokens = 300
        check = self.budget_guard.evaluate(input_tokens, output_tokens)
        return {
            "allowed": check.allowed,
            "budget_reason": check.reason,
            "estimated_input_tokens": check.estimated_input_tokens,
            "estimated_output_tokens": check.estimated_output_tokens,
            "estimated_total_tokens": check.estimated_total_tokens,
            "estimated_usd": check.estimated_usd,
            "check": check,
        }

    def _cached_audit(self, decision: GeneratedAgentDecision) -> dict[str, Any]:
        usage = raw_usage_tokens(decision.raw_response)
        return {
            "allowed": True,
            "budget_reason": "cache_hit",
            "estimated_input_tokens": usage["input_tokens"],
            "estimated_output_tokens": usage["output_tokens"],
            "estimated_total_tokens": usage["total_tokens"],
            "estimated_usd": estimate_llm_cost(
                input_tokens=usage["input_tokens"],
                output_tokens=usage["output_tokens"],
                input_usd_per_mtok=self.config.input_usd_per_mtok,
                output_usd_per_mtok=self.config.output_usd_per_mtok,
            ),
        }


def _empty_decision_budget_audit() -> dict[str, Any]:
    return {
        "allowed": True,
        "budget_reason": "",
        "estimated_input_tokens": 0,
        "estimated_output_tokens": 0,
        "estimated_total_tokens": 0,
        "estimated_usd": 0.0,
    }


def _budget_exceeded_decision(
    request: LLMDecisionRequest,
    audit: dict[str, Any],
) -> GeneratedAgentDecision:
    return GeneratedAgentDecision(
        intent=request.current_intent,
        abstain=True,
        rationale=f"budget_guard:{audit['budget_reason']}",
        provider="budget_guard",
        model="local",
        raw_response={
            "error": audit["budget_reason"],
            "estimated_input_tokens": audit["estimated_input_tokens"],
            "estimated_output_tokens": audit["estimated_output_tokens"],
            "estimated_total_tokens": audit["estimated_total_tokens"],
            "estimated_usd": audit["estimated_usd"],
        },
    )


def _append_decision_audit(
    simulation,
    event: LLMDecisionEvent,
    request: LLMDecisionRequest,
    decision: GeneratedAgentDecision,
    validation: ValidationResult,
    audit: dict[str, Any],
) -> None:
    rows = getattr(simulation, "llm_call_audit", None)
    if rows is None:
        return
    usage = raw_usage_tokens(decision.raw_response)
    rows.append(
        {
            "step": event.step,
            "time_s": event.time_s,
            "surface": "agent_decision",
            "policy": "llm_decisions",
            "agent_id": event.agent_id,
            "provider": decision.provider,
            "model": decision.model,
            "cache_key": event.cache_key,
            "cache_status": event.cache_status,
            "validation_status": validation.status,
            "validation_reasons": event.validation_reasons,
            "used_fallback": event.used_fallback,
            "objective": request.objective,
            "prompt_style": request.prompt_style,
            "target_x": None,
            "target_y": None,
            "estimated_input_tokens": int(audit.get("estimated_input_tokens", 0)),
            "estimated_output_tokens": int(audit.get("estimated_output_tokens", 0)),
            "estimated_total_tokens": int(audit.get("estimated_total_tokens", 0)),
            "estimated_usd": float(audit.get("estimated_usd", 0.0)),
            "budget_reason": str(audit.get("budget_reason", "")),
            "raw_input_tokens": usage["input_tokens"],
            "raw_output_tokens": usage["output_tokens"],
            "raw_total_tokens": usage["total_tokens"],
        }
    )


def create_agent_decision_policy(
    payload: Optional[dict[str, Any]],
) -> Optional[AgentDecisionPolicy]:
    config = AgentDecisionConfig.from_mapping(payload)
    if not config.enabled:
        return None
    return AgentDecisionPolicy(config)


def validate_agent_decision(
    decision: GeneratedAgentDecision,
    *,
    request: LLMDecisionRequest,
    min_confidence: float,
    max_trust_delta: float,
) -> ValidationResult:
    reasons: list[str] = []
    known_exits = {tuple(exit_) for exit_ in request.known_exits}
    congested_exits = {tuple(exit_) for exit_ in request.congested_exits}
    if decision.abstain:
        reasons.append("generator_abstained")
    if decision.intent not in ALLOWED_INTENTS:
        reasons.append(f"unsupported_intent:{decision.intent}")
    if decision.target_exit is not None:
        target = tuple(decision.target_exit)
        if target not in known_exits:
            reasons.append(f"unknown_target_exit:{target}")
        if decision.avoid_congested and target in congested_exits:
            reasons.append(f"congested_target_exit:{target}")
    if abs(float(decision.trust_delta)) > max_trust_delta:
        reasons.append(f"unsafe_trust_delta:{decision.trust_delta}")
    if decision.confidence < min_confidence and not decision.abstain:
        reasons.append(f"low_confidence:{decision.confidence:.2f}")
    if not decision.rationale.strip() and not decision.abstain:
        reasons.append("empty_rationale")
    return ValidationResult(accepted=not reasons, reasons=reasons)


def _build_decision_request(
    simulation, agent, config: AgentDecisionConfig
) -> LLMDecisionRequest:
    known_exits = (
        [tuple(exit_) for exit_ in agent.beliefs.known_exits()]
        if hasattr(agent, "beliefs")
        else []
    )
    entropy = (
        agent_entropy(agent.beliefs, len(simulation.exits), len(simulation.hazards))
        if hasattr(agent, "beliefs")
        else 0.0
    )
    return LLMDecisionRequest(
        step=int(simulation.current_step),
        agent_id=int(agent.id),
        current_intent=str(getattr(agent, "intention", INTENTION_EVACUATE)),
        objective=config.objective,
        known_exits=known_exits,
        congested_exits=_congested_exits(simulation),
        hazards=[
            HazardSnapshot(
                position=_point3(hazard.pos),
                kind=str(hazard.kind),
                radius=float(hazard.radius),
                severity=float(hazard.severity),
            )
            for hazard in simulation.hazards
        ],
        local_density=float(getattr(agent, "local_density", 0.0)),
        hazard_load=float(getattr(agent, "current_hazard_load", 0.0)),
        entropy=float(entropy),
        prompt_style=config.prompt_style,
    )


def _apply_agent_decision(agent, decision: GeneratedAgentDecision) -> None:
    agent.intention = decision.intent
    if decision.target_exit is not None:
        agent.target_exit = tuple(decision.target_exit)
        agent.current_path = []
        agent.path_index = 0
    if decision.trust_delta:
        value = float(getattr(agent, "base_rationality", 0.8)) + float(
            decision.trust_delta
        )
        agent.base_rationality = max(0.0, min(1.0, value))
        agent.rationality = agent.base_rationality * getattr(
            agent.physiology, "rationality_factor", 1.0
        )


def _congested_exits(simulation) -> list[Cell]:
    totals = getattr(simulation, "exit_flow_cumulative", {})
    if not totals:
        return []
    labels_by_cell = {label: cell for cell, label in simulation.exit_labels.items()}
    total_flow = sum(totals.values())
    if total_flow <= 0:
        return []
    return [
        labels_by_cell[label]
        for label, flow in totals.items()
        if flow / total_flow >= 0.5 and label in labels_by_cell
    ]


def _decision_instructions(prompt_style: str) -> str:
    return (
        "You control one simulated evacuation agent in a bounded research model. "
        "Return only JSON with keys intent, target_exit, trust_delta, "
        "avoid_congested, rationale, confidence, abstain. intent must be one of "
        "EVACUATE, EXPLORE, FOLLOW. target_exit must be null or one listed known_exit. "
        "trust_delta must be between -0.2 and 0.2. Do not invent exits, hazards, "
        "layout changes, physics changes, or operational advice. "
        f"Prompt style: {prompt_style}."
    )


def _parse_optional_cell(value: Any) -> Optional[Cell]:
    if value in (None, ""):
        return None
    if isinstance(value, dict):
        if value.get("floor") is not None:
            value = [value.get("floor"), value.get("x"), value.get("y")]
        else:
            value = [value.get("x"), value.get("y")]
    if (
        isinstance(value, (list, tuple))
        and len(value) >= 3
        and isinstance(value[0], str)
    ):
        try:
            return (str(value[0]), int(value[1]), int(value[2]))
        except (TypeError, ValueError):
            return None
    if isinstance(value, (list, tuple)) and len(value) >= 2:
        try:
            return (int(value[0]), int(value[1]))
        except (TypeError, ValueError):
            return None
    return None


def _cell_floor(cell: Optional[Cell]) -> Optional[str]:
    if cell is None:
        return None
    return str(cell[0]) if len(cell) >= 3 and isinstance(cell[0], str) else None


def _cell_xy(cell: Cell) -> tuple[int, int]:
    if len(cell) >= 3 and isinstance(cell[0], str):
        return int(cell[1]), int(cell[2])
    return int(cell[0]), int(cell[1])


def _point3(value: Any) -> tuple[float, float, float]:
    if len(value) >= 3:
        return (float(value[0]), float(value[1]), float(value[2]))
    return (float(value[0]), float(value[1]), 0.0)


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
