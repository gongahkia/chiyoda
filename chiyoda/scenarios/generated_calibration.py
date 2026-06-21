from __future__ import annotations

from copy import deepcopy
from dataclasses import asdict, dataclass, field
import hashlib
import json
from pathlib import Path
from typing import Any, Dict, List, Mapping, Optional, Sequence, Tuple
from urllib import request as urlrequest
from urllib.error import HTTPError, URLError

from chiyoda.information.llm import (
    LLMBudgetGuard,
    _extract_anthropic_text,
    estimate_llm_cost,
    estimate_llm_tokens,
    load_anthropic_api_key,
    load_anthropic_model,
    load_openai_api_key,
    load_openai_model,
    raw_usage_tokens,
)


ALLOWED_TARGETS = {"cohort_mix", "parameter_priors", "scenario_metadata"}
DEFAULT_ALLOWED_TARGETS = ("parameter_priors", "scenario_metadata")
OVERWRITE_POLICY = "missing_only"
CALIBRATION_PARAMETER_BOUNDS = {
    "base_speed": (0.4, 2.2),
    "base_speed_mps": (0.4, 2.2),
    "base_rationality": (0.0, 1.0),
    "calmness": (0.0, 1.0),
    "familiarity": (0.0, 1.0),
    "credibility": (0.0, 1.0),
    "gossip_radius": (0.0, 10.0),
    "base_vision_radius": (0.5, 20.0),
    "separation_anxiety_threshold": (0.0, 10.0),
    "breathing_height_m": (0.5, 2.2),
    "homophily_weight": (0.0, 1.0),
    "exit_affinity": (0.0, 1.0),
    "herding": (0.0, 1.0),
}
SAFE_COHORT_FIELDS = {
    "name",
    "count",
    "personality",
    "group_size",
    "persona_condition",
    "mobility_class",
    "homophily_profile",
    "family_id",
    "role_in_group",
}


@dataclass(frozen=True)
class PopulationCalibrationConfig:
    enabled: bool = False
    provider: str = "template"
    model: str = "template"
    cache_path: Optional[str] = None
    cache_mode: str = "cache_first"
    store_cache: bool = True
    allowed_targets: Tuple[str, ...] = DEFAULT_ALLOWED_TARGETS
    objective: str = "document_population_assumptions"
    prompt_style: str = "conservative"
    overwrite_policy: str = OVERWRITE_POLICY
    min_confidence: float = 0.2
    persona_conditions: Tuple[str, ...] = ()
    max_calls_per_run: Optional[int] = None
    max_estimated_tokens_per_run: Optional[int] = None
    max_estimated_usd_per_run: Optional[float] = None
    input_usd_per_mtok: float = 0.0
    output_usd_per_mtok: float = 0.0

    @classmethod
    def from_mapping(cls, payload: Optional[Mapping[str, Any]]) -> "PopulationCalibrationConfig":
        data = dict(payload or {})
        allowed = tuple(str(item) for item in data.get("allowed_targets", DEFAULT_ALLOWED_TARGETS))
        personas_raw = data.get("persona_conditions", data.get("personas", ())) or ()
        personas = (personas_raw,) if isinstance(personas_raw, str) else tuple(personas_raw)
        invalid = sorted(set(allowed) - ALLOWED_TARGETS)
        if invalid:
            raise ValueError(f"Unsupported generated calibration targets: {invalid}")
        overwrite_policy = str(data.get("overwrite_policy", OVERWRITE_POLICY))
        if overwrite_policy != OVERWRITE_POLICY:
            raise ValueError("generated population calibration only supports overwrite_policy='missing_only'")
        return cls(
            enabled=bool(data.get("enabled", False)),
            provider=str(data.get("provider", "template")),
            model=str(data.get("model", "template")),
            cache_path=None if data.get("cache_path") is None else str(data.get("cache_path")),
            cache_mode=str(data.get("cache_mode", "cache_first")),
            store_cache=bool(data.get("store_cache", True)),
            allowed_targets=allowed,
            objective=str(data.get("objective", "document_population_assumptions")),
            prompt_style=str(data.get("prompt_style", "conservative")),
            overwrite_policy=overwrite_policy,
            min_confidence=float(data.get("min_confidence", 0.2)),
            persona_conditions=tuple(str(item) for item in personas),
            max_calls_per_run=None
            if data.get("max_calls_per_run") is None
            else int(data["max_calls_per_run"]),
            max_estimated_tokens_per_run=None
            if data.get("max_estimated_tokens_per_run") is None
            else int(data["max_estimated_tokens_per_run"]),
            max_estimated_usd_per_run=None
            if data.get("max_estimated_usd_per_run") is None
            else float(data["max_estimated_usd_per_run"]),
            input_usd_per_mtok=float(data.get("input_usd_per_mtok", 0.0)),
            output_usd_per_mtok=float(data.get("output_usd_per_mtok", 0.0)),
        )


@dataclass(frozen=True)
class PopulationCalibrationRequest:
    scenario_name: str
    objective: str
    prompt_style: str
    allowed_targets: Tuple[str, ...]
    population_total: Optional[int]
    existing_cohorts: Tuple[Dict[str, Any], ...]
    hazard_count: int
    responder_count: int
    metadata_keys: Tuple[str, ...] = ()
    persona_conditions: Tuple[str, ...] = ()


@dataclass
class GeneratedPopulationCalibration:
    cohorts: List[Dict[str, Any]] = field(default_factory=list)
    parameter_priors: Dict[str, Dict[str, float]] = field(default_factory=dict)
    scenario_metadata: Dict[str, Any] = field(default_factory=dict)
    notes: List[str] = field(default_factory=list)
    confidence: float = 0.0
    abstain: bool = False
    provider: str = "deterministic"
    model: str = "template"
    raw_response: Dict[str, Any] = field(default_factory=dict)


@dataclass
class PopulationCalibrationValidation:
    accepted: bool
    reasons: List[str] = field(default_factory=list)

    @property
    def status(self) -> str:
        return "accepted" if self.accepted else "rejected"


@dataclass
class PopulationCalibrationRecord:
    cache_key: str
    request: PopulationCalibrationRequest
    calibration: GeneratedPopulationCalibration
    validation: PopulationCalibrationValidation
    application: Dict[str, Any] = field(default_factory=dict)

    def to_json_dict(self) -> Dict[str, Any]:
        return {
            "cache_key": self.cache_key,
            "request": _to_jsonable(self.request),
            "calibration": _to_jsonable(self.calibration),
            "validation": _to_jsonable(self.validation),
            "application": _to_jsonable(self.application),
        }

    @classmethod
    def from_json_dict(cls, payload: Mapping[str, Any]) -> "PopulationCalibrationRecord":
        request_payload = payload["request"]
        calibration_payload = payload["calibration"]
        validation_payload = payload["validation"]
        return cls(
            cache_key=str(payload["cache_key"]),
            request=PopulationCalibrationRequest(
                scenario_name=str(request_payload["scenario_name"]),
                objective=str(request_payload["objective"]),
                prompt_style=str(request_payload["prompt_style"]),
                allowed_targets=tuple(str(item) for item in request_payload["allowed_targets"]),
                population_total=None
                if request_payload.get("population_total") is None
                else int(request_payload["population_total"]),
                existing_cohorts=tuple(dict(item) for item in request_payload.get("existing_cohorts", [])),
                hazard_count=int(request_payload.get("hazard_count", 0)),
                responder_count=int(request_payload.get("responder_count", 0)),
                metadata_keys=tuple(str(item) for item in request_payload.get("metadata_keys", [])),
                persona_conditions=tuple(
                    str(item) for item in request_payload.get("persona_conditions", [])
                ),
            ),
            calibration=GeneratedPopulationCalibration(
                cohorts=[dict(item) for item in calibration_payload.get("cohorts", [])],
                parameter_priors={
                    str(name): {str(key): float(value) for key, value in values.items()}
                    for name, values in calibration_payload.get("parameter_priors", {}).items()
                    if isinstance(values, Mapping)
                },
                scenario_metadata=dict(calibration_payload.get("scenario_metadata", {})),
                notes=[str(item) for item in calibration_payload.get("notes", [])],
                confidence=float(calibration_payload.get("confidence", 0.0)),
                abstain=bool(calibration_payload.get("abstain", False)),
                provider=str(calibration_payload.get("provider", "deterministic")),
                model=str(calibration_payload.get("model", "template")),
                raw_response=dict(calibration_payload.get("raw_response", {})),
            ),
            validation=PopulationCalibrationValidation(
                accepted=bool(validation_payload["accepted"]),
                reasons=[str(item) for item in validation_payload.get("reasons", [])],
            ),
            application=dict(payload.get("application", {})),
        )


class PopulationCalibrationCache:
    def __init__(self, path: Path | str) -> None:
        self.path = Path(path)

    def key_for(self, request: PopulationCalibrationRequest) -> str:
        payload = json.dumps(_to_jsonable(request), sort_keys=True, separators=(",", ":"))
        return hashlib.sha256(payload.encode("utf-8")).hexdigest()

    def load(self, key: str) -> Optional[PopulationCalibrationRecord]:
        record_path = self.path / f"{key}.json"
        if not record_path.exists():
            return None
        return PopulationCalibrationRecord.from_json_dict(json.loads(record_path.read_text()))

    def store(self, record: PopulationCalibrationRecord) -> None:
        self.path.mkdir(parents=True, exist_ok=True)
        record_path = self.path / f"{record.cache_key}.json"
        record_path.write_text(json.dumps(record.to_json_dict(), indent=2, sort_keys=True) + "\n")


class PopulationCalibrationGenerator:
    provider = "base"
    model = "base"

    def generate(
        self,
        request: PopulationCalibrationRequest,
        cache_key: str,
    ) -> GeneratedPopulationCalibration:
        raise NotImplementedError


class ReplayPopulationCalibrationGenerator(PopulationCalibrationGenerator):
    provider = "cache"
    model = "replay_only"

    def __init__(self, cache: PopulationCalibrationCache) -> None:
        self.cache = cache

    def generate(
        self,
        request: PopulationCalibrationRequest,
        cache_key: str,
    ) -> GeneratedPopulationCalibration:
        record = self.cache.load(cache_key)
        if record is None:
            return GeneratedPopulationCalibration(
                notes=["No cached generated population calibration is available."],
                abstain=True,
                provider=self.provider,
                model=self.model,
            )
        return record.calibration


class TemplatePopulationCalibrationGenerator(PopulationCalibrationGenerator):
    provider = "deterministic"
    model = "template"

    def generate(
        self,
        request: PopulationCalibrationRequest,
        cache_key: str,
    ) -> GeneratedPopulationCalibration:
        allowed = set(request.allowed_targets)
        calibration = GeneratedPopulationCalibration(
            notes=[
                "Deterministic heuristic proposal for cache/replay plumbing.",
                "Use as an explicit prior only until matched external references exist.",
            ],
            confidence=0.7,
            provider=self.provider,
            model=self.model,
        )

        cohorts = list(request.existing_cohorts)
        if "cohort_mix" in allowed and not cohorts:
            total = int(request.population_total or 100)
            if request.persona_conditions:
                calibration.cohorts = _persona_conditioned_cohorts(
                    request.persona_conditions,
                    total=total,
                )
            else:
                regular_count = max(1, int(round(total * 0.7)))
                visitor_count = max(0, total - regular_count)
                calibration.cohorts = [
                    {
                        "name": "generated_regulars",
                        "count": regular_count,
                        "personality": "NORMAL",
                        "calmness": 0.8,
                        "familiarity": 0.7,
                        "group_size": 1,
                        "base_speed": 1.34,
                        "base_rationality": 0.8,
                        "credibility": 0.8,
                        "gossip_radius": 2.0,
                        "base_vision_radius": 5.0,
                    },
                    {
                        "name": "generated_visitors",
                        "count": visitor_count,
                        "personality": "NORMAL",
                        "calmness": 0.6,
                        "familiarity": 0.1,
                        "group_size": 2,
                        "base_speed": 1.2,
                        "base_rationality": 0.6,
                        "credibility": 0.65,
                        "gossip_radius": 1.6,
                        "base_vision_radius": 4.5,
                    },
                ]
            cohorts = calibration.cohorts

        if "parameter_priors" in allowed:
            calibration.parameter_priors = {
                str(cohort.get("name", f"cohort_{index + 1}")): _template_priors(cohort)
                for index, cohort in enumerate(cohorts)
            }

        if "scenario_metadata" in allowed:
            calibration.scenario_metadata = {
                "population_calibration_status": "generated_heuristic_prior",
                "population_calibration_objective": request.objective,
                "population_calibration_warning": (
                    "Generated values are priors only and are not matched to a real "
                    "trajectory, drill, VR, incident, or expert-coded reference."
                ),
            }

        return calibration


class OpenAIPopulationCalibrationGenerator(PopulationCalibrationGenerator):
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
        self,
        request: PopulationCalibrationRequest,
        cache_key: str,
    ) -> GeneratedPopulationCalibration:
        if not self.api_key:
            return GeneratedPopulationCalibration(
                notes=["OpenAI API key is not configured."],
                abstain=True,
                provider=self.provider,
                model=self.model,
                raw_response={"error": "missing_api_key"},
            )

        payload = {
            "model": self.model,
            "store": False,
            "max_output_tokens": 900,
            "instructions": _population_calibration_instructions(),
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
            return self._error_calibration(f"http_{exc.code}")
        except (URLError, TimeoutError) as exc:
            return self._error_calibration(type(exc).__name__)

        text = _extract_response_text(response_payload)
        parsed = _parse_json_object(text)
        if parsed is None:
            return GeneratedPopulationCalibration(
                notes=["OpenAI response was not parseable JSON."],
                abstain=True,
                provider=self.provider,
                model=self.model,
                raw_response={"unparsed_response": response_payload},
            )
        return _calibration_from_payload(parsed, provider=self.provider, model=self.model, raw_response=response_payload)

    def _error_calibration(self, error: str) -> GeneratedPopulationCalibration:
        return GeneratedPopulationCalibration(
            notes=["OpenAI generated population calibration failed."],
            abstain=True,
            provider=self.provider,
            model=self.model,
            raw_response={"error": error},
        )


class AnthropicPopulationCalibrationGenerator(PopulationCalibrationGenerator):
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
        self,
        request: PopulationCalibrationRequest,
        cache_key: str,
    ) -> GeneratedPopulationCalibration:
        if not self.api_key:
            return GeneratedPopulationCalibration(
                notes=["Anthropic API key is not configured."],
                abstain=True,
                provider=self.provider,
                model=self.model,
                raw_response={"error": "missing_api_key"},
            )

        payload = {
            "model": self.model,
            "max_tokens": 900,
            "system": _population_calibration_instructions(),
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
            return self._error_calibration(f"http_{exc.code}")
        except (URLError, TimeoutError) as exc:
            return self._error_calibration(type(exc).__name__)

        parsed = _parse_json_object(_extract_anthropic_text(response_payload))
        if parsed is None:
            return GeneratedPopulationCalibration(
                notes=["Anthropic response was not parseable JSON."],
                abstain=True,
                provider=self.provider,
                model=self.model,
                raw_response={"unparsed_response": response_payload},
            )
        return _calibration_from_payload(parsed, provider=self.provider, model=self.model, raw_response=response_payload)

    def _error_calibration(self, error: str) -> GeneratedPopulationCalibration:
        return GeneratedPopulationCalibration(
            notes=["Anthropic generated population calibration failed."],
            abstain=True,
            provider=self.provider,
            model=self.model,
            raw_response={"error": error},
        )


def apply_generated_population_calibration(scenario: Mapping[str, Any]) -> Dict[str, Any]:
    updated = deepcopy(dict(scenario))
    config = PopulationCalibrationConfig.from_mapping(updated.get("generated_population_calibration"))
    if not config.enabled:
        return updated

    cache = PopulationCalibrationCache(config.cache_path) if config.cache_path else None
    provider = config.provider.lower()
    config = PopulationCalibrationConfig(**{**config.__dict__, "provider": provider})
    budget_guard = LLMBudgetGuard(
        max_calls=config.max_calls_per_run,
        max_estimated_tokens=config.max_estimated_tokens_per_run,
        max_estimated_usd=config.max_estimated_usd_per_run,
        input_usd_per_mtok=config.input_usd_per_mtok,
        output_usd_per_mtok=config.output_usd_per_mtok,
    )
    if provider == "template":
        generator: PopulationCalibrationGenerator = TemplatePopulationCalibrationGenerator()
    elif provider in {"replay", "local_replay"}:
        if cache is None:
            raise ValueError("generated_population_calibration replay provider requires cache_path")
        config = PopulationCalibrationConfig(
            **{**config.__dict__, "cache_mode": "replay_only"}
        )
        generator = ReplayPopulationCalibrationGenerator(cache)
    elif provider == "openai":
        model = config.model if config.model and config.model != "template" else load_openai_model()
        generator = OpenAIPopulationCalibrationGenerator(model=model)
    elif provider == "anthropic":
        model = config.model if config.model and config.model != "template" else load_anthropic_model()
        generator = AnthropicPopulationCalibrationGenerator(model=model)
    else:
        raise ValueError("Unsupported generated population calibration provider")

    request = build_population_calibration_request(updated, config)
    cache_key = cache.key_for(request) if cache is not None else ""
    calibration, cached_validation, cache_status, budget_audit = _generate_calibration(
        generator,
        cache,
        config,
        request,
        cache_key,
        budget_guard,
    )
    validation = validate_generated_population_calibration(calibration, request, config)
    if cached_validation is not None and cached_validation.reasons == validation.reasons:
        validation = cached_validation

    application = _apply_calibration_payload(updated, calibration, validation, cache_key) if validation.accepted else {
        "applied_targets": [],
        "skipped": [],
    }
    _attach_calibration_audit(
        updated,
        config=config,
        calibration=calibration,
        validation=validation,
        cache_key=cache_key,
        cache_status=cache_status,
        application=application,
        budget_audit=budget_audit,
    )

    if (
        cache is not None
        and config.store_cache
        and config.provider not in {"replay", "local_replay"}
        and cache_status != "hit"
        and cache_status != "budget_exceeded"
    ):
        cache.store(
            PopulationCalibrationRecord(
                cache_key=cache_key,
                request=request,
                calibration=calibration,
                validation=validation,
                application=dict(application),
            )
        )

    return updated


def build_population_calibration_request(
    scenario: Mapping[str, Any],
    config: PopulationCalibrationConfig,
) -> PopulationCalibrationRequest:
    population = scenario.get("population", {}) or {}
    cohorts = tuple(_cohort_request_summary(cohort) for cohort in population.get("cohorts", []) or [])
    return PopulationCalibrationRequest(
        scenario_name=str(scenario.get("name", "unnamed_scenario")),
        objective=config.objective,
        prompt_style=config.prompt_style,
        allowed_targets=tuple(config.allowed_targets),
        population_total=None if population.get("total") is None else int(population.get("total")),
        existing_cohorts=cohorts,
        hazard_count=len(scenario.get("hazards", []) or []),
        responder_count=sum(int(item.get("count", 1)) for item in scenario.get("responders", []) or []),
        metadata_keys=tuple(sorted(str(key) for key in (scenario.get("metadata", {}) or {}).keys())),
        persona_conditions=tuple(config.persona_conditions),
    )


def validate_generated_population_calibration(
    calibration: GeneratedPopulationCalibration,
    request: PopulationCalibrationRequest,
    config: PopulationCalibrationConfig,
) -> PopulationCalibrationValidation:
    reasons: List[str] = []
    allowed = set(request.allowed_targets)
    existing_names = {str(cohort.get("name")) for cohort in request.existing_cohorts}

    if calibration.abstain:
        reasons.append("generator_abstained")
    if calibration.confidence < config.min_confidence and not calibration.abstain:
        reasons.append(f"low_confidence:{calibration.confidence:.2f}")
    if calibration.cohorts and "cohort_mix" not in allowed:
        reasons.append("disallowed_target:cohort_mix")
    if calibration.parameter_priors and "parameter_priors" not in allowed:
        reasons.append("disallowed_target:parameter_priors")
    if calibration.scenario_metadata and "scenario_metadata" not in allowed:
        reasons.append("disallowed_target:scenario_metadata")

    if calibration.cohorts:
        total = 0
        persona_set = set(request.persona_conditions)
        for index, cohort in enumerate(calibration.cohorts):
            name = str(cohort.get("name", "")).strip()
            if not name:
                reasons.append(f"cohort_{index}_missing_name")
            persona = cohort.get("persona_condition")
            if persona is not None and str(persona) not in persona_set:
                reasons.append(f"cohort_{name}_unknown_persona:{persona}")
            count = cohort.get("count")
            if count is not None:
                try:
                    count_int = int(count)
                except (TypeError, ValueError):
                    reasons.append(f"cohort_{name}_invalid_count")
                    count_int = 0
                if count_int < 0:
                    reasons.append(f"cohort_{name}_negative_count")
                total += max(0, count_int)
            for field_name, value in cohort.items():
                if field_name in SAFE_COHORT_FIELDS:
                    continue
                if field_name in CALIBRATION_PARAMETER_BOUNDS:
                    _validate_parameter_value(reasons, f"cohort_{name}", field_name, value)
        if request.population_total is not None and total > 0 and total != request.population_total:
            reasons.append(f"cohort_count_total_mismatch:{total}!={request.population_total}")

    for cohort_name, priors in calibration.parameter_priors.items():
        if existing_names and cohort_name not in existing_names:
            reasons.append(f"unknown_cohort_prior:{cohort_name}")
        for field_name, value in priors.items():
            if field_name not in CALIBRATION_PARAMETER_BOUNDS:
                reasons.append(f"unsupported_parameter_prior:{field_name}")
                continue
            _validate_parameter_value(reasons, f"prior_{cohort_name}", field_name, value)

    return PopulationCalibrationValidation(accepted=not reasons, reasons=reasons)


def _generate_calibration(
    generator: PopulationCalibrationGenerator,
    cache: Optional[PopulationCalibrationCache],
    config: PopulationCalibrationConfig,
    request: PopulationCalibrationRequest,
    cache_key: str,
    budget_guard: LLMBudgetGuard,
) -> Tuple[GeneratedPopulationCalibration, Optional[PopulationCalibrationValidation], str, Dict[str, Any]]:
    if cache is None:
        audit = _population_budget_check(config, request, budget_guard)
        if not audit["allowed"]:
            return _budget_exceeded_calibration(audit), None, "budget_exceeded", audit
        budget_guard.record(audit["check"])
        return generator.generate(request, cache_key), None, "disabled", audit

    cached = cache.load(cache_key)
    if cached is not None and config.cache_mode in {"cache_first", "replay_only"}:
        return cached.calibration, cached.validation, "hit", _cached_population_audit(
            config,
            cached.calibration,
        )

    if config.cache_mode == "replay_only":
        return generator.generate(request, cache_key), None, "miss", _empty_budget_audit()

    audit = _population_budget_check(config, request, budget_guard)
    if not audit["allowed"]:
        return _budget_exceeded_calibration(audit), None, "budget_exceeded", audit
    budget_guard.record(audit["check"])
    return generator.generate(request, cache_key), None, "miss", audit


def _population_budget_check(
    config: PopulationCalibrationConfig,
    request: PopulationCalibrationRequest,
    budget_guard: LLMBudgetGuard,
) -> Dict[str, Any]:
    input_tokens = estimate_llm_tokens(
        {
            "instructions": _population_calibration_instructions(),
            "input": _to_jsonable(request),
        },
        output_tokens=0,
    )
    output_tokens = 900
    check = budget_guard.evaluate(input_tokens, output_tokens)
    return {
        "allowed": check.allowed,
        "budget_reason": check.reason,
        "estimated_input_tokens": check.estimated_input_tokens,
        "estimated_output_tokens": check.estimated_output_tokens,
        "estimated_total_tokens": check.estimated_total_tokens,
        "estimated_usd": check.estimated_usd,
        "check": check,
    }


def _cached_population_audit(
    config: PopulationCalibrationConfig,
    calibration: GeneratedPopulationCalibration,
) -> Dict[str, Any]:
    usage = raw_usage_tokens(calibration.raw_response)
    return {
        "allowed": True,
        "budget_reason": "cache_hit",
        "estimated_input_tokens": usage["input_tokens"],
        "estimated_output_tokens": usage["output_tokens"],
        "estimated_total_tokens": usage["total_tokens"],
        "estimated_usd": estimate_llm_cost(
            input_tokens=usage["input_tokens"],
            output_tokens=usage["output_tokens"],
            input_usd_per_mtok=config.input_usd_per_mtok,
            output_usd_per_mtok=config.output_usd_per_mtok,
        ),
    }


def _empty_budget_audit() -> Dict[str, Any]:
    return {
        "allowed": True,
        "budget_reason": "",
        "estimated_input_tokens": 0,
        "estimated_output_tokens": 0,
        "estimated_total_tokens": 0,
        "estimated_usd": 0.0,
    }


def _budget_exceeded_calibration(audit: Mapping[str, Any]) -> GeneratedPopulationCalibration:
    return GeneratedPopulationCalibration(
        notes=["LLM budget guard blocked generated population calibration."],
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


def _apply_calibration_payload(
    scenario: Dict[str, Any],
    calibration: GeneratedPopulationCalibration,
    validation: PopulationCalibrationValidation,
    cache_key: str,
) -> Dict[str, Any]:
    if not validation.accepted:
        return {"applied_targets": [], "skipped": []}

    population = scenario.setdefault("population", {})
    applied: List[str] = []
    skipped: List[str] = []

    if calibration.cohorts:
        existing_cohorts = list(population.get("cohorts", []) or [])
        if existing_cohorts:
            skipped.append("cohort_mix:existing_cohorts_present")
        else:
            generated = []
            for cohort in calibration.cohorts:
                prepared = dict(cohort)
                prepared.setdefault("calibration_status", "generated_heuristic_prior")
                prepared.setdefault("parameter_provenance", {})
                for field_name in CALIBRATION_PARAMETER_BOUNDS:
                    if field_name in prepared:
                        prepared["parameter_provenance"][field_name] = f"generated:{cache_key}"
                generated.append(prepared)
            population["cohorts"] = generated
            if "total" not in population:
                population["total"] = sum(int(cohort.get("count", 0)) for cohort in generated)
            applied.append("cohort_mix")

    if calibration.parameter_priors:
        for cohort in population.get("cohorts", []) or []:
            cohort_name = str(cohort.get("name", ""))
            priors = calibration.parameter_priors.get(cohort_name)
            if not priors:
                continue
            provenance = cohort.setdefault("parameter_provenance", {})
            for field_name, value in priors.items():
                if field_name in cohort:
                    skipped.append(f"parameter_priors:{cohort_name}.{field_name}:existing_value")
                    continue
                cohort[field_name] = float(value)
                provenance[field_name] = f"generated:{cache_key}"
                applied.append(f"parameter_priors:{cohort_name}.{field_name}")

    if calibration.scenario_metadata:
        metadata = scenario.setdefault("metadata", {})
        generated_metadata = metadata.setdefault("generated_population", {})
        for key, value in calibration.scenario_metadata.items():
            if key in generated_metadata:
                skipped.append(f"scenario_metadata:{key}:existing_value")
                continue
            generated_metadata[key] = value
            applied.append(f"scenario_metadata:{key}")

    return {
        "applied_targets": applied,
        "skipped": skipped,
    }


def _attach_calibration_audit(
    scenario: Dict[str, Any],
    *,
    config: PopulationCalibrationConfig,
    calibration: GeneratedPopulationCalibration,
    validation: PopulationCalibrationValidation,
    cache_key: str,
    cache_status: str,
    application: Mapping[str, Any],
    budget_audit: Mapping[str, Any],
) -> None:
    metadata = scenario.setdefault("metadata", {})
    usage = raw_usage_tokens(calibration.raw_response)
    metadata["generated_population_calibration_audit"] = {
        "provider": calibration.provider,
        "model": calibration.model,
        "cache_key": cache_key,
        "cache_status": cache_status,
        "validation_status": validation.status,
        "validation_reasons": list(validation.reasons),
        "allowed_targets": list(config.allowed_targets),
        "overwrite_policy": config.overwrite_policy,
        "persona_conditions": list(config.persona_conditions),
        "applied_targets": list(application.get("applied_targets", [])),
        "skipped": list(application.get("skipped", [])),
        "estimated_input_tokens": int(budget_audit.get("estimated_input_tokens", 0)),
        "estimated_output_tokens": int(budget_audit.get("estimated_output_tokens", 0)),
        "estimated_total_tokens": int(budget_audit.get("estimated_total_tokens", 0)),
        "estimated_usd": float(budget_audit.get("estimated_usd", 0.0)),
        "budget_reason": str(budget_audit.get("budget_reason", "")),
        "raw_input_tokens": usage["input_tokens"],
        "raw_output_tokens": usage["output_tokens"],
        "raw_total_tokens": usage["total_tokens"],
    }


def _cohort_request_summary(cohort: Mapping[str, Any]) -> Dict[str, Any]:
    summary: Dict[str, Any] = {
        "name": str(cohort.get("name", "")),
        "count": cohort.get("count"),
    }
    for field_name in CALIBRATION_PARAMETER_BOUNDS:
        if field_name in cohort:
            summary[field_name] = cohort[field_name]
    if "group_size" in cohort:
        summary["group_size"] = cohort["group_size"]
    if "personality" in cohort:
        summary["personality"] = cohort["personality"]
    for field_name in SAFE_COHORT_FIELDS:
        if field_name in cohort and field_name not in summary:
            summary[field_name] = cohort[field_name]
    if "calibration_status" in cohort:
        summary["calibration_status"] = cohort["calibration_status"]
    return summary


def _persona_conditioned_cohorts(
    persona_conditions: Sequence[str],
    *,
    total: int,
) -> List[Dict[str, Any]]:
    personas = [str(item) for item in persona_conditions if str(item).strip()]
    if not personas:
        return []
    base = total // len(personas)
    remainder = total % len(personas)
    cohorts = []
    for index, persona in enumerate(personas):
        count = base + (1 if index < remainder else 0)
        cohorts.append(_cohort_from_persona(persona, count=count))
    return cohorts


def _cohort_from_persona(persona: str, *, count: int) -> Dict[str, Any]:
    normalized = "_".join(persona.lower().replace("-", "_").split())
    tokens = set(normalized.split("_"))
    visitor_like = bool(tokens & {"visitor", "tourist", "guest", "unfamiliar"})
    regular_like = bool(tokens & {"regular", "commuter", "staff", "worker"})
    family_like = bool(tokens & {"family", "parent", "child", "caregiver"})
    mobility_class = "standard"
    if "wheelchair" in tokens:
        mobility_class = "wheelchair"
    elif "walker" in tokens:
        mobility_class = "walker"
    elif "visual" in tokens or "blind" in tokens:
        mobility_class = "visual-impairment"
    cohort = {
        "name": f"generated_{normalized or 'persona'}",
        "count": int(count),
        "personality": "NORMAL",
        "persona_condition": persona,
        "mobility_class": mobility_class,
        "calmness": 0.65,
        "familiarity": 0.5,
        "group_size": 2 if family_like else 1,
        "base_speed": 1.2 if visitor_like else 1.34,
        "base_rationality": 0.6 if visitor_like else 0.8,
        "credibility": 0.65 if visitor_like else 0.8,
        "gossip_radius": 1.6 if visitor_like else 2.0,
        "base_vision_radius": 4.5 if visitor_like else 5.0,
        "homophily_profile": {"persona": normalized, "mobility_class": mobility_class},
        "homophily_weight": 0.6 if family_like else 0.35,
        "exit_affinity": 0.45 if visitor_like else 0.65,
        "herding": 0.65 if family_like or visitor_like else 0.45,
        "separation_anxiety_threshold": 2.5 if family_like else 1.5,
    }
    if regular_like:
        cohort["familiarity"] = 0.75
        cohort["calmness"] = 0.78
    if mobility_class == "wheelchair":
        cohort["breathing_height_m"] = 1.1
    elif mobility_class == "walker":
        cohort["breathing_height_m"] = 1.35
    else:
        cohort["breathing_height_m"] = 1.5
    return cohort


def _template_priors(cohort: Mapping[str, Any]) -> Dict[str, float]:
    name = str(cohort.get("name", "")).lower()
    visitor_like = any(token in name for token in ("visitor", "tourist", "unfamiliar"))
    if visitor_like:
        return {
            "base_speed": 1.2,
            "base_rationality": 0.6,
            "credibility": 0.65,
            "gossip_radius": 1.6,
            "base_vision_radius": 4.5,
        }
    return {
        "base_speed": 1.34,
        "base_rationality": 0.8,
        "credibility": 0.8,
        "gossip_radius": 2.0,
        "base_vision_radius": 5.0,
    }


def _validate_parameter_value(
    reasons: List[str],
    prefix: str,
    field_name: str,
    value: Any,
) -> None:
    low, high = CALIBRATION_PARAMETER_BOUNDS[field_name]
    try:
        number = float(value)
    except (TypeError, ValueError):
        reasons.append(f"{prefix}_{field_name}_not_numeric")
        return
    if not low <= number <= high:
        reasons.append(f"{prefix}_{field_name}_out_of_range:{number}")


def _population_calibration_instructions() -> str:
    return (
        "You propose bounded population-calibration priors for a research "
        "evacuation simulator. Return only JSON with keys: cohorts, "
        "parameter_priors, scenario_metadata, notes, confidence, abstain. "
        "Only use targets listed in allowed_targets. Do not overwrite existing "
        "cohort fields; propose priors only for missing values. When "
        "persona_conditions are present, cohorts must use only those exact "
        "persona_condition values. Do not claim that generated priors are "
        "measured or validated."
    )


def _calibration_from_payload(
    payload: Mapping[str, Any],
    *,
    provider: str,
    model: str,
    raw_response: Mapping[str, Any],
) -> GeneratedPopulationCalibration:
    return GeneratedPopulationCalibration(
        cohorts=[dict(item) for item in payload.get("cohorts", []) if isinstance(item, Mapping)],
        parameter_priors={
            str(name): {
                str(key): float(value)
                for key, value in values.items()
                if isinstance(value, (int, float))
            }
            for name, values in (payload.get("parameter_priors", {}) or {}).items()
            if isinstance(values, Mapping)
        },
        scenario_metadata=dict(payload.get("scenario_metadata", {}) or {}),
        notes=[str(item) for item in payload.get("notes", []) or []],
        confidence=float(payload.get("confidence", 0.0) or 0.0),
        abstain=bool(payload.get("abstain", False)),
        provider=provider,
        model=model,
        raw_response={"id": raw_response.get("id"), "usage": raw_response.get("usage")},
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


def _extract_response_text(payload: Mapping[str, Any]) -> str:
    if isinstance(payload.get("output_text"), str):
        return str(payload["output_text"])
    chunks: List[str] = []
    for item in payload.get("output", []):
        for content in item.get("content", []):
            if isinstance(content.get("text"), str):
                chunks.append(content["text"])
    return "\n".join(chunks)


def _parse_json_object(text: str) -> Optional[Dict[str, Any]]:
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
