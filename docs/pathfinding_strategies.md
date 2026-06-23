# Pathfinding Strategies

Chiyoda routes agents through a directed, weighted, floor-aware graph. Edge cost
can include connector travel time, density penalty, ground-truth hazard penalty,
or per-agent hazard beliefs.

Configure routing under `simulation.pathfinding_strategy`:

| Strategy | Use |
|:--|:--|
| `auto` | Default. Uses reverse Dijkstra for evacuation routes and heap A* for target pursuit routes. |
| `networkx_astar` | Compatibility path using NetworkX A*. |
| `heap_astar` | Custom heap A* with lower callback overhead for one-off target routes. |
| `reverse_dijkstra` | Multi-source reverse Dijkstra/flow-field cache for many agents sharing exits. |

`auto` is the recommended production setting. It keeps the public navigator call
shape stable while avoiding repeated per-agent A* searches when many agents route
to the same exit set.

PathFinding.js-style options such as BFS, IDA*, best-first search, and Jump Point
Search are useful educational grid algorithms, but they are not default Chiyoda
strategies. Chiyoda routing is weighted, directed, multi-floor, density-aware,
and hazard-belief-aware; unweighted grid assumptions would change model behavior.

Run route-focused tests with:

```sh
.venv/bin/python -m pytest tests/test_pathfinding_strategies.py -q
```

Run a bounded performance check with:

```sh
.venv/bin/python scripts/profile_large_scenario.py scenarios/station_sarin.yaml --max-steps 20 --population-total 250 --top-n 20 -o out/profile_pathfinding.json
```
