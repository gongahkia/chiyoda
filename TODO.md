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
