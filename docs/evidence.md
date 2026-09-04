# Evidence and claim boundary

Chiyoda is an open research alpha. Its current evidence consists of language
conformance and deterministic-runtime tests only. There are **no calibrated
population profiles, facility models, empirical benchmark scores, or
operational claims** in this repository.

## Requirements for an empirical benchmark round

Every public round must include a machine-readable manifest accepted by:

```console
$ chiyoda benchmark verify benchmarks/rounds/ROUND.json
```

The manifest requires:

- at least one calibration dataset and one held-out dataset;
- an openly redistributable license, stable source URL, SHA-256 digest, and
  documented transformation for every dataset;
- a versioned generator, public fixture seeds, a committed evaluation-seed
  hash, and a commitment to release the seeds after the round; and
- a plain-language statement of the supported population, facility primitives,
  metrics, uncertainty, and exclusions.

The validator deliberately rejects private or non-redistributable data. It
does not assess whether a dataset is scientifically adequate; that requires
peer review and the published calibration protocol.

## Population and accessibility boundary

No population profile is shipped until openly redistributable evidence supports
its parameters and held-out evaluation. This avoids presenting “illustrative”
mobility or disability behavior as empirical fidelity. The DSL has lift,
capacity, body-radius, and body-height primitives, but their presence is not a
claim of accessible-egress validation.

## Prohibited interpretation

Do not use Chiyoda outputs to certify buildings, direct evacuations, set
emergency procedures, assess a real facility’s vulnerability, or claim that a
countermeasure will improve safety. The project may support future research on
such questions only after scenario-specific empirical evidence and appropriate
governance.

