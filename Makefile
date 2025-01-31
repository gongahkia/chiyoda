all:config

config:requirements.txt
	@echo "Installing requirements..."
	@pip install -r requirements.txt

precommit: 
	@echo "installing precommit hooks..."
	@pip install pre-commit
	@pre-commit install
	@pre-commit autoupdate
	@pre-commit run --all-files
