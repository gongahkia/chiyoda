# Static 3D Viewer

Chiyoda exports a lightweight Three.js viewer beside study bundles. It is for
research inspection: scenario geometry, trajectory playback, bottlenecks,
hazards, interventions, and LLM decision events.

Run any scenario or study:

```sh
.venv/bin/python -m chiyoda.cli run scenarios/example.yaml -o out/example
```

Open:

```sh
open out/example/viewer/index.html
```

For an existing bundle:

```sh
.venv/bin/python -m chiyoda.cli export-viewer out/example
```

The viewer reads `viewer/viewer_data.json`, generated from:

- `metadata.json`
- `tables/agent_steps.*`
- `tables/hazards.*`
- `tables/bottlenecks.*`
- `tables/interventions.*`
- `tables/llm_decisions.*`

The current viewer is static and local. It does not run simulations, edit source
scenario files, or replace trajectory-analysis tools.
