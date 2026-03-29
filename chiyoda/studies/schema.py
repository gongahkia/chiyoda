from __future__ import annotations

from typing import Any, Dict, List, Literal, Optional, Tuple

from pydantic import BaseModel, Field, field_validator, model_validator


Cell = Tuple[int, int]


class ExportConfig(BaseModel):
    profile: str = "paper"
    formats: List[str] = Field(default_factory=lambda: ["png", "svg", "pdf"])
    table_formats: List[str] = Field(default_factory=lambda: ["parquet", "csv"])
    include_figures: bool = True

    @field_validator("formats")
    @classmethod
    def validate_formats(cls, values: List[str]) -> List[str]:
        allowed = {"png", "svg", "pdf"}
        normalized = [value.lower() for value in values]
        invalid = [value for value in normalized if value not in allowed]
        if invalid:
            raise ValueError(f"Unsupported figure formats: {invalid}")
        return normalized

    @field_validator("table_formats")
    @classmethod
    def validate_table_formats(cls, values: List[str]) -> List[str]:
        allowed = {"parquet", "csv"}
        normalized = [value.lower() for value in values]
        invalid = [value for value in normalized if value not in allowed]
        if invalid:
            raise ValueError(f"Unsupported table formats: {invalid}")
        return normalized

    @model_validator(mode="after")
    def validate_export_targets(self) -> "ExportConfig":
        if not self.table_formats:
            raise ValueError("At least one table format is required")
        if self.include_figures and not self.formats:
            raise ValueError("At least one figure format is required when include_figures is enabled")
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
    name: Optional[str] = None
    cells: List[Cell] = Field(default_factory=list)
    exits: List[Cell] = Field(default_factory=list)
    cohort: Optional[str] = None
    release_step: Optional[int] = None
    count: Optional[int] = None
    personality: str = "NORMAL"
    calmness: float = 0.8
    base_speed_multiplier: float = 1.0
    group_size: int = 1
    spawn_cells: List[Cell] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_shape(self) -> "InterventionConfig":
        if self.type in {
            "corridor_narrowing",
            "corridor_widening",
            "block_cells",
            "clear_cells",
        } and not self.cells:
            raise ValueError(f"{self.type} requires a non-empty cells list")
        if self.type == "exit_closure" and not self.exits:
            raise ValueError("exit_closure requires an exits list")
        if self.type == "demand_surge" and (self.count is None or self.release_step is None):
            raise ValueError("demand_surge requires count and release_step")
        if self.type == "staggered_release" and self.release_step is None:
            raise ValueError("staggered_release requires release_step")
        return self


class SweepParameter(BaseModel):
    path: str
    values: List[Any]
    label: Optional[str] = None

    @field_validator("values")
    @classmethod
    def validate_values(cls, values: List[Any]) -> List[Any]:
        if not values:
            raise ValueError("Sweep values must not be empty")
        return values


class StudyVariant(BaseModel):
    name: str
    description: Optional[str] = None
    interventions: List[InterventionConfig] = Field(default_factory=list)
    scenario_overrides: Dict[str, Any] = Field(default_factory=dict)
    seeds: Optional[List[int]] = None


class StudyConfig(BaseModel):
    name: str
    scenario_file: str
    description: Optional[str] = None
    seeds: List[int] = Field(default_factory=list)
    repetitions: int = 1
    variants: List[StudyVariant] = Field(default_factory=list)
    sweep: List[SweepParameter] = Field(default_factory=list)
    export: ExportConfig = Field(default_factory=ExportConfig)

    @model_validator(mode="after")
    def validate_execution(self) -> "StudyConfig":
        if self.repetitions < 1:
            raise ValueError("repetitions must be at least 1")
        return self
