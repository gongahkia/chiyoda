# TODO

This file records unfinished work only. Completed empirical artifacts,
implementation notes, and smoke-build status should live in the repo history,
README, paper text, or `paper/REPRODUCIBILITY.md`.

## Scope Guard

External operational validation remains future work, not a blocker for the
current paper as long as the paper does not purport operational readiness for
real-world deployment and does not claim validated hazard physics or calibrated
behavior before matched hazard, trajectory, drill, incident, or expert-coded
references are integrated.

## Pre-Submission Follow-Up

1. Wuppertal bottleneck calibration sweep.
   - The external Wuppertal trajectory ingestion and flow comparison are now
     implemented, but the current Chiyoda bottleneck proxy underestimates
     observed bottleneck flow.
   - Run a small calibration sweep over bottleneck width, grid geometry,
     base-speed, density slowdown, and social-force parameters.
   - Record the best comparison table and explicitly state whether the result
     is a calibrated bottleneck-flow match or only a diagnostic gap.
   - Do not claim calibrated pedestrian behavior unless the Wuppertal flow and
     time-headway errors are reduced and reported.

2. Final paper build dependency check.
   - Install or vendor the missing TeX dependency `hyperxmp.sty`.
   - Re-run `make doctor PYTHON=.venv/bin/python`, `cd paper && make smoke
     PYTHON=../.venv/bin/python`, and the final `make paper` target before
     submission.

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
