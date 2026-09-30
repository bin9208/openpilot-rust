# Continuous procLog runtime validation

Issue: [#30](https://github.com/bin9208/openpilot-rust/issues/30), part of
[#1](https://github.com/bin9208/openpilot-rust/issues/1) and
[#5](https://github.com/bin9208/openpilot-rust/issues/5).
Source base: `6ed3b4bc32fb538faf6f00ff386f55c4ee6cbe34`.

## Implemented boundary

`openpilot-proclogd-runtime` reuses the existing Rust `Collector` and
`wire::encode_snapshot`, publishing canonical, valid `Event.procLog` messages
through `Publisher::for_runtime("procLog", 10 * 1024 * 1024)`. It honors the
original `OPENPILOT_PREFIX`, including the original default namespace, without
waiting for subscribers. Normal invocation runs indefinitely. `--frames N`
provides a positive bounded count, and `--proc-root DIR` permits synthetic host
QA. The original bounded `openpilot-proclogd` executable and its namespace
protections are unchanged. Production manager selection is unchanged.

The source loop in `openpilot/system/proclogd.py` timestamps each event before
collection. `Ratekeeper(0.5)` in `openpilot/common/realtime.py` initializes its
first deadline at the first call after publication, then advances by two
seconds on every iteration. Missed deadlines accumulate: the daemon catches up
without resetting the schedule or skipping collections. The Rust implementation
preserves these details, including lag diagnostics. The final bounded QA frame
exits immediately without an unnecessary trailing wait.

SIGINT/SIGTERM set a shutdown flag. Sleep checks the flag at most every 20 ms
without moving the absolute deadline. An in-progress synchronous collection
finishes before shutdown; its result is discarded when shutdown was requested.
This does not interrupt an indefinitely blocked filesystem operation. The host
FIFO scenarios explicitly release their blocked read before requiring exit.
Collector caching, fields and errors remain in the existing implementation:
process races are skipped, CPU/memory failures produce warnings, and failure to
enumerate the proc root terminates with an error.

## Reproducible checks

Run from the repository root. `PYTHONPATH` must include the isolated original
msgq binding directory when running the daemon check.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-proclogd -p openpilot-runtime-core --all-targets --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-proclogd --bins --examples --locked
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-proclogd --all-targets --locked -- -D warnings
cargo fmt --manifest-path rust/Cargo.toml --all --check
python rust/tools/check_proclog_reference.py
python rust/tools/build_msgq_python.py --output /tmp/proclog-msgq-python
PYTHONPATH=/tmp/proclog-msgq-python python rust/tools/check_proclog_daemon.py \
  --binary rust/target/debug/openpilot-proclogd-runtime \
  --cadence-binary rust/target/debug/examples/cadence_trace \
  --output /tmp/proclog-runtime-evidence
```

The Rust CI invokes the native daemon check after building the original Python
binding, and retains its JSON reports, stderr logs and captured canonical
messages. Existing workspace host and generic aarch64 builds include the new
binary. Exact-head cloud CI and independent review remain integration gates;
local checks do not establish those results.

## Local evidence (2026-09-30)

Artifacts are retained under `.omo/evidence/proclog-runtime/` in the issue worktree.
The local evidence index records full invocations and artifact locations.

| Scenario | Binary observable | Artifact |
| --- | --- | --- |
| TDD before implementation | Missing `cadence` import fails compilation | `red.log` |
| Collector, wire, CLI and schedule regressions | 34 tests pass; existing isolated probe guards retained | `tests.log` |
| Source collector equivalence | Every canonical field matches for 22 rollup plus 22 fallback smaps cycles | `oracle.log` |
| Source fake-clock cadence | 105 actual Python Ratekeeper steps match Rust wait durations within 1 ns of float conversion | `final-native/source-cadence.json` |
| Real native IPC with collection delay | Four messages; timestamp precedes delayed collection; first next-start follows publication by two seconds; 4.4 s overrun catches up | `final-native/report.json`, `final-native/blocked-cadence/message-*.capnp` |
| Live `/proc` publication | Valid memory/CPU/process data, producer PID present, consecutive timestamps near two seconds | `final-native/report.json`, `final-native/SIG*-sleeping/*.capnp` |
| Subscriber timeout and bounded exit | No data before publisher, no extra fifth message, exit status 0 | `final-native.log` |
| SIGINT and SIGTERM | Exit status 0 while sleeping and after releasing collection; no post-signal collection publication | `final-native/report.json` |
| CLI and errors | Invalid options fail; single frame completes without subscriber; unavailable proc root fails; missing CPU/memory warn | `final-native/cli.json`, `tests.log` |

The fake-clock comparison proves source schedule arithmetic; the native IPC
scenarios separately exercise real host time. Neither is an AGNOS scheduling or
vehicle measurement. Raw host messages remain local and are not committed.

## Remaining limits

No device was accessed, deployed to, or requested for testing. There is no CPU,
thermal or vehicle-performance claim. The original msgq native transport remains
an explicit external boundary. Whole-runtime startup, logging, existing upload
integration, unported project-owned services and eventual user device comparison
remain open under #1/#5. As required by [design.md](design.md), the first device
handoff waits for the complete project-owned runtime candidate; older M1 probe
handoff wording does not override this gate.

Docs-Not-Needed: internal daemon implementation and host validation only; no
settings or user-facing production behavior changed.
