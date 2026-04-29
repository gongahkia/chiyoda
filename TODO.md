# TODO

This file records unfinished work only. Completed empirical artifacts,
implementation notes, and smoke-build status should live in the repo history,
README, paper text, or `paper/REPRODUCIBILITY.md`.

## Next Implementation Work

1. Hazard physics cross-checks.
   - Add an import path for precomputed hazard fields from FDS or published
     gas/smoke examples before making any stronger validated-physics claim.
   - Keep Chiyoda's current hazard model described as stylized unless it is
     cross-checked against such a reference.
   - Add tests that verify imported hazard fields affect exposure, visibility,
     and route penalties consistently.

2. Evacuation drill and incident-data ingestion.
   - Define a schema for drill, VR, incident, or expert-coded event references.
   - Keep ingestion separate from simulation execution so comparisons remain
     auditable and do not become hidden hand-tuning.
   - Add explicit provenance fields for source, license, timestamp, station,
     scenario assumptions, and known missing data.

3. Pedestrian trajectory reference work.
   - Collect one small, license-compatible trajectory reference sample for
     CI-scale regression tests.
   - Keep full public trajectory datasets optional because video-derived
     trajectory corpora can range from tens of megabytes to multiple gigabytes.
   - Add a PedPy analysis notebook or script that consumes Chiyoda `agent_steps`
     exports instead of reimplementing full trajectory science in Chiyoda.
   - Add JuPedSim/Vadere-compatible trajectory export if it helps comparison
     with established pedestrian simulators.

4. Real station geometry fixture.
   - Collect one small, license-compatible OSM/OpenStationMap or GTFS Pathways
     station sample for CI-scale ingestion checks.
   - Record source URL, license, access date, station, level, coordinate
     transform, manual edits, and known missing indoor topology.
   - Keep it separate from paper validation until trajectory, drill, incident,
     or expert-coded references are matched to the same station.

5. Generated calibration cache audit.
   - Extend cache audit or synthesis scripts to summarize generated population
     calibration cache records before any live generated population priors are
     reported as study artifacts.
   - Include provider, model, validation status, rejection reasons, token
     usage when present, applied targets, and skipped overwrite attempts.

6. Developer environment cleanup.
   - Repair or recreate `.venv`; it currently lacks `pip` and `pytest`.
   - Install or document the full TeX dependencies needed for the ACM-style
     `make paper` target; the local TeX install currently has `acmart.cls` but
     is missing `hyperxmp.sty`.
   - Document the expected Python command for verification so `python3 -m
     pytest`, `.venv/bin/python -m pytest`, and paper smoke builds are
     reproducible on a fresh checkout.

## Scope Guard

External validation remains future work, not a blocker for the current paper as
long as the paper does not purport operational readiness for real-world
deployment and does not claim validated hazard physics or calibrated behavior
before the corresponding references are integrated.

## After Paper Acceptance/Final arXiv Release

1. Publish the final paper to arXiv.
   - Build the release package from `paper/` with `make arxiv` after the final
     manuscript checks are complete.
   - After arXiv assigns the paper URL, add the arXiv link to this repo's
     `README.md`.
   - Add the same paper link to the project/site page in
     `gongahkia.github.io`.
   - Use `make paper` for the final local paper build before publishing site
     or README links.

2. Draft a repo-process blog post.
   - Write a Markdown draft in this repo first.
   - Cover the full process of working on Chiyoda: simulator scope, study
     design, paper hardening, arXiv packaging, reproducibility artifacts, and
     lessons learned.
   - Do not publish the blog post until the paper state and arXiv link are
     final.
