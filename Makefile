PYTHON ?= python3
VENV_PYTHON ?= .venv/bin/python

.PHONY: all config venv test verify doctor paper-smoke precommit

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
	@command -v pdflatex >/dev/null || (echo "missing pdflatex"; exit 1)
	@command -v bibtex >/dev/null || (echo "missing bibtex"; exit 1)
	@kpsewhich acmart.cls >/dev/null || (echo "missing acmart.cls"; exit 1)
	@kpsewhich hyperxmp.sty >/dev/null || (echo "missing hyperxmp.sty"; exit 1)

paper-smoke:
	@$(MAKE) -C paper smoke PYTHON=../$(VENV_PYTHON)

precommit: 
	@echo "installing precommit hooks..."
	@$(PYTHON) -m pip install pre-commit
	@pre-commit install
	@pre-commit autoupdate
	@pre-commit run --all-files
