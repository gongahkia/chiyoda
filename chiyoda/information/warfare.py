"""Adversarial information channels and source credibility updates."""

from __future__ import annotations

from collections.abc import Iterable, Sequence
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any
from uuid import uuid4

import numpy as np

from chiyoda.information.field import ExitBelief, HazardBelief
from chiyoda.information.llm import (
    AnthropicMessagesGenerator,
    GeneratedEvacuationMessage,
    HazardSnapshot,
    LLMBudgetGuard,
    LLMGenerationRecord,
    LLMMessageCache,
    LLMMessageRequest,
    OpenAIResponsesGenerator,
    ReplayOnlyGenerator,
    TemplateLLMGenerator,
    ValidationResult,
    build_prompt_instructions,
    estimate_llm_cost,
    estimate_llm_tokens,
    load_anthropic_model,
    load_openai_model,
    raw_usage_tokens,
)


class AttackerObjective(str, Enum):
    FALSE_PROTECTIVE_ACTION = "false-protective-action"
    THREAT_AMPLIFICATION = "threat-amplification"
    AUTHORITY_CONFUSION = "authority-confusion"
    SOCIAL_PROOF_POISONING = "social-proof-poisoning"


@dataclass
class BeliefRevisionConfig:
    prior_alpha: float = 2.0
    prior_beta: float = 2.0
    forgetting_factor: float = 0.95
    evidence_weight: float = 1.0
    min_credibility: float = 0.05
    max_credibility: float = 0.95


@dataclass
class SourceCredibilityState:
    source_id: str
    alpha: float
    beta: float
    last_updated_step: int = 0

    @property
    def credibility(self) -> float:
        denom = self.alpha + self.beta
        if denom <= 0:
            return 0.5
        return self.alpha / denom


@dataclass
class ProvenanceRecord:
    record_id: str
    source_id: str
    timestamp_s: float
    step: int
    channel_type: str
    objective: str
    claimed_exit: tuple | None = None
    claimed_hazard: tuple | None = None
    observed_outcome: bool | None = None
    credibility_after: float | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "record_id": self.record_id,
            "source_id": self.source_id,
            "timestamp_s": self.timestamp_s,
            "step": self.step,
            "channel_type": self.channel_type,
            "objective": self.objective,
            "claimed_exit": (
                list(self.claimed_exit) if self.claimed_exit is not None else None
            ),
            "claimed_hazard": (
                list(self.claimed_hazard) if self.claimed_hazard is not None else None
            ),
            "observed_outcome": self.observed_outcome,
            "credibility_after": self.credibility_after,
        }


@dataclass
class BeliefRevisionModel:
    config: BeliefRevisionConfig = field(default_factory=BeliefRevisionConfig)
    sources: dict[str, SourceCredibilityState] = field(default_factory=dict)
    provenance: list[ProvenanceRecord] = field(default_factory=list)

    def source_credibility(self, source_id: str) -> float:
        state = self._state(source_id)
        return float(
            np.clip(
                state.credibility,
                self.config.min_credibility,
                self.config.max_credibility,
            )
        )

    def record_claim(
        self,
        *,
        source_id: str,
        timestamp_s: float,
        step: int,
        channel_type: str,
        objective: str,
        claimed_exit: Sequence[Any] | None = None,
        claimed_hazard: Sequence[Any] | None = None,
    ) -> ProvenanceRecord:
        record = ProvenanceRecord(
            record_id=str(uuid4()),
            source_id=str(source_id),
            timestamp_s=float(timestamp_s),
            step=int(step),
            channel_type=str(channel_type),
            objective=str(objective),
            claimed_exit=_tuple_or_none(claimed_exit),
            claimed_hazard=_tuple_or_none(claimed_hazard),
        )
        self.provenance.append(record)
        return record

    def update_source(
        self,
        source_id: str,
        supported: bool,
        *,
        step: int = 0,
        weight: float | None = None,
    ) -> float:
        state = self._state(source_id)
        cfg = self.config
        state.alpha = (
            cfg.prior_alpha + (state.alpha - cfg.prior_alpha) * cfg.forgetting_factor
        )
        state.beta = (
            cfg.prior_beta + (state.beta - cfg.prior_beta) * cfg.forgetting_factor
        )
        evidence = cfg.evidence_weight if weight is None else float(weight)
        if supported:
            state.alpha += evidence
        else:
            state.beta += evidence
        state.last_updated_step = int(step)
        return self.source_credibility(source_id)

    def observe_record(
        self, record_id: str, supported: bool, *, step: int = 0
    ) -> float:
        record = next(
            (item for item in self.provenance if item.record_id == record_id), None
        )
        if record is None:
            raise KeyError(record_id)
        credibility = self.update_source(record.source_id, supported, step=step)
        record.observed_outcome = bool(supported)
        record.credibility_after = credibility
        return credibility

    def pending_records(self) -> Iterable[ProvenanceRecord]:
        return (record for record in self.provenance if record.observed_outcome is None)

    def _state(self, source_id: str) -> SourceCredibilityState:
        if source_id not in self.sources:
            self.sources[source_id] = SourceCredibilityState(
                source_id=source_id,
                alpha=float(self.config.prior_alpha),
                beta=float(self.config.prior_beta),
            )
        return self.sources[source_id]


@dataclass
class HostileChannelConfig:
    """Hostile-channel configuration.

    Persona-targeting fields (``target_persona``) follow the
    cohort/mobility/age homophily code path in
    ``chiyoda/information/propagation.py``. See arxiv 2511.04697 for the
    persona-driven misinformation framing this mirrors.
    """

    id: str = "hostile_channel"
    channel_type: str = "gossip"
    objective: AttackerObjective = AttackerObjective.FALSE_PROTECTIVE_ACTION
    budget: int = 1
    start_step: int = 0
    interval_steps: int = 1
    plausibility: float = 0.65
    radius: float = 6.0
    target_cohort: str | None = None
    target_persona: dict[str, Any] | None = None
    source_id: str = "attacker"
    claimed_exit: tuple | None = None
    claimed_hazard: tuple | None = None
    enabled: bool = True
    llm_claims_enabled: bool = False
    llm_provider: str = "template"
    llm_model: str = "template"
    llm_cache_path: str | None = None
    llm_cache_mode: str = "cache_first"
    llm_store_cache: bool = True
    llm_prompt_style: str = "hostile_red_team"
    llm_max_calls_per_run: int | None = None
    llm_max_estimated_tokens_per_run: int | None = None
    llm_max_estimated_usd_per_run: float | None = None
    llm_input_usd_per_mtok: float = 0.0
    llm_output_usd_per_mtok: float = 0.0

    @classmethod
    def from_mapping(cls, payload: dict[str, Any]) -> HostileChannelConfig:
        objective = AttackerObjective(
            str(
                payload.get(
                    "objective", AttackerObjective.FALSE_PROTECTIVE_ACTION.value
                )
            )
        )
        llm_provider = payload.get("llm_provider")
        return cls(
            id=str(payload.get("id", "hostile_channel")),
            channel_type=str(
                payload.get("channel_type", payload.get("type", "gossip"))
            ),
            objective=objective,
            budget=max(0, int(payload.get("budget", 1))),
            start_step=int(payload.get("start_step", 0)),
            interval_steps=max(1, int(payload.get("interval_steps", 1))),
            plausibility=float(payload.get("plausibility", 0.65)),
            radius=float(payload.get("radius", 6.0)),
            target_cohort=(
                None
                if payload.get("target_cohort") is None
                else str(payload["target_cohort"])
            ),
            target_persona=_normalize_persona(payload.get("target_persona")),
            source_id=str(payload.get("source_id", "attacker")),
            claimed_exit=_tuple_or_none(payload.get("claimed_exit")),
            claimed_hazard=_tuple_or_none(payload.get("claimed_hazard")),
            enabled=bool(payload.get("enabled", True)),
            llm_claims_enabled=bool(
                payload.get("llm_claims_enabled", llm_provider is not None)
            ),
            llm_provider=str(llm_provider or "template").lower(),
            llm_model=str(payload.get("llm_model", "template")),
            llm_cache_path=(
                None
                if payload.get("llm_cache_path") is None
                else str(payload["llm_cache_path"])
            ),
            llm_cache_mode=str(payload.get("llm_cache_mode", "cache_first")),
            llm_store_cache=bool(payload.get("llm_store_cache", True)),
            llm_prompt_style=str(payload.get("llm_prompt_style", "hostile_red_team")),
            llm_max_calls_per_run=_optional_int(payload.get("llm_max_calls_per_run")),
            llm_max_estimated_tokens_per_run=_optional_int(
                payload.get("llm_max_estimated_tokens_per_run")
            ),
            llm_max_estimated_usd_per_run=_optional_float(
                payload.get("llm_max_estimated_usd_per_run")
            ),
            llm_input_usd_per_mtok=float(payload.get("llm_input_usd_per_mtok", 0.0)),
            llm_output_usd_per_mtok=float(payload.get("llm_output_usd_per_mtok", 0.0)),
        )


@dataclass
class HostileChannelEvent:
    step: int
    time_s: float
    channel_id: str
    channel_type: str
    objective: str
    source_id: str
    recipients: int
    credibility: float
    claimed_exit: tuple | None = None
    claimed_hazard: tuple | None = None


class LLMHostileClaimGenerator:
    def __init__(self, config: HostileChannelConfig) -> None:
        self.config = config
        self.cache = (
            LLMMessageCache(Path(config.llm_cache_path))
            if config.llm_cache_path
            else None
        )
        self.budget_guard = LLMBudgetGuard(
            max_calls=config.llm_max_calls_per_run,
            max_estimated_tokens=config.llm_max_estimated_tokens_per_run,
            max_estimated_usd=config.llm_max_estimated_usd_per_run,
            input_usd_per_mtok=config.llm_input_usd_per_mtok,
            output_usd_per_mtok=config.llm_output_usd_per_mtok,
        )
        provider = config.llm_provider.lower()
        if provider == "template":
            self.generator = TemplateLLMGenerator()
        elif provider in {"replay", "local_replay"}:
            if self.cache is None:
                raise ValueError("hostile LLM replay requires llm_cache_path")
            config.llm_cache_mode = "replay_only"
            self.generator = ReplayOnlyGenerator(self.cache)
        elif provider == "openai":
            model = (
                config.llm_model
                if config.llm_model and config.llm_model != "template"
                else load_openai_model()
            )
            self.generator = OpenAIResponsesGenerator(model=model)
        elif provider == "anthropic":
            model = (
                config.llm_model
                if config.llm_model and config.llm_model != "template"
                else load_anthropic_model()
            )
            self.generator = AnthropicMessagesGenerator(model=model)
        else:
            raise ValueError(
                "Unsupported hostile llm_provider. Use 'template', 'replay', 'local_replay', 'openai', or 'anthropic'."
            )

    def build_claim(
        self,
        simulation,
        recipients: Sequence[Any],
        fallback: dict[str, tuple | None],
    ) -> dict[str, tuple | None]:
        request = self._build_request(simulation, recipients, fallback)
        cache_key = self.cache.key_for(request) if self.cache is not None else ""
        generated, cached_validation, cache_status, audit = self._generate(
            request, cache_key
        )
        claim, validation, used_fallback = self._claim_from_message(generated, fallback)
        if (
            cached_validation is not None
            and validation.reasons == cached_validation.reasons
        ):
            validation = cached_validation
        if (
            self.cache is not None
            and self.config.llm_store_cache
            and self.config.llm_provider not in {"replay", "local_replay"}
            and cache_status not in {"hit", "budget_exceeded"}
        ):
            self.cache.store(
                LLMGenerationRecord(
                    cache_key=cache_key,
                    request=request,
                    message=generated,
                    validation=validation,
                )
            )
        _append_hostile_llm_audit(
            simulation,
            config=self.config,
            request=request,
            generated=generated,
            validation=validation,
            cache_key=cache_key,
            cache_status=cache_status,
            used_fallback=used_fallback,
            audit=audit,
        )
        return claim

    def _build_request(
        self,
        simulation,
        recipients: Sequence[Any],
        fallback: dict[str, tuple | None],
    ) -> LLMMessageRequest:
        target = _mean_agent_point(recipients)
        exits = [fallback["exit"]] if fallback.get("exit") is not None else []
        hazards = (
            [
                HazardSnapshot(
                    position=tuple(fallback["hazard"]),
                    kind="hostile_claim",
                    radius=float(self.config.radius),
                    severity=float(self.config.plausibility),
                )
            ]
            if fallback.get("hazard") is not None
            else []
        )
        return LLMMessageRequest(
            policy=f"hostile_channel:{self.config.id}",
            step=int(simulation.current_step),
            target=target,
            selected_reason="hostile_claim_generation",
            objective=self.config.objective.value,
            exits=exits,
            hazards=hazards,
            congested_exits=[],
            recipients_estimate=len(recipients),
            mean_local_density=_mean(
                [float(getattr(agent, "local_density", 0.0)) for agent in recipients]
            ),
            mean_hazard_load=_mean(
                [
                    float(getattr(agent, "current_hazard_load", 0.0))
                    for agent in recipients
                ]
            ),
            prompt_style=self.config.llm_prompt_style,
        )

    def _generate(
        self, request: LLMMessageRequest, cache_key: str
    ) -> tuple[
        GeneratedEvacuationMessage, ValidationResult | None, str, dict[str, Any]
    ]:
        if self.cache is not None:
            cached = self.cache.load(cache_key)
            if cached is not None and self.config.llm_cache_mode in {
                "cache_first",
                "replay_only",
            }:
                return (
                    cached.message,
                    cached.validation,
                    "hit",
                    self._cached_audit(cached.message),
                )
            if self.config.llm_cache_mode == "replay_only":
                return (
                    self.generator.generate(request, cache_key),
                    None,
                    "miss",
                    _empty_budget_audit(),
                )

        check = self._budget_check(request)
        if not check["allowed"]:
            return self._budget_exceeded_message(check), None, "budget_exceeded", check
        self.budget_guard.record(check["check"])
        status = "miss" if self.cache is not None else "disabled"
        return self.generator.generate(request, cache_key), None, status, check

    def _budget_check(self, request: LLMMessageRequest) -> dict[str, Any]:
        input_tokens = estimate_llm_tokens(
            {
                "instructions": build_prompt_instructions(request.prompt_style),
                "input": request,
            },
            output_tokens=0,
        )
        output_tokens = 500
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

    def _cached_audit(self, message: GeneratedEvacuationMessage) -> dict[str, Any]:
        usage = raw_usage_tokens(message.raw_response)
        return {
            "allowed": True,
            "budget_reason": "cache_hit",
            "estimated_input_tokens": usage["input_tokens"],
            "estimated_output_tokens": usage["output_tokens"],
            "estimated_total_tokens": usage["total_tokens"],
            "estimated_usd": estimate_llm_cost(
                input_tokens=usage["input_tokens"],
                output_tokens=usage["output_tokens"],
                input_usd_per_mtok=self.config.llm_input_usd_per_mtok,
                output_usd_per_mtok=self.config.llm_output_usd_per_mtok,
            ),
        }

    def _budget_exceeded_message(
        self, audit: dict[str, Any]
    ) -> GeneratedEvacuationMessage:
        return GeneratedEvacuationMessage(
            text="LLM budget guard blocked hostile claim generation.",
            abstain=True,
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

    def _claim_from_message(
        self,
        message: GeneratedEvacuationMessage,
        fallback: dict[str, tuple | None],
    ) -> tuple[dict[str, tuple | None], ValidationResult, bool]:
        if message.abstain:
            return (
                fallback,
                ValidationResult(False, ["generator_abstained"]),
                True,
            )
        if self.config.objective == AttackerObjective.THREAT_AMPLIFICATION:
            if message.hazard_positions:
                return (
                    {"exit": None, "hazard": tuple(message.hazard_positions[0])},
                    ValidationResult(True, []),
                    False,
                )
            return fallback, ValidationResult(False, ["missing_claimed_hazard"]), True
        if message.recommended_exits:
            return (
                {"exit": tuple(message.recommended_exits[0]), "hazard": None},
                ValidationResult(True, []),
                False,
            )
        return fallback, ValidationResult(False, ["missing_claimed_exit"]), True


class HostileChannel:
    def __init__(self, config: HostileChannelConfig) -> None:
        self.config = config
        self.emitted = 0
        self.llm_claim_generator = (
            LLMHostileClaimGenerator(config) if config.llm_claims_enabled else None
        )

    def should_fire(self, simulation) -> bool:
        cfg = self.config
        if not cfg.enabled or self.emitted >= cfg.budget:
            return False
        if simulation.current_step < cfg.start_step:
            return False
        return (simulation.current_step - cfg.start_step) % cfg.interval_steps == 0

    def execute(self, simulation) -> HostileChannelEvent | None:
        if not self.should_fire(simulation):
            return None
        recipients = self._select_recipients(simulation)
        if not recipients:
            return None
        remaining = self.config.budget - self.emitted
        recipients = recipients[:remaining]
        claim = self._build_claim(simulation, recipients)
        objective = self.config.objective.value
        credibility_values = []
        for agent in recipients:
            source_id = self._source_id(agent)
            credibility = self._effective_credibility(agent, source_id)
            credibility_values.append(credibility)
            if claim.get("exit") is not None:
                agent.beliefs.exit_beliefs[claim["exit"]] = ExitBelief(
                    position=claim["exit"],
                    exists_prob=min(
                        0.99, max(0.01, self.config.plausibility * credibility + 0.2)
                    ),
                    congestion_est=0.0,
                    freshness=0.0,
                    source_credibility=credibility,
                    hop_count=0,
                )
            if claim.get("hazard") is not None:
                agent.beliefs.hazard_beliefs.append(
                    HazardBelief(
                        position=claim["hazard"],
                        severity_est=min(
                            1.0, 0.4 + self.config.plausibility * credibility
                        ),
                        radius_est=max(1.0, self.config.radius),
                        freshness=0.0,
                        source_credibility=credibility,
                        hop_count=0,
                    )
                )
                agent.beliefs.general_danger_level = max(
                    agent.beliefs.general_danger_level, 0.8
                )
            agent.belief_revision.record_claim(
                source_id=source_id,
                timestamp_s=simulation.time_s,
                step=simulation.current_step,
                channel_type=self.config.channel_type,
                objective=objective,
                claimed_exit=claim.get("exit"),
                claimed_hazard=claim.get("hazard"),
            )
            if hasattr(agent, "update_intention"):
                agent.update_intention(simulation)
        self.emitted += len(recipients)
        return HostileChannelEvent(
            step=int(simulation.current_step),
            time_s=float(simulation.time_s),
            channel_id=self.config.id,
            channel_type=self.config.channel_type,
            objective=objective,
            source_id=self.config.source_id,
            recipients=len(recipients),
            credibility=(
                float(np.mean(credibility_values)) if credibility_values else 0.0
            ),
            claimed_exit=claim.get("exit"),
            claimed_hazard=claim.get("hazard"),
        )

    def _select_recipients(self, simulation) -> list[Any]:
        persona = self.config.target_persona or {}
        active = [
            agent
            for agent in simulation._active_agents()
            if hasattr(agent, "beliefs")
            and (
                self.config.target_cohort is None
                or getattr(agent, "cohort_name", None) == self.config.target_cohort
            )
            and _persona_match(agent, persona)
        ]
        if not active:
            return []
        if self.config.objective == AttackerObjective.SOCIAL_PROOF_POISONING:
            active.sort(
                key=lambda item: getattr(item, "credibility", 0.5), reverse=True
            )
            return active
        if (
            self.config.objective == AttackerObjective.THREAT_AMPLIFICATION
            and simulation.hazards
        ):
            hazard_pos = np.array(simulation.hazards[0].pos, dtype=float)
            active.sort(key=lambda item: float(np.linalg.norm(item.pos - hazard_pos)))
            return active
        return active

    def _build_claim(
        self, simulation, recipients: Sequence[Any] = ()
    ) -> dict[str, tuple | None]:
        fallback = self._fallback_claim(simulation)
        if self.llm_claim_generator is not None:
            return self.llm_claim_generator.build_claim(
                simulation, recipients, fallback
            )
        return fallback

    def _fallback_claim(self, simulation) -> dict[str, tuple | None]:
        if self.config.objective in {
            AttackerObjective.FALSE_PROTECTIVE_ACTION,
            AttackerObjective.AUTHORITY_CONFUSION,
            AttackerObjective.SOCIAL_PROOF_POISONING,
        }:
            claimed_exit = self.config.claimed_exit
            if claimed_exit is None:
                claimed_exit = _default_false_exit(simulation)
            return {"exit": tuple(claimed_exit), "hazard": None}
        claimed_hazard = self.config.claimed_hazard
        if claimed_hazard is None:
            claimed_hazard = _default_false_hazard(simulation)
        return {"exit": None, "hazard": tuple(claimed_hazard)}

    def _source_id(self, agent) -> str:
        if self.config.objective == AttackerObjective.AUTHORITY_CONFUSION:
            return "responder:spoofed"
        if self.config.objective == AttackerObjective.SOCIAL_PROOF_POISONING:
            return f"agent:{agent.id}:poisoned"
        return self.config.source_id

    def _effective_credibility(self, agent, source_id: str) -> float:
        trust = agent.belief_revision.source_credibility(source_id)
        return float(np.clip(self.config.plausibility * (0.5 + trust), 0.01, 0.99))


def create_hostile_channels(
    payload: Sequence[dict[str, Any]] | None,
) -> list[HostileChannel]:
    return [
        HostileChannel(HostileChannelConfig.from_mapping(item))
        for item in (payload or [])
    ]


def evaluate_pending_provenance(agent, simulation, vision_radius: float) -> None:
    if not hasattr(agent, "belief_revision"):
        return
    true_exits = {tuple(exit_.pos) for exit_ in simulation.exits}
    true_hazards = [
        tuple(float(value) for value in hazard.pos) for hazard in simulation.hazards
    ]
    for record in list(agent.belief_revision.pending_records()):
        supported = None
        if record.claimed_exit is not None:
            claim_world = simulation.layout.world_position(record.claimed_exit)
            if _distance(agent.pos, claim_world) <= vision_radius:
                supported = tuple(record.claimed_exit) in true_exits
        if supported is None and record.claimed_hazard is not None:
            if _distance(agent.pos, record.claimed_hazard) <= vision_radius:
                supported = any(
                    _distance(record.claimed_hazard, hazard_pos) < 3.0
                    for hazard_pos in true_hazards
                )
        if supported is not None:
            agent.belief_revision.observe_record(
                record.record_id, supported, step=simulation.current_step
            )


def _append_hostile_llm_audit(
    simulation,
    *,
    config: HostileChannelConfig,
    request: LLMMessageRequest,
    generated: GeneratedEvacuationMessage,
    validation: ValidationResult,
    cache_key: str,
    cache_status: str,
    used_fallback: bool,
    audit: dict[str, Any],
) -> None:
    rows = getattr(simulation, "llm_call_audit", None)
    if rows is None:
        return
    usage = raw_usage_tokens(generated.raw_response)
    rows.append(
        {
            "step": int(simulation.current_step),
            "time_s": float(simulation.time_s),
            "surface": "hostile_channel",
            "policy": config.id,
            "agent_id": None,
            "provider": generated.provider,
            "model": generated.model,
            "cache_key": cache_key,
            "cache_status": cache_status,
            "validation_status": validation.status,
            "validation_reasons": ";".join(validation.reasons),
            "judge_status": "",
            "judge_safety": None,
            "judge_specificity": None,
            "judge_alignment": None,
            "judge_reasons": "",
            "judge_provider": "",
            "used_fallback": bool(used_fallback),
            "objective": request.objective,
            "prompt_style": request.prompt_style,
            "target_x": float(request.target[0]),
            "target_y": float(request.target[1]),
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


def _empty_budget_audit() -> dict[str, Any]:
    return {
        "allowed": False,
        "budget_reason": "cache_miss",
        "estimated_input_tokens": 0,
        "estimated_output_tokens": 0,
        "estimated_total_tokens": 0,
        "estimated_usd": 0.0,
    }


def _default_false_exit(simulation) -> tuple:
    floor_id = simulation.layout.primary_floor_id
    return (
        floor_id,
        max(0, simulation.layout.width - 2),
        max(0, simulation.layout.height - 2),
    )


def _default_false_hazard(simulation) -> tuple:
    floor_id = simulation.layout.primary_floor_id
    z = float(simulation.layout.floor_z(floor_id))
    return (
        max(0.5, simulation.layout.width / 2.0),
        max(0.5, simulation.layout.height / 2.0),
        z,
    )


def _tuple_or_none(value: Sequence[Any] | None) -> tuple | None:
    if value is None:
        return None
    if isinstance(value, dict):
        if {"floor", "x", "y"}.issubset(value):
            return (str(value["floor"]), int(value["x"]), int(value["y"]))
        if {"x", "y", "z"}.issubset(value):
            return (float(value["x"]), float(value["y"]), float(value["z"]))
    return tuple(value)


def _optional_int(value: Any) -> int | None:
    return None if value is None else int(value)


def _optional_float(value: Any) -> float | None:
    return None if value is None else float(value)


def _distance(a: Sequence[Any], b: Sequence[Any]) -> float:
    return float(np.linalg.norm(_point3(a) - _point3(b)))


def _normalize_persona(value: Any) -> dict[str, Any] | None:
    if not value:
        return None
    if not isinstance(value, dict):
        return None
    normalized: dict[str, Any] = {}
    if "cohort" in value:
        normalized["cohort"] = str(value["cohort"])
    if "mobility" in value:
        normalized["mobility"] = str(value["mobility"]).lower()
    if "age_band" in value:
        normalized["age_band"] = str(value["age_band"]).lower()
    return normalized or None


def _persona_match(agent: Any, persona: dict[str, Any]) -> bool:
    if not persona:
        return True
    cohort = persona.get("cohort")
    if cohort is not None and getattr(agent, "cohort_name", None) != cohort:
        return False
    mobility = persona.get("mobility")
    if mobility is not None:
        agent_mobility = str(
            getattr(agent, "mobility", getattr(agent, "mobility_class", "")) or ""
        ).lower()
        if agent_mobility != mobility:
            return False
    age_band = persona.get("age_band")
    if age_band is not None:
        agent_age = str(getattr(agent, "age_band", "") or "").lower()
        if agent_age != age_band:
            return False
    return True


def _point3(value: Sequence[Any]) -> np.ndarray:
    if len(value) >= 3 and isinstance(value[0], str):
        return np.array(
            [float(value[1]) + 0.5, float(value[2]) + 0.5, 0.0], dtype=float
        )
    if len(value) >= 3:
        return np.array(
            [float(value[0]), float(value[1]), float(value[2])], dtype=float
        )
    return np.array([float(value[0]), float(value[1]), 0.0], dtype=float)


def _mean(values: Sequence[float]) -> float:
    if not values:
        return 0.0
    return float(sum(values) / len(values))


def _mean_agent_point(agents: Sequence[Any]) -> tuple[float, float]:
    if not agents:
        return (0.0, 0.0)
    xs = [float(agent.pos[0]) for agent in agents]
    ys = [float(agent.pos[1]) for agent in agents]
    return (_mean(xs), _mean(ys))
