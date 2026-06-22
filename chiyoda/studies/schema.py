from __future__ import annotations

from typing import Any, Literal, Union

from pydantic import BaseModel, Field, field_validator, model_validator

Cell = Union[tuple[int, int], tuple[str, int, int], dict[str, Any]]


class ExportConfig(BaseModel):
    profile: str = "report"
    formats: list[str] = Field(default_factory=lambda: ["png", "svg"])
    table_formats: list[str] = Field(default_factory=lambda: ["parquet", "csv"])
    include_figures: bool = True

    @field_validator("formats")
    @classmethod
    def validate_formats(cls, values: list[str]) -> list[str]:
        allowed = {"png", "svg", "pdf"}
        normalized = [value.lower() for value in values]
        invalid = [value for value in normalized if value not in allowed]
        if invalid:
            raise ValueError(f"Unsupported figure formats: {invalid}")
        return normalized

    @field_validator("table_formats")
    @classmethod
    def validate_table_formats(cls, values: list[str]) -> list[str]:
        allowed = {"parquet", "csv"}
        normalized = [value.lower() for value in values]
        invalid = [value for value in normalized if value not in allowed]
        if invalid:
            raise ValueError(f"Unsupported table formats: {invalid}")
        return normalized

    @model_validator(mode="after")
    def validate_export_targets(self) -> ExportConfig:
        if not self.table_formats:
            raise ValueError("At least one table format is required")
        if self.include_figures and not self.formats:
            raise ValueError(
                "At least one figure format is required when include_figures is enabled"
            )
        return self


class InterventionConfig(BaseModel):
    type: Literal[
        "corridor_narrowing",
        "corridor_widening",
        "block_cells",
        "clear_cells",
        "exit_closure",
        "staggered_release",
        "demand_surge",
    ]
    name: str | None = None
    cells: list[Cell] = Field(default_factory=list)
    exits: list[Cell] = Field(default_factory=list)
    cohort: str | None = None
    release_step: int | None = None
    count: int | None = None
    personality: str = "NORMAL"
    calmness: float = 0.8
    base_speed_multiplier: float = 1.0
    group_size: int = 1
    spawn_cells: list[Cell] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_shape(self) -> InterventionConfig:
        if (
            self.type
            in {
                "corridor_narrowing",
                "corridor_widening",
                "block_cells",
                "clear_cells",
            }
            and not self.cells
        ):
            raise ValueError(f"{self.type} requires a non-empty cells list")
        if self.type == "exit_closure" and not self.exits:
            raise ValueError("exit_closure requires an exits list")
        if self.type == "demand_surge" and (
            self.count is None or self.release_step is None
        ):
            raise ValueError("demand_surge requires count and release_step")
        if self.type == "staggered_release" and self.release_step is None:
            raise ValueError("staggered_release requires release_step")
        return self


class SweepParameter(BaseModel):
    path: str
    values: list[Any]
    label: str | None = None

    @field_validator("values")
    @classmethod
    def validate_values(cls, values: list[Any]) -> list[Any]:
        if not values:
            raise ValueError("Sweep values must not be empty")
        return values


class StudyVariant(BaseModel):
    name: str
    description: str | None = None
    interventions: list[InterventionConfig] = Field(default_factory=list)
    scenario_overrides: dict[str, Any] = Field(default_factory=dict)
    seeds: list[int] | None = None


class AdversarialStudyConfig(BaseModel):
    attacker_budget: list[int] = Field(default_factory=list)
    defender_policy: list[str] = Field(default_factory=list)
    pairing: Literal["stackelberg", "grid"] = "stackelberg"
    hostile_channel_index: int = 0

    @model_validator(mode="after")
    def validate_pairing(self) -> AdversarialStudyConfig:
        if self.attacker_budget and not self.defender_policy:
            raise ValueError(
                "adversarial.defender_policy is required when attacker_budget is set"
            )
        if self.defender_policy and not self.attacker_budget:
            raise ValueError(
                "adversarial.attacker_budget is required when defender_policy is set"
            )
        if self.hostile_channel_index < 0:
            raise ValueError("hostile_channel_index must be non-negative")
        return self


class StudyConfig(BaseModel):
    name: str
    scenario_file: str
    description: str | None = None
    seeds: list[int] = Field(default_factory=list)
    treatment_assignments: dict[int, str] = Field(default_factory=dict)
    repetitions: int = 1
    jobs: int = 1
    variants: list[StudyVariant] = Field(default_factory=list)
    sweep: list[SweepParameter] = Field(default_factory=list)
    adversarial: AdversarialStudyConfig | None = None
    export: ExportConfig = Field(default_factory=ExportConfig)

    @model_validator(mode="after")
    def validate_execution(self) -> StudyConfig:
        if self.repetitions < 1:
            raise ValueError("repetitions must be at least 1")
        if self.jobs < 1:
            raise ValueError("jobs must be at least 1")
        return self
