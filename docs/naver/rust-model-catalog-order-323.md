# Model catalog byte ordering (#323)

At source `7c8b2f0f05b1140d6d391275d3643ba3daa769b1`, the
[workspace CI job](https://github.com/bin9208/openpilot-rust/actions/runs/38560531619/job/115840965686)
failed `export::publication_tests::original_index_pointer_bytes_and_immutable_reuse_match`.
The native catalog used insertion-order object keys when the workspace enabled
`serde_json/preserve_order` (Athena requests this feature). The isolated package's
default sorted maps had hidden this dependency on Cargo feature unification.

Both original publishers, `rust/tools/model_export/publish.py` and
`offline_publish.py`, encode the catalog with `sort_keys=True, indent=2` and a
trailing newline. Catalog bytes determine the immutable generation SHA-256 and
`current.json`; equivalent parsed JSON therefore does not satisfy the contract.
The existing original catalog and pointer fixtures remain the byte oracle.

The correction belongs only at catalog publication: recursively sort that JSON
value before the existing Python-compatible pretty formatter. Do not sort
`python_pretty` globally: `model_export/compile.py` emits CPU graph JSON without
`sort_keys`, preserving dataclass field order. Existing CPU v1/v2 graph fixtures
protect that separate contract. Bundle selection array order is also preserved.

## Verification

Local evidence on 2026-10-14, from the source above plus this catalog-only fix:

- RED: `cargo test --manifest-path rust/Cargo.toml --locked -p
  openpilot-model-runtime --lib --features serde_json/preserve_order
  export::publication_tests::original_index_pointer_bytes_and_immutable_reuse_match`
  failed at line638 with insertion-order top-level and nested bundle keys versus
  the unchanged sorted original fixture.
- GREEN: full `cargo test --manifest-path rust/Cargo.toml --locked -p
  openpilot-model-runtime`, both with and without `--features
  serde_json/preserve_order`: 43 tests pass in each mode. This includes catalog
  bytes, generation pointer/digest, immutable reuse, failed publication retention,
  and original CPU v1/v2 graph byte fixtures.
- Strict `cargo clippy` for the same package, `--all-targets -- -D warnings`,
  passes in both feature modes. Rustfmt with repository edition2021 and
  `git diff --check` pass. An initial edition2024 formatting check reported
  unrelated existing formatting differences; rerunning with the actual workspace
  edition2021 passed without rewriting them.
- Every Cargo invocation checked capacity first (over372GiB available versus
  25GiB plus1GiB estimated growth), used the coordinated target cache, two jobs
  and incremental compilation disabled.

The existing regression fixture was sufficient; no fixture or source oracle was
edited. Full workspace CI rerun, rebuilt runtime packaging and device acceptance
remain separate gates owned by the parent task.
