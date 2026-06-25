"""Scenario configuration and loading."""

from chiyoda.scenarios.calibration_audit import (
    build_calibration_audit,
    calibration_audit_file,
)
from chiyoda.scenarios.generated_calibration import (
    AnthropicPopulationCalibrationGenerator,
    GeneratedPopulationCalibration,
    PopulationCalibrationCache,
    PopulationCalibrationConfig,
    PopulationCalibrationRecord,
    PopulationCalibrationRequest,
    PopulationCalibrationValidation,
    TemplatePopulationCalibrationGenerator,
    apply_generated_population_calibration,
    validate_generated_population_calibration,
)
from chiyoda.scenarios.geometry_audit import build_geometry_audit, geometry_audit_file
from chiyoda.scenarios.hazard_audit import build_hazard_audit, hazard_audit_file
from chiyoda.scenarios.ifc_import import (
    strict_layout_and_metadata_from_ifc,
    strict_layout_from_ifc,
    strict_scenario_from_ifc,
)
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios.patching import (
    apply_exported_patch_file,
    apply_json_patch,
    canonical_scenario_bytes,
    exported_scenario_body,
)
from chiyoda.scenarios.validation import (
    ScenarioValidationIssue,
    ScenarioValidationResult,
    validate_scenario_config,
    validate_scenario_file,
)
from chiyoda.scenarios.validation_evidence_audit import (
    build_validation_evidence_audit,
    validation_evidence_audit_file,
)

__all__ = [
    "GeneratedPopulationCalibration",
    "AnthropicPopulationCalibrationGenerator",
    "PopulationCalibrationCache",
    "PopulationCalibrationConfig",
    "PopulationCalibrationRecord",
    "PopulationCalibrationRequest",
    "PopulationCalibrationValidation",
    "ScenarioManager",
    "ScenarioValidationIssue",
    "ScenarioValidationResult",
    "TemplatePopulationCalibrationGenerator",
    "apply_exported_patch_file",
    "apply_generated_population_calibration",
    "apply_json_patch",
    "build_geometry_audit",
    "build_calibration_audit",
    "build_hazard_audit",
    "build_validation_evidence_audit",
    "calibration_audit_file",
    "canonical_scenario_bytes",
    "exported_scenario_body",
    "geometry_audit_file",
    "hazard_audit_file",
    "strict_layout_and_metadata_from_ifc",
    "strict_layout_from_ifc",
    "strict_scenario_from_ifc",
    "validate_scenario_config",
    "validation_evidence_audit_file",
    "validate_scenario_file",
    "validate_generated_population_calibration",
]
