# Chiyoda Study Workflow

`Chiyoda` now exports a single-run congestion study dashboard instead of a last-frame demo view.

## Quick Start

Create a local virtual environment and install the project requirements:

```bash
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r requirements.txt
```

Run the example scenario and export the study dashboard:

```bash
. .venv/bin/activate
python -m chiyoda.cli run scenarios/example.yaml --headless -o out.html
```

Run the regression suite:

```bash
. .venv/bin/activate
python -m unittest discover -s tests -v
```

## What The Dashboard Shows

- `Replay`: animated crowd movement with short trails, exits, bottleneck markers, and hazard overlays.
- `Occupancy Heatmap`: where agents are physically concentrating at the selected timestep.
- `Speed Heatmap`: where motion is slowing down, which is usually the first signal of queue formation.
- `Bottleneck Queue / Throughput`: how many agents are waiting at each detected choke point and how many are clearing it over time.
- `Exit Usage / Evacuation`: cumulative flow per exit plus total evacuated count.
- `Density / Speed Timeline`: overall crowding and motion trends for the whole run.
- `Cumulative Path Usage`: which corridors and cells are carrying the most traffic over the run.
- `Travel Time Distribution`: how long completed trips took.
- `Bottleneck Dwell Distribution`: how long agents spent traversing detected bottlenecks.

## How To Read It

- Use the replay and occupancy heatmap together. If occupancy spikes but speed stays healthy, the area is busy but not yet jammed.
- Watch for queue growth without matching throughput growth. That is the clearest sign of a bottleneck that is saturating.
- Compare exit curves. A dominant exit usually means the route choice model and geometry are funneling demand unevenly.
- Check the path-usage heatmap after the run. It shows the persistent routing preference, not just a transient frame.
- Use dwell-time and travel-time distributions together. A long travel tail with a long dwell tail usually means localized congestion, not uniform slowdown across the map.

## Implementation Notes

- Runtime telemetry is captured every step and includes occupancy, local density, speed, per-exit cumulative flow, path usage, and bottleneck metrics.
- Animation frames are sampled for long runs so the exported HTML stays practical to open while the underlying time-series charts still use the full run history.
- Scenario loading and simulation bootstrap both reseed Python and NumPy RNGs when `random_seed` is set, so repeated runs stay deterministic.
