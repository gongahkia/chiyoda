# Developer Environment

Use the repository-local virtual environment for reproducible Python checks:

```sh
python3 -m venv .venv
.venv/bin/python -m ensurepip --upgrade
.venv/bin/python -m pip install -r requirements.txt pytest
.venv/bin/python -m pytest
```

Equivalent Make targets:

```sh
make venv
make test PYTHON=.venv/bin/python
make verify PYTHON=.venv/bin/python
```

The local `.venv` is intentionally ignored by git. If it loses `pip` again,
rerun:

```sh
.venv/bin/python -m ensurepip --upgrade
.venv/bin/python -m pip install -r requirements.txt pytest
```

## Paper Build Dependencies

The lightweight article smoke build and the ACM-style paper build require a
TeX installation with:

- `pdflatex`
- `bibtex`
- `acmart.cls`
- `hyperxmp.sty`

On TeX Live/MacTeX, the practical dependency set is the full MacTeX
distribution or BasicTeX plus the relevant publisher and LaTeX-extra packages.
For `tlmgr`-managed installs, check or install:

```sh
tlmgr install acmart hyperxmp latexmk
```

Then verify:

```sh
kpsewhich acmart.cls
kpsewhich hyperxmp.sty
make doctor PYTHON=.venv/bin/python
```

Current local state at the time this note was written: `.venv` was repaired,
`acmart.cls` was present, and `hyperxmp.sty` was still missing from the TeX
tree. Until `hyperxmp.sty` is installed, `make paper` may fail even when
Python tests pass.

## Expected Verification Commands

For code changes:

```sh
.venv/bin/python -m pytest
```

For paper smoke checks:

```sh
cd paper
make smoke PYTHON=../.venv/bin/python
```

For final ACM/arXiv checks after TeX dependencies are installed:

```sh
cd paper
make paper PYTHON=../.venv/bin/python
make arxiv PYTHON=../.venv/bin/python
```
