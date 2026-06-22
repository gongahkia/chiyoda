from __future__ import annotations

import json
import logging

from chiyoda._logging import get_logger, log_event
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.studies.runner import run_study
from chiyoda.studies.schema import StudyConfig, StudyVariant


def test_log_event_emits_json_when_enabled(monkeypatch, capsys):
    logger_name = "chiyoda.test.json"
    _reset_logger(logger_name)
    monkeypatch.setenv("CHIYODA_LOG_FORMAT", "json")

    logger = get_logger(logger_name)
    log_event(logger, "unit.event", step=3)

    payload = json.loads(capsys.readouterr().err)
    assert payload["event"] == "unit.event"
    assert payload["step"] == 3
    _reset_logger(logger_name)


def test_simulation_emits_step_events(monkeypatch, capsys):
    _reset_logger("chiyoda")
    monkeypatch.setenv("CHIYODA_LOG_FORMAT", "json")
    simulation = ScenarioManager().build_simulation(
        {
            "name": "logging_smoke",
            "layout": {
                "floors": [
                    {
                        "id": "0",
                        "z": 0.0,
                        "text": "XXXXX\nX@.EX\nXXXXX",
                    }
                ]
            },
            "population": {"total": 1},
            "simulation": {"max_steps": 1, "random_seed": 3},
        }
    )

    simulation.run()

    events = [
        json.loads(line)["event"]
        for line in capsys.readouterr().err.splitlines()
        if line.strip()
    ]
    assert "simulation.run.start" in events
    assert "simulation.step.start" in events
    assert "simulation.step.end" in events
    assert "simulation.run.end" in events
    _reset_logger("chiyoda")


def test_run_study_emits_stage_events(monkeypatch, capsys, tmp_path):
    _reset_logger("chiyoda")
    monkeypatch.setenv("CHIYODA_LOG_FORMAT", "json")
    scenario_file = tmp_path / "scenario.yaml"
    scenario_file.write_text(
        """
scenario:
  name: logging_study
  layout:
    floors:
      - id: "0"
        z: 0.0
        text: |
          XXXXX
          X@.EX
          XXXXX
  population:
    total: 1
  simulation:
    max_steps: 1
    random_seed: 4
"""
    )

    run_study(
        StudyConfig(
            name="logging_study",
            scenario_file=str(scenario_file),
            seeds=[4],
            variants=[StudyVariant(name="base")],
        )
    )

    events = [
        json.loads(line)["event"]
        for line in capsys.readouterr().err.splitlines()
        if line.strip()
    ]
    assert "study.run.start" in events
    assert "study.variant.start" in events
    assert "study.seed_run.start" in events
    assert "study.seed_run.end" in events
    assert "study.variant.end" in events
    assert "study.run.complete" in events
    _reset_logger("chiyoda")


def _reset_logger(name: str) -> None:
    logger = logging.getLogger(name)
    for handler in list(logger.handlers):
        logger.removeHandler(handler)
        handler.close()
    if hasattr(logger, "_chiyoda_configured"):
        delattr(logger, "_chiyoda_configured")
