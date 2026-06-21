from __future__ import annotations

from chiyoda.environment.layout import Connector
from chiyoda.navigation.connectors import ConnectorQueue


def _connector(**overrides) -> Connector:
    data = {
        "id": "stairs",
        "type": "stairs",
        "from_cell": ("0", 1, 1),
        "to_cell": ("1", 1, 1),
        "capacity": 1,
        "flow_rate": 1.0,
        "queue_mode": "fifo",
        "panic_jam_density": 2.0,
        "jam_flow_multiplier": 0.25,
    }
    data.update(overrides)
    return Connector(**data)


def test_connector_queue_capacity_saturates_and_drains_fifo():
    queue = ConnectorQueue.from_connector(_connector(travel_s=1.0))
    queue.enqueue(1, ("0", 1, 1), ("1", 1, 1))
    queue.enqueue(2, ("0", 1, 1), ("1", 1, 1))

    events = queue.step(time_s=0.0, dt=0.0, density=0.0)

    assert [event.phase for event in events] == ["start"]
    assert events[0].agent_id == 1
    assert queue.active_count() == 1
    assert queue.waiting_count() == 1

    assert queue.finish_ready(time_s=1.0)[0].agent_id == 1
    events = queue.step(time_s=1.0, dt=1.0, density=0.0)

    assert events[0].agent_id == 2
    assert queue.active_count() == 1
    assert queue.waiting_count() == 0


def test_connector_queue_panic_jam_collapses_effective_flow():
    queue = ConnectorQueue.from_connector(_connector())

    assert queue.effective_flow_rate(1.0) == 1.0
    assert queue.effective_flow_rate(3.0) == 0.25
