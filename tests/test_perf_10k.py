from __future__ import annotations

import os
import time

import pytest

from chiyoda.scenarios.manager import ScenarioManager


@pytest.mark.slow
def test_transit_cbrn_10k_clock_budget():
    budget_s = float(os.environ.get("CHIYODA_10K_BUDGET_S", "60"))
    start = time.perf_counter()
    simulation = ScenarioManager().load_scenario(
        "scenarios/benchmark/transit_cbrn_10k.yaml"
    )
    simulation.run()
    elapsed_s = time.perf_counter() - start

    assert len(simulation.agents) == 10000
    assert simulation.current_step == 1
    assert elapsed_s <= budget_s
