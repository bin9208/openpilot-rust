# Issue #46: bound torqued DEBUG publication

Scope: [#46](https://github.com/bin9208/openpilot-rust/issues/46), assigned under
[#497](https://github.com/bin9208/openpilot-rust/issues/497). Worktree:
`/home/fma1/.codex/worktrees/backlog-fixes-497/openpilot-rust`, branch
`codex/fix-497-stale-runtime-defects`, stable source
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.

## Mechanism and change

The source-valid 4,080-point cache serializes to approximately 98 KiB. The
256,000-byte service queue requires three framed/aligned messages to fit.
Original Python aborts; Rust's publisher rejects the payload and main propagates
the error, exiting the daemon. Ordinary DEBUG=0 packets and Params writes are
unaffected by this limit.

Only the IPC encoder now checks the actual serialized length against the existing
msgq publisher limit (`service.queue_size / 3 - 16`). If it exceeds that limit,
it re-encodes the same packet without its diagnostic points list and returns
omission metadata without logging. Daemon-local state reports the omitted count,
original size and limit only when entering an oversized diagnostic interval.
Any subsequent fitting/no-points publication clears that state, permitting one
new warning if publication becomes oversized again. No process-global state is
used. Sparse DEBUG packets remain complete. Estimator state, fit calls, RNG progression, filters, publication
cadence/validity, queue sizes and full Params serialization remain unchanged.

This is an intentional source compatibility exception only for the `points` field
of an oversized DEBUG IPC packet. Original Python remains the oracle, including
its isolated SIGABRT reproduction. No numeric comparison tolerance changes.

## Regression and hosted commands

`check_torque_debug_capacity.py` was changed before implementation. It executes
the real daemon with eight buckets containing 1, 440, 448, 510 or 1,500 points each,
checks six consecutive publications, every other field against the unchanged
source oracle, bounded byte lengths, exactly one omission warning for six oversized
publications (zero for fitting packets), and full persisted points/parameters. The sparse and boundary cases ensure points are retained when
admissible. The original abort child uses its own disposable IPC namespace.

Boyle's P2 review identified per-publication warning spam in the first patch.
The follow-up adds `src/publication_tests.rs` to the actual daemon binary's unit
tests. Using the production encoder, daemon-local omission state and real estimator,
it checks one warning over six oversized publications, a NaN fit resetting the
actual bucket population and warning state, re-entry after refill, and a no-points
publication rearming the next DEBUG interval. Full Params encoding still contains
all 4,080 points. The existing all-targets hosted command below includes this test;
no local test execution was performed.

Run only in GitHub-hosted Actions, using the existing Rust CI dependency setup,
from the repository root (fresh output directories):

```sh
export PYTHONPATH="$RUNNER_TEMP/rust-msgq-python:$PWD:$PWD/rust/tools"
export OPENBLAS_NUM_THREADS=1
cargo test --manifest-path rust/Cargo.toml -p openpilot-torqued --all-targets --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-torqued --bins --examples --locked
python rust/tools/build_msgq_python.py --output "$RUNNER_TEMP/rust-msgq-python"
python rust/tools/check_torque_reference.py --binary rust/target/debug/examples/torque_trace --numerics "$RUNNER_TEMP/torque-numerics" --output "$RUNNER_TEMP/issue-46-estimator"
python rust/tools/check_torque_loop.py --binary rust/target/debug/examples/torque_loop --numerics "$RUNNER_TEMP/torque-numerics" --output "$RUNNER_TEMP/issue-46-loop"
python rust/tools/check_torque_daemon.py --binary rust/target/debug/openpilot-torqued --numerics "$RUNNER_TEMP/torque-numerics" --output "$RUNNER_TEMP/issue-46-daemon"
python rust/tools/check_torque_debug_capacity.py --binary rust/target/debug/openpilot-torqued --numerics "$RUNNER_TEMP/torque-numerics" --output "$RUNNER_TEMP/issue-46-debug"
```

For actual RED, in a disposable hosted candidate checkout retain the changed
checker but restore **only** these production files from the stable SHA before
building the daemon:

```sh
git show b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:rust/crates/torqued/src/main.rs > rust/crates/torqued/src/main.rs
git show b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:rust/crates/torqued/src/wire.rs > rust/crates/torqued/src/wire.rs
```

The new checker must fail when it reaches the first oversized cache because the
original runtime exits nonzero. Rebuild the unchanged candidate in the separate
GREEN job; do not modify another worker's checkout or run this restoration locally.

## Evidence status

Local checks are limited to Rust formatting, Python AST syntax and scoped diff
review. No local build, test, fixture, performance run, device operation, commit,
push or GitHub mutation was performed. Parent owns exact-SHA hosted RED/GREEN,
aggregate checks and issue acceptance. Hosted run URLs and device acceptance are
not yet available; this record does not claim them.

Docs-Not-Needed: internal daemon debug failure handling; no settings or user guide
change was assigned.
