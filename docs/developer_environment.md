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

## Expected Verification Commands

```sh
.venv/bin/python -m pytest
make doctor PYTHON=.venv/bin/python
```

## Profiling

To produce a cProfile run on a large scenario:

```sh
make profile PYTHON=.venv/bin/python
```

This writes `out/profile.prof`. Inspect interactively with
[snakeviz](https://jiffyclub.github.io/snakeviz/):

```sh
.venv/bin/python -m pip install snakeviz
.venv/bin/snakeviz out/profile.prof
```
