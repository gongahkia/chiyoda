# Trajectory Reference Workflow

Chiyoda exports `agent_steps` tables for comparison with external pedestrian
analysis tools. The project keeps first-order summary metrics in
`chiyoda.analysis.trajectory_reference` and leaves full trajectory science to
specialist packages such as PedPy, JuPedSim, or Vadere.

## CI Fixture

`tests/fixtures/trajectories/ci_corridor_reference.csv` is a small
license-compatible synthetic reference fixture for regression tests. Its
metadata file records the CC0 license and the limitation that it is not
real-world validation evidence.

## External Tool Exports

Use the Python helpers when comparing Chiyoda output with established tools:

```python
from chiyoda.analysis.trajectory_reference import (
    export_jupedsim_trajectory,
    export_vadere_trajectory,
    load_trajectory_table,
)

agent_steps = load_trajectory_table("out/study_bundle")
export_jupedsim_trajectory(agent_steps, "out/agent_steps_jupedsim.txt")
export_vadere_trajectory(agent_steps, "out/agent_steps_vadere.csv")
```

For PedPy preparation:

```sh
PYTHONPATH=. python3 scripts/analyze_agent_steps_with_pedpy.py out/study_bundle -o out/pedpy_agent_steps.csv
```

The script writes a PedPy-ready point table and reports whether PedPy is
installed locally. PedPy remains optional so the core Chiyoda test suite does
not inherit heavyweight trajectory-analysis dependencies.
