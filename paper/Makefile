# Makefile for the Chiyoda research paper.
#
# Targets:
#   make paper      -- build main.pdf via PDFLaTeX/BibTeX.
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
PDLATEX     ?= pdflatex
BIBTEX      ?= bibtex
PYTHON      ?= python3
LATEXMK_FLAGS = -pdf -interaction=nonstopmode -halt-on-error \
                -output-directory=$(BUILD_DIR)
PDFLATEX_FLAGS = -interaction=nonstopmode -halt-on-error \
                 -output-directory=$(BUILD_DIR)

FIGURES_MMD := $(wildcard figures/*.mmd)
FIGURES_PDF := $(FIGURES_MMD:.mmd=.pdf)
STUDY_FIGURES := figures/information-safety-frontier.pdf \
                 figures/intervention-timeline.pdf \
                 figures/regime-robustness-heatmap.pdf
ARXIV_FILES := $(PAPER).tex sections/*.tex references.bib $(STATS_TEX) \
               $(LLM_TABLES_TEX) figures/policy-taxonomy.pdf \
               figures/intervention-loop.pdf $(STUDY_FIGURES)

.PHONY: paper smoke stats llm-tables figures arxiv clean distclean watch

paper: stats llm-tables $(FIGURES_PDF) $(STUDY_FIGURES) | $(BUILD_DIR)
	$(PDLATEX) $(PDFLATEX_FLAGS) $(PAPER).tex
	@cp references.bib $(BUILD_DIR)/
	cd $(BUILD_DIR) && $(BIBTEX) $(PAPER)
	$(PDLATEX) $(PDFLATEX_FLAGS) $(PAPER).tex
	$(PDLATEX) $(PDFLATEX_FLAGS) $(PAPER).tex
	@cp $(BUILD_DIR)/$(PAPER).pdf ./$(PAPER).pdf
	@echo "Built $(PAPER).pdf"

smoke: stats llm-tables $(FIGURES_PDF) $(STUDY_FIGURES) | $(BUILD_DIR)
	$(PDLATEX) $(PDFLATEX_FLAGS) main_smoke.tex
	@cp references.bib $(BUILD_DIR)/
	cd $(BUILD_DIR) && $(BIBTEX) main_smoke
	$(PDLATEX) $(PDFLATEX_FLAGS) main_smoke.tex
	$(PDLATEX) $(PDFLATEX_FLAGS) main_smoke.tex
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

figures: $(FIGURES_PDF) $(STUDY_FIGURES)

figures/%.pdf: figures/%.mmd
	mmdc -i $< -o $@ -b transparent || \
	  (echo "warning: mmdc not installed; skipping $<"; touch $@)

figures/information-safety-frontier.pdf: ../out/information_control_50/figures/13_information_safety_frontier.pdf
	cp $< $@

figures/intervention-timeline.pdf: ../out/information_control_50/figures/12_intervention_timeline.pdf
	cp $< $@

arxiv: stats llm-tables $(FIGURES_PDF) $(STUDY_FIGURES)
	@rm -f arxiv.tar.gz
	tar --exclude='$(BUILD_DIR)' --exclude='*.aux' --exclude='*.log' \
	    --exclude='*.fls' --exclude='*.fdb_latexmk' --exclude='arxiv.tar.gz' \
	    -czf arxiv.tar.gz $(ARXIV_FILES)
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
