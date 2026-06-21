PYTHON ?= python3
VENV_PYTHON ?= .venv/bin/python

.PHONY: all config venv test verify doctor precommit

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
	@$(PYTHON) -m pytest

verify: test

doctor:
	@$(PYTHON) -m pytest --version

precommit: 
	@echo "installing precommit hooks..."
	@$(PYTHON) -m pip install pre-commit
	@pre-commit install
	@pre-commit autoupdate
	@pre-commit run --all-files
