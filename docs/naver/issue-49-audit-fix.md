# Issue #49: unsupported Jetlink raw diagnostic must not stop publication

2026-10-23; parent task [#497](https://github.com/bin9208/openpilot-rust/issues/497),
defect [#49](https://github.com/bin9208/openpilot-rust/issues/49).
Base: `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`; patch remains uncommitted.
Hosted regression evidence, build results and device acceptance are pending.

## Owned changed paths

- `rust/crates/drivingd/src/daemon.rs`
- `rust/crates/drivingd/src/jetlink.rs`
- `rust/crates/drivingd/src/lib.rs`
- `rust/crates/drivingd/tests/jetlink.rs`
- `rust/crates/drivingd/tests/issue_17_49.rs` (shared with the #17 fix)
- `rust/tools/check_jetlink_raw.py`
- `rust/tools/model_logging_reference.py` (exact driving-model warning expectation only)
- `docs/naver/issue-49-audit-fix.md`

## Failure and explicit diagnostic contract

The original Jetlink adapter returns parsed fields without `raw_pred`.
`fill_model_msg()` unconditionally reads it with `SEND_RAW_PRED`, raising
`KeyError` before otherwise valid publication. The Rust parity port intentionally
preserved that failure as `JetlinkRawPredictionsUnavailable`, propagated by `?`
in the driving daemon.

For an external selected frame, requested raw predictions are now explicitly
unsupported: publication contains empty `rawPredictions`, never bytes from the
native model that happened to run alongside it. Native-selected frames retain
their exact raw buffer; turning diagnostics off still publishes no raw data.
When the environment request is enabled, the daemon emits one WARNING at startup
explaining both behaviors, outside the per-frame budget. Failure to emit that
diagnostic cannot abort inference. There is no retry, per-frame warning, invented
external tensor, source switch, or external raw protocol extension.

Frame selection, source/status identity, publication freshness, recurrent state,
USB/eGPU selection and the complete 50 ms Jetlink deadline remain unchanged.
The existing `valid_at_publish()` and all-three-topic invalidation still run
after message construction. The warning does not authorize a stale frame.

## Tests and exact source-comparison scope

Tests were written before implementation. The standalone
`rust/crates/drivingd/tests/issue_17_49.rs` uses the old public API so the parent
can apply it to original production source. It exercises actual modelV2,
drivingModelData and cameraOdometry publication across native/external/repeated
external/native source choices, requests on/off, and stale invalidation. It checks
frame/capture identity and exact native raw bytes. An additional test compares
all three complete serialized external packets with diagnostics on/off.

The existing Jetlink raw unit test now expects a successful unsupported result.
`rust/tools/check_jetlink_raw.py` still requires the actual original
adapter/parser/publication `KeyError('raw_pred')` and normal non-raw success; only
the deliberately corrected Rust raw-request outcome changes to success with no
raw bytes. Its original hardware-only seam and original-error artifact remain.

`rust/tools/model_logging_reference.py` adds exactly one expected level-30 warning
for the existing raw-enabled dual-camera scenario. Full Counter equality still
requires every other original log record and rejects duplicate/extra warnings.
No output-equivalence tolerance, validation threshold or deadline is relaxed.

## Hosted commands and remaining acceptance

Use the dedicated baseline production-file restore described in
[the #17 record](issue-17-audit-fix.md), then run the two original-API #49 tests
on GitHub-hosted Actions; require assertion/panic RED from the typed raw error,
not a compilation/setup failure:

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-driving-modeld \
  --test issue_17_49 issue_49_
```

For a fresh candidate checkout, run the package/all-targets tests, ARM build and
existing model-daemon comparison listed in the #17 record. Retain all existing
Jetlink deadline, reset, source freshness and RPC tests. Build and run these
existing exclusive probes in the repository's original-Python environment:

```sh
cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-driving-modeld \
  --example jetlink_raw_probe
cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-jetlink --examples
PYTHONPATH=.:tinygrad_repo:rust/tools python rust/tools/check_jetlink_raw.py \
  rust/target/debug/examples/jetlink_raw_probe "$RUNNER_TEMP/jetlink-raw"
PYTHONPATH=.:tinygrad_repo:rust/tools python rust/tools/check_jetlink_reference.py \
  rust/target/debug/examples/jetlink_probe "$RUNNER_TEMP/jetlink-reference"
PYTHONPATH=.:tinygrad_repo:rust/tools python rust/tools/check_jetlink_rpc.py \
  rust/target/debug/examples/rpc_probe "$RUNNER_TEMP/jetlink-rpc"
```

No functional tests/builds/fixtures were run locally; rustfmt, Python AST syntax
and scoped diff checks/review are the local evidence only. Parent must record
actual hosted RED/GREEN, unchanged model/logging comparisons, build/review results
and exact Actions URLs before considering software acceptance. Device testing
remains user-owned. No GitHub mutation, commit, push, CI/manifest/lockfile or USB
policy changes were made by this worker.

User guides are outside this worker's ownership. Parent must resolve paired-guide
requirements for the diagnostic behavior change before integration.
