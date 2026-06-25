PYTHON ?= python3
VENV_PYTHON ?= .venv/bin/python

.PHONY: all config venv test verify lint typecheck scenario-audit viewer-visual-qa doctor precommit profile build dist-check

all:config

config: requirements.txt requirements-dev.txt
	@echo "Installing requirements..."
	@$(PYTHON) -m pip install -r requirements.txt -r requirements-dev.txt

venv: requirements.txt requirements-dev.txt
	@echo "Creating/updating .venv..."
	@python3 -m venv .venv
	@$(VENV_PYTHON) -m ensurepip --upgrade
	@$(VENV_PYTHON) -m pip install -r requirements.txt -r requirements-dev.txt

test:
	@$(PYTHON) -m pytest -m "not slow"

verify: test

lint:
	@$(PYTHON) -m ruff check chiyoda tests scripts
	@$(PYTHON) -m black --check chiyoda tests scripts

typecheck:
	@$(PYTHON) scripts/check_mypy_baseline.py

scenario-audit:
	@$(PYTHON) scripts/audit_scenarios.py

viewer-visual-qa:
	@$(PYTHON) scripts/verify_viewer_visual.py out/viewer --screenshot out/viewer_visual_qa.png

doctor:
	@$(PYTHON) -m pytest --version

precommit:
	@echo "installing precommit hooks..."
	@$(PYTHON) -m pip install -r requirements-dev.txt
	@pre-commit install
	@pre-commit autoupdate
	@pre-commit run --all-files

profile:
	@mkdir -p out
	@$(PYTHON) -m cProfile -o out/profile.prof scripts/profile_large_scenario.py
	@echo "wrote out/profile.prof; inspect with: snakeviz out/profile.prof"

build:
	@$(PYTHON) -m pip install --quiet build
	@$(PYTHON) -m build --sdist --wheel

dist-check: build
	@$(PYTHON) -m pip install --quiet twine
	@$(PYTHON) -m twine check dist/*
