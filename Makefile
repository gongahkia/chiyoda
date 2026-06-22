PYTHON ?= python3
VENV_PYTHON ?= .venv/bin/python

.PHONY: all config venv test verify doctor precommit profile build dist-check

all:config

config:requirements.txt
	@echo "Installing requirements..."
	@$(PYTHON) -m pip install -r requirements.txt pytest

venv: requirements.txt
	@echo "Creating/updating .venv..."
	@python3 -m venv .venv
	@$(VENV_PYTHON) -m ensurepip --upgrade
	@$(VENV_PYTHON) -m pip install -r requirements.txt pytest

test:
	@$(PYTHON) -m pytest -m "not slow"

verify: test

doctor:
	@$(PYTHON) -m pytest --version

precommit:
	@echo "installing precommit hooks..."
	@$(PYTHON) -m pip install pre-commit
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
