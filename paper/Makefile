# Makefile for the Chiyoda research paper.
#
# Targets:
#   make paper      -- build main.pdf via latexmk.
#   make smoke      -- article-class smoke build.
#   make stats      -- regenerate stats.tex from an exported study bundle.
#   make figures    -- render Mermaid sources in figures/ to PDF.
#   make arxiv      -- produce arxiv.tar.gz for upload.
#   make clean      -- remove build artefacts.

PAPER       ?= main
BUILD_DIR   ?= build
STUDY_DIR   ?= ../out/information_control_50
STATS_TEX    = stats.tex
LLM_TABLES_TEX = llm_tables.tex
LLM_MEDIUM_CSV ?= ../out/llm_medium/tables/llm_policy_comparison.csv
LLM_TARGET_CSV ?= ../out/llm_target_selection_ablation/tables/llm_policy_comparison.csv
LLM_REGIME_CSV ?= ../out/llm_regime_robustness/tables/llm_policy_comparison.csv
DET_REGIME_CSV ?= ../out/regime_robustness_900/tables/regime_summary.csv
LLM_PROMPT_CSV ?= ../out/llm_prompt_objective_ablation/tables/llm_policy_comparison.csv
LLM_BUDGET_CSV ?= ../out/llm_budget_equivalence/tables/llm_policy_comparison.csv
LLM_CLAIM_STATS_CSV ?= ../out/llm_synthesis/llm_claim_statistics.csv
LATEXMK     ?= latexmk
PYTHON      ?= python3
LATEXMK_FLAGS = -lualatex -interaction=nonstopmode -halt-on-error \
                -output-directory=$(BUILD_DIR) -shell-escape

FIGURES_MMD := $(wildcard figures/*.mmd)
FIGURES_PDF := $(FIGURES_MMD:.mmd=.pdf)

.PHONY: paper smoke stats llm-tables figures arxiv clean distclean watch

paper: stats llm-tables $(FIGURES_PDF) | $(BUILD_DIR)
	$(LATEXMK) $(LATEXMK_FLAGS) $(PAPER).tex
	@cp $(BUILD_DIR)/$(PAPER).pdf ./$(PAPER).pdf
	@echo "Built $(PAPER).pdf"

smoke: stats llm-tables $(FIGURES_PDF) | $(BUILD_DIR)
	lualatex -interaction=nonstopmode -output-directory=$(BUILD_DIR) main_smoke.tex
	@cp references.bib $(BUILD_DIR)/
	cd $(BUILD_DIR) && bibtex main_smoke
	lualatex -interaction=nonstopmode -output-directory=$(BUILD_DIR) main_smoke.tex
	lualatex -interaction=nonstopmode -output-directory=$(BUILD_DIR) main_smoke.tex
	@cp $(BUILD_DIR)/main_smoke.pdf ./main_smoke.pdf
	@echo "Built main_smoke.pdf"

stats:
	$(PYTHON) scripts/gen_stats.py $(STUDY_DIR) > $(STATS_TEX)

llm-tables:
	$(PYTHON) scripts/gen_llm_tables.py \
	  --medium $(LLM_MEDIUM_CSV) \
	  --target $(LLM_TARGET_CSV) \
	  --llm-regime $(LLM_REGIME_CSV) \
	  --det-regime $(DET_REGIME_CSV) \
	  --prompt-objective $(LLM_PROMPT_CSV) \
	  --budget-equivalence $(LLM_BUDGET_CSV) \
	  --claim-stats $(LLM_CLAIM_STATS_CSV) \
	  -o $(LLM_TABLES_TEX)

figures: $(FIGURES_PDF)

figures/%.pdf: figures/%.mmd
	mmdc -i $< -o $@ -b transparent || \
	  (echo "warning: mmdc not installed; skipping $<"; touch $@)

arxiv: paper
	@rm -f arxiv.tar.gz
	tar --exclude='$(BUILD_DIR)' --exclude='*.aux' --exclude='*.log' \
	    --exclude='*.fls' --exclude='*.fdb_latexmk' --exclude='arxiv.tar.gz' \
	    -czf arxiv.tar.gz \
	    $(PAPER).tex sections/ figures/ references.bib stats.tex llm_tables.tex \
	    2>/dev/null || true
	@echo "Wrote arxiv.tar.gz"

watch: stats
	$(LATEXMK) $(LATEXMK_FLAGS) -pvc $(PAPER).tex

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

clean:
	rm -rf $(BUILD_DIR)
	rm -f $(PAPER).aux $(PAPER).log $(PAPER).out $(PAPER).bbl $(PAPER).blg \
	      $(PAPER).fls $(PAPER).fdb_latexmk $(PAPER).synctex.gz

distclean: clean
	rm -f $(PAPER).pdf main_smoke.pdf $(STATS_TEX) $(LLM_TABLES_TEX) figures/*.pdf arxiv.tar.gz
