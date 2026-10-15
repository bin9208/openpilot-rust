# Pinned Carrot source coverage

`carrot-wip.json` records every commit in the pinned common-ancestor-to-source
range and every net changed path. Merge commit paths are compared to their first
parent; constituent commits remain individually listed. It is a work ledger,
not an executable feature registry or an assertion of runtime parity.

- `pending`: implementation/equivalence review or verification remains.
- `partially_implemented`: the evidence identifies the implemented portion and
  the outstanding portion of the same source commit.
- `implemented_host_validated`: native code and focused host tests exist; full
  differential, cloud, hardware and user acceptance are separate gates.
- `schema_adapted_host_validated`: wire declarations/tests exist, but producers
  and consumers are not thereby implemented. Deliberate ordinal adaptations
  must be recorded explicitly.
- `reference_only`: non-executable engineering history retained at its source
  SHA, without importing source instructions into this independent repository.
- `excluded_source_ci`: keep this repository's native CI/publication policy;
  source-repository workflows are not copied.

An optimization is not redundant just because its replacement is Rust. Mark it
equivalent only with a concrete native implementation and comparison evidence.
Intermediate experiments later reverted do not define the final runtime; their
commits still remain in the coverage ledger.

`proclog-source.json` retains exact licensed original source only as a
hash-verified differential-test fixture. It has no runtime loader or package
role. The native production sampler is under `crates/runtime-core` and
`crates/proclogd`. Existing retained VW/NNFF comparison files are deliberately
mixed reference snapshots with precise applied source hunks documented in
`docs/naver/dev-md-reference-validation.md` and `dev-md-nnff-validation.md`.

The complete port remains issue #338. Never route these changes into `dev`;
`main` promotion requires later user confirmation.
