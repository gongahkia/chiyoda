from __future__ import annotations

import hashlib

from chiyoda.scenarios.patching import (
    apply_exported_patch_file,
    canonical_scenario_bytes,
    exported_scenario_body,
)
from chiyoda.scenarios.validation import validate_scenario_config


def test_exported_patch_reapplies_to_origin_scenario(tmp_path):
    origin = tmp_path / "origin.yaml"
    origin.write_text(
        """
scenario:
  name: original
  description: original source
  layout:
    cell_size: 1.0
    floors:
      - id: "0"
        z: 0.0
        text: |-
          XXX
          X@X
          XEX
  population:
    total: 1
  behavior:
    freeze_probability: 0.0
"""
    )
    exported = tmp_path / "exported.yaml"
    exported.write_text(
        f"""
origin:
  path: "{origin.resolve()}"
  sha256: "{hashlib.sha256(origin.read_bytes()).hexdigest()}"
patch:
  format: "RFC6902"
  ops:
    - op: "remove"
      path: "/behavior"
    - op: "replace"
      path: "/name"
      value: "edited"
    - op: "replace"
      path: "/description"
      value: "Edited from Chiyoda static viewer export."
    - op: "replace"
      path: "/layout"
      value:
        cell_size: 1
        floors:
          - id: "0"
            z: 0
            text: |
              XXX
              X@X
              XEX
    - op: "replace"
      path: "/population"
      value:
        total: 1
        cohorts:
          - name: baseline
            count: 1
            spawn_cells:
              - {{floor: "0", x: 1, y: 1}}
    - op: "add"
      path: "/simulation"
      value:
        max_steps: 20
        dt: 0.1
        random_seed: 42
    - op: "add"
      path: "/information"
      value:
        mode: asymmetric
scenario:
  name: "edited"
  description: "Edited from Chiyoda static viewer export."
  layout:
    cell_size: 1
    floors:
      - id: "0"
        z: 0
        text: |
          XXX
          X@X
          XEX
  population:
    total: 1
    cohorts:
      - name: baseline
        count: 1
        spawn_cells:
          - {{floor: "0", x: 1, y: 1}}
  simulation:
    max_steps: 20
    dt: 0.1
    random_seed: 42
  information:
    mode: asymmetric
"""
    )

    patched = apply_exported_patch_file(exported)
    expected = exported_scenario_body(exported)

    assert canonical_scenario_bytes(patched) == canonical_scenario_bytes(expected)
    assert not validate_scenario_config(patched).has_errors


def test_exported_patch_rejects_origin_hash_mismatch(tmp_path):
    origin = tmp_path / "origin.yaml"
    origin.write_text("scenario:\n  name: original\n")
    exported = tmp_path / "exported.yaml"
    exported.write_text(
        f"""
origin:
  path: "{origin.resolve()}"
  sha256: "{'0' * 64}"
patch:
  ops: []
scenario:
  name: "original"
"""
    )

    try:
        apply_exported_patch_file(exported)
    except ValueError as exc:
        assert "sha256 mismatch" in str(exc)
    else:
        raise AssertionError("expected sha256 mismatch")
