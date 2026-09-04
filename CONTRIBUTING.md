# Contributing

Chiyoda accepts contributions that preserve its three non-negotiable contracts:

1. DSL source must compile deterministically to canonical IR.
2. Runtime behavior must be explicit, hashable, and covered by conformance
   tests.
3. Empirical claims require redistributable evidence and held-out evaluation.

Run `make verify` before opening a pull request. Do not add real-facility
security details, private trajectory data, unlicensed data, or claims of
operational safety. New language constructs require updates to the language
reference, executable semantics, validator, generator constraints, and tests.

