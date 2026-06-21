from __future__ import annotations

import pandas as pd

from chiyoda.studies.models import StudyBundle


def test_empty_optional_tables_export_readable_csv_headers(tmp_path):
    bundle = StudyBundle(
        metadata={},
        summary=pd.DataFrame([{"record_type": "run"}]),
        steps=pd.DataFrame(),
        cells=pd.DataFrame(),
        agent_steps=pd.DataFrame(),
        agents=pd.DataFrame(),
        bottlenecks=pd.DataFrame(),
        dwell_samples=pd.DataFrame(),
        exits=pd.DataFrame(),
        hazards=pd.DataFrame(),
        measurements=pd.DataFrame(),
        gossip=pd.DataFrame(),
        interventions=pd.DataFrame(),
        llm_decisions=pd.DataFrame(),
    )

    bundle.export(tmp_path, table_formats=("csv",))

    for table_name in ("dwell_samples", "measurements", "gossip", "interventions", "llm_decisions"):
        frame = pd.read_csv(tmp_path / "tables" / f"{table_name}.csv")
        assert frame.empty
        assert len(frame.columns) > 0
