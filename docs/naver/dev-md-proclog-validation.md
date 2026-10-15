# dev-md incremental procLog PSS port — #338

Source target: `e8e152dbcfd9574a1791ed12026d1880e87689b3`; relevant source commit `1cd1b2c6` (Spread PSS sampling and batch shared C3/C4 drawing calls), compared with ancestor `43203371004e035bdb70a00a8dad29a4b657c6c3`. This record covers only procLog sampling/publication. No commit, push or device action.

## Contract being ported

The final source `openpilot/system/proclog_smaps.py` replaces synchronous all-process smaps scans in 2-second publications with incremental sampling in the same thread. Each step performs at most one 32 KiB read and then pauses at least 5 ms or 19 times its thread CPU use (5% duty budget). Refresh is 40 seconds after completion; cache and incomplete scan maximum age are 120 seconds. Cache timestamp denotes scan start, not completion. Rollup fallback is per process and only on missing rollup. Publication never reads smaps and reports zero values/timestamp for absent or stale samples. Identity checks at open/EOF and snapshot eligibility removal protect PID reuse, exit and RSS threshold crossings. A failed rescan does not overwrite a still-fresh prior completed sample.

Production publication and sampler wakeups must share a thread. Publication schedules from the iteration's current time plus 2 seconds; stalls do not replay missed slots. A single kernel read is indivisible, so userspace pacing cannot guarantee a hard read-duration or whole-core CPU limit.

## Validation status

2026-10-15 host validation used the coordinated `/home/fma1/rust/cache/cargo-target`, incremental disabled, two build jobs, dev/test debug=0. Available disk was checked before every build and remained about 366 GiB. No dependency installation was required; the existing SDK and `.venv` were reused.

RED observed before production edits:

- `runtime-core --test procfs publication_never_reads_smaps_or_exposes_unscanned_pss`: expected zero, baseline incorrectly returned 51,200 PSS bytes during collection.
- `proclogd --test cadence`: expected deadlines 4.9 s and 12 s; baseline returned 4.7 s and 5 s because of accumulated catch-up deadlines.

GREEN:

- `cargo test --manifest-path rust/Cargo.toml -p openpilot-runtime-core -p openpilot-proclogd`: all 47 tests and both doc-test suites passed. Eleven new deterministic sampler tests cover partial/EOF visibility, 32 KiB chunk boundaries, final unterminated lines, 45-process staggering and new-process fairness, completion-based refresh, exact expiry boundary, thread CPU duty pause using post-read wall time, incomplete-scan timeout, PID reuse before/during/after scan, exit/small-process eviction, malformed/failed scans retaining only fresh completed cache, missing-file retry and per-process rollup fallback. Existing metadata-read FIFO PID race protection remains passing.
- Native CLI synthetic procfs test verifies first publication has zero PSS/timestamp and completed PSS appears within three 100 ms frames with scan-start timestamp no later than publication.
- `cargo build ... -p openpilot-proclogd --examples --bins`: passed.
- `.venv/bin/python rust/tools/check_proclog_reference.py --binary /home/fma1/rust/cache/cargo-target/debug/examples/reference_trace`: full Event/procLog packet differential passed against exact final source for both rollup and chunked smaps, including incomplete scan, completion, changed content, 40-second refresh and 120-second expiry.
- Native IPC daemon oracle passed with the original msgq binding built in `.analysis/scratch/2026-10-15-dev-md-proclog/msgq-python`. Actual final upstream main produced `[10,12,17,19,21]` in the controlled stall trace. Real daemon collection blocked for 4.4 s skipped missed slots and resumed with 2-second spacing; SIGINT and SIGTERM exited cleanly during both sleeping and blocked collection. Runtime executable SHA256: `be35bf858e4ae1ffcc1ff2ca8581f3de7368b105fd102b7729755eb2ea34137c`. This is native host evidence, not device or performance acceptance.

Implementation uses `Collector.snapshot_with_clock` to evaluate freshness after ordinary process enumeration; deterministic `snapshot_at` is available for tests. The sampler receives an explicit monotonic/thread-CPU clock and owns its open file, bounded pending line and identity-keyed cache. Public wire field `memPssMonoTime @20` comes from the coordinated schema change. All owned Rust files are formatted and `git diff --check` passed. `cargo clippy --manifest-path rust/Cargo.toml -p openpilot-runtime-core -p openpilot-proclogd --all-targets -- -D warnings` passed on the final changes.

## Source oracle migration

`rust/tools/check_proclog_reference.py` now loads `rust/upstream/proclog-source.json`, containing unchanged final source text for `proclogd.py` and `proclog_smaps.py`. The loader verifies source repository/commit, Git blob SHA1 and content SHA256 before executing source AST at test time. Source blob IDs are `33e93ebc09b788fd90dc7e2d432445a89de9006f` and `4033ad42fae96db37f5de998a0ed44f59b47d1dc`; inherited repository licensing/provenance remains retained. This JSON is an explicitly approved original-source test fixture, not a runtime Python implementation.

`check_proclog_daemon.py` executes the final upstream main scheduling body instead of the obsolete Ratekeeper oracle. `check_aarch64.py` now requires initial unavailable PSS/timestamp and eventual completed PSS within its bounded three-frame run; all other gates remain. Aarch64/QEMU and device runs were not performed in this slice. Float-source oracle timestamps are spaced beyond exact binary-float scheduling boundaries; native integer nanosecond cadence boundaries are tested independently.
