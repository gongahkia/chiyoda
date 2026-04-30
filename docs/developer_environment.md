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
- `ACM-Reference-Format.bst`
- `fontaxes.sty`
- `binhex.tex`
- `balance.sty`

The repo vendors `paper/hyperxmp.sty` because BasicTeX cannot install the
non-relocatable `hyperxmp` package in user mode. A system-installed
`hyperxmp.sty` is also fine.

On TeX Live/MacTeX, the practical dependency set is the full MacTeX
distribution or BasicTeX plus the relevant publisher and LaTeX-extra packages.
For `tlmgr`-managed installs, check or install:

```sh
tlmgr install acmart fontaxes kastrup preprint latexmk
```

Then verify:

```sh
kpsewhich acmart.cls
kpsewhich -format=bst ACM-Reference-Format.bst
kpsewhich fontaxes.sty
kpsewhich binhex.tex
kpsewhich balance.sty
make doctor PYTHON=.venv/bin/python
```

If the system TeX tree is not writable, initialize user mode and install the
relocatable packages there:

```sh
tlmgr init-usertree
tlmgr --usermode install acmart fontaxes kastrup preprint
```

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

For final paper checks after TeX dependencies are installed:

```sh
cd paper
make paper PYTHON=../.venv/bin/python
```
