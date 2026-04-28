# TODO

This file records unfinished work only. Completed empirical artifacts,
implementation notes, and smoke-build status should live in the repo history,
README, paper text, or `paper/REPRODUCIBILITY.md`.

## Paper Hardening

1. Full narrative read from abstract to conclusion.
   - Keep the thesis continuous: emergency communication is safety control, not
     just information spread.
   - Ensure the abstract, introduction, claims, evaluation, limitations, and
     conclusion make the same bounded claim.
   - Make LLM guidance a bounded extension result, not the central paper claim.

2. Claim-evidence alignment pass.
   - Check every major claim in the abstract and introduction against the
     deterministic and LLM artifacts.
   - Weaken or remove any claim that is not directly supported.
   - Preserve the limitation that Chiyoda is a stylized simulator, not an
     operational evacuation planner.

3. Methods and metric clarity.
   - Confirm the agent belief state, route-choice logic, hazard exposure model,
     intervention policy mechanics, and telemetry pipeline are precise enough
     for a reviewer to reproduce.
   - Define ISE and HCI clearly as paper-level constructs.
   - Explain why coupled belief/safety metrics answer the research question
     better than entropy, reach, or evacuation count alone.
   - Explain the newly exposed calibration knobs without implying that current
     paper results are empirically calibrated.

4. Statistical interpretation.
   - Confirm the paper explains seed-level aggregates, Mann-Whitney tests,
     descriptive effect sizes, and nonparametric interpretation.
   - Avoid claiming operational superiority from stylized simulation.
   - Emphasize tradeoff patterns and bounded evidence.

5. Related work final pass.
   - Keep the new JuPedSim, Vadere, SUMO, PedPy, FDS, OpenStationMap, and GTFS
     Pathways positioning focused on interoperability, not replacement.
   - Re-check citation quality and exact bibliographic metadata before release.
   - Make clear why the paper is about communication as control, not only about
     adding messages to an evacuation simulator.

6. Figure, table, and PDF polish.
   - Visually inspect the final PDF for table placement, overfull boxes, figure
     order, and caption clarity.
   - Keep the central result figures and compact policy tables prominent.
   - Avoid overwhelming the reader with raw metrics that do not support the
     thesis.

7. Final release checks.
   - Run the Python test suite.
   - Run the paper smoke build.
   - Run the full ACM-style `make paper` build if the local environment has
     `acmart.cls` and `latexmk`.
   - Confirm reproduction commands and artifact paths in
     `paper/REPRODUCIBILITY.md`.

## Next Implementation Work

1. Heuristic population calibration.
   - Add example scenario variants that use the exposed `behavior` and cohort
     calibration knobs.
   - Keep these as calibration examples until matched against a real trajectory,
     drill, VR, or incident reference.
   - Document parameter provenance for speed, rationality, familiarity,
     credibility, gossip radius, and vision radius.

2. Generated population calibration.
   - Define what external API input would be allowed to influence: cohort mix,
     parameter priors, or scenario metadata.
   - Keep live API use cacheable and replayable, following the existing LLM
     cache/replay pattern.
   - Prevent generated calibration from silently overwriting measured or
     hand-audited scenario parameters.

3. Hazard physics cross-checks.
   - Add an import path for precomputed hazard fields from FDS or published
     gas/smoke examples before making any stronger validated-physics claim.
   - Keep Chiyoda's current hazard model described as stylized unless it is
     cross-checked against such a reference.
   - Add tests that verify imported hazard fields affect exposure, visibility,
     and route penalties consistently.

4. Evacuation drill and incident-data ingestion.
   - Define a schema for drill, VR, incident, or expert-coded event references.
   - Keep ingestion separate from simulation execution so comparisons remain
     auditable and do not become hidden hand-tuning.
   - Add explicit provenance fields for source, license, timestamp, station,
     scenario assumptions, and known missing data.

5. Pedestrian trajectory reference work.
   - Collect one small, license-compatible trajectory reference sample for
     CI-scale regression tests.
   - Keep full public trajectory datasets optional because video-derived
     trajectory corpora can range from tens of megabytes to multiple gigabytes.
   - Add a PedPy analysis notebook or script that consumes Chiyoda `agent_steps`
     exports instead of reimplementing full trajectory science in Chiyoda.
   - Add JuPedSim/Vadere-compatible trajectory export if it helps comparison
     with established pedestrian simulators.

6. Real station geometry fixture.
   - Collect one small, license-compatible OSM/OpenStationMap or GTFS Pathways
     station sample for CI-scale ingestion checks.
   - Record source URL, license, access date, station, level, coordinate
     transform, manual edits, and known missing indoor topology.
   - Keep it separate from paper validation until trajectory, drill, incident,
     or expert-coded references are matched to the same station.

7. Developer environment cleanup.
   - Repair or recreate `.venv`; it currently lacks `pip` and `pytest`.
   - Document the expected Python command for verification so `python3 -m
     pytest` and paper smoke builds are reproducible on a fresh checkout.

## Scope Guard

External validation remains future work, not a blocker for the current paper as
long as the paper does not purport operational readiness for real-world
deployment and does not claim validated hazard physics or calibrated behavior
before the corresponding references are integrated.
