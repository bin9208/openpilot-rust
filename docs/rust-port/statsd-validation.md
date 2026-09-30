# Native statistics producer and daemon

Issue [#75](https://github.com/bin9208/openpilot-rust/issues/75), under the approved
[full-runtime design](design.md) and [#1](https://github.com/bin9208/openpilot-rust/issues/1).
Source baseline: `f9d2c471`, including the reviewed runtime-version prerequisite
`97edf436`. `openpilot/system/statsd.py`, its configuration, and production
manager selection are unchanged.

## Ownership and behavior

`openpilot-statsd` provides `producer::StatLog` and the continuous `statsd-rs`
entrypoint. Run the entrypoint from the repository root, matching the runtime's
working-directory contract. It reads real typed `DongleId` Params, obtains build
metadata through `openpilot-runtime-version`, binds `ipc:///tmp/stats`, subscribes
to actual `deviceState` through the existing SubMaster, and publishes statistics
to the source PC/TICI stats root. It adds no production daemon-selection switch.

The producer uses a fresh PUSH context, LINGER=10, nonblocking sends, and drops
only EAGAIN. EINTR retries the same metric packet, matching PyZMQ; other
transport errors still propagate. See [the interrupted-send regression](../naver/rust-interrupted-logging-93.md). After fork it reconnects before using inherited handles; destruction
in the child does not close or terminate the parent's copied libzmq handles.
Float payloads use the established Python-compatible formatter. The type-safe
native gauge/sample API retains f32/f64 versus integer values, including the
actual cereal Int8/UInt16 hardware metrics, without adding a decimal suffix.

The daemon preserves these source contracts:

- Decode UTF-8 before the malformed-metric handler. Invalid UTF-8 is fatal.
  Split only the source-selected fields; extra separators are not newly rejected.
  Bad numeric values produce INFO `malformed metric`; an unknown type is logged
  only after successfully parsing its value. If that unknown-type log fails, the
  same source handler attempts `malformed metric` with the original metric; a
  second logging failure propagates. Unicode 15 decimal digits,
  underscores, signs, special floats, and source whitespace behavior are retained.
- Preserve metric insertion order, replace gauges in place, accumulate samples,
  and emit all gauges before samples. Float sorting retains CPython's comparison
  and merge order, including unordered NaNs and signed zeros. Compensated summation
  follows Python 3.12, and percentile indices use nearest-even rounding.
- Poll deviceState with the source 100 ms update timeout. Flush only for strictly
  greater than 60 seconds or a received started-state change. Tag the flush with
  the new state. All metric lines share one wall-clock timestamp.
- Preserve arbitrary metadata JSON values and their Python str/repr spelling.
  Normalize the origin during initialization before querying hardware; stringify
  tag values only when rendering a metric. The only shared metadata-library API
  addition is `python_str(&JsonValue) -> Result<Vec<u32>, Error>`, exposing the
  existing renderer while retaining surrogate codepoints until file encoding.
- Reproduce timezone-aware datetime conversion: truncate nanoseconds to integer
  microseconds, correctly round total microseconds divided by 1,000,000, multiply
  by 1e9, and truncate to an integer. Reject times outside datetime years 1–9999
  before publication. Monotonic conversion likewise follows
  CPython's total-nanosecond conversion, including the exact-second branch.
- Clear aggregates and update the flush clock before checking the 10,000-entry
  directory cap or publishing. A full directory logs ERROR `stats dir full` even
  for an empty flush, and drops that flush. Count all directory entries.
- Publish through a same-directory 0600 temporary file and rename. Preserve the
  pre-write existing-destination check, replacement of dangling symlinks, index
  advancement only after success, and leftover temporary files on encoding or
  publication errors. Do not add fsync, retries, truncation, sanitization, or a
  shutdown flush that the source does not perform.

SIGINT/SIGTERM request orderly native loop termination and resource destruction.
Direct original Python SIGINT is KeyboardInterrupt, and direct SIGTERM uses the
OS default; native orderly termination exits zero. Stop is checked at metric
boundaries as well as the outer loop so sustained PUSH traffic cannot postpone
shutdown until EAGAIN. No final metrics flush is
introduced. Neither process status nor emulator execution establishes manager
integration or vehicle acceptance.

## Native dependencies and source provenance

The runtime contains no Python interpreter invocation. Existing Rust logging,
Params, metadata, cereal and messaging crates are reused. libzmq and the existing
original C++ msgq transport remain native dependencies; converting that transport
is separately inventoried. Linux clocks, signals, filesystem and the TICI model
file are operating-system interfaces. Git remains the metadata library's external
source-checkout dependency when no build.json exists.

The safe Rust float-specialized powersort/galloping merge and compensated sum are
derived from CPython v3.12.14 `Objects/listobject.c` and `Python/bltinmodule.c`.
Their PSF license and notices are retained in `rust/crates/statsd/CPYTHON-LICENSE`
and `NOTICE`. The original OpenPilot source and licensing remain unchanged.

## Verification and reproducibility

The isolated worktree's `.omo/evidence/statsd/evidence.json` maps every criterion
to exact commands, binary observables, source/executable hashes and retained
artifacts. Failed attempts remain alongside final runs. Tests use synthetic
identifiers and local directories; no C3X, vehicle, NAS or deployment is involved.

The focused checkers are:

| Checker | Actual surface and acceptance |
| --- | --- |
| `check_stats_numbers.py` | 598 cases compile the unchanged source's line formatter and aggregation statements; randomized IEEE doubles, NaNs/infinities, compensated cancellation, signed zero and run lengths through 4096 produce identical file text. |
| `check_stats_clock.py` | 5,048 datetime cases across years 1–9999 and 5,009 calls to CPython's actual `_PyTime_AsSecondsDouble` match the native conversion exactly. |
| `check_stats_daemon.py` | Native PUSH producer, original cereal/msgq deviceState publisher, actual original/native continuous loops, real files and original logmessaged collector. Compare exact file bytes/modes, all Unicode 15 decimal digits, overwritten gauge order, sample output, malformed/unknown log payloads and levels, strict-time/state flushes, file cap with empty/data flushes and subsequent discarded-data behavior. |
| `check_stats_failures.py` | Original/native fatal UTF-8, surrogate write, missing directory, permission denial, existing destination and dangling-symlink publication; physical missing/invalid UTF-8 Params and real warning logs. |
| `check_stats_producer.py` | Original/native fork reconnect and inherited-handle destruction; default HWM accepts 1000 of 20,000 queued gauge sends while the remaining 19,000 are dropped without blocking. Ten captured gauge/sample packets from real cereal Int8/UInt16/Float32 getters and integer/float samples match without coercing the source inputs. |
| `check_stats_log_fault.py` | A one-shot ZMQ EINVAL at the logging boundary in the actual original and native continuous loops produces the same actual collector malformed-metric record. A separate regression verifies that a second log failure propagates. |
| `check_stats_flood.py` | The production daemon must stop on SIGINT/SIGTERM while two native PUSH processes continue flooding metrics. The pre-fix SIGINT run timed out at two seconds; the regression checks termination without a shutdown flush. |
| `check_stats_runtime.py` | Production `statsd-rs` with real clocks and default socket/PC paths; actual deviceState, file publication and bounded SIGTERM exit. |

The original main is compiled unchanged with imports supplied by an adapter.
In normal comparisons, only clock values, temporary paths and the PC hardware
fixture are adapted;
metric parsing, aggregation, SubMaster, real Cython Params, atomic_write and
logging execute their original implementations. The dedicated logging-fault
scenario additionally injects one logging error while retaining the real successful
logging/collector path. The numeric extraction is a
separate focused test, not a replacement for continuous runtime verification.

Run focused Rust checks with:

```sh
cargo fmt --all -- --check
cargo clippy -p openpilot-statsd -p openpilot-runtime-version --all-targets --locked -- -D warnings
cargo test -p openpilot-statsd -p openpilot-runtime-version --all-targets --locked
cargo build -p openpilot-statsd --all-targets --locked
```

Run Python checkers with Python 3.12, the original msgq binding first on
PYTHONPATH, this checkout and rust/tools. The daemon/failure checkers take the
native debug binary directory, original Params binding `.so`, and a new evidence
directory. The number and clock checkers take their matching built examples.
The producer checker takes the examples directory; the runtime checker takes
the binary directory. All commands and exact arguments are in the local ledger.

The final source/runtime oracles also run in an isolated environment matching CI:
Python 3.12, pycapnp 2.1.0, NumPy 2.5.3, pyzmq 27.2.0 and zstandard 0.25.0.
Earlier local pycapnp 2.2.4 runs remain explicitly labeled historical evidence.

The first timestamp oracle exposed a one-nanosecond error for 1 second plus
999,999,999 nanoseconds. It is retained as RED evidence, corrected by integer
microsecond division, and locked by a focused regression test. Earlier harness
failures include the Unix IPC path-length limit, the original collector's expected
SIGINT status, and a cross-transport fixture race: a PUSH send acknowledgement did not prove
metric consumption before a deviceState transition. The checker now waits for
actual collector records from same-socket malformed-metric barriers before
advancing state/time, then bounds the publication wait. These are recorded separately from code
parity failures.

## Validation boundaries

Host checks and a generic aarch64 GNU 2.28 build are distinct. QEMU userspace runs
use the same original x86 peers with the aarch64 binaries and a matching sysroot;
they exercise emulated IPC/filesystem behavior, not AGNOS or a device. Metadata
fixtures use build.json; no successful QEMU Git execve parity is claimed.

Miri stops at the unsupported external `zmq_ctx_new` call; this is an explicit
FFI coverage limitation, not a passing Miri result. The targeted ASAN examples
exercise the actual producer/fork/drop/backpressure scenarios. Existing external
native C/C++ dependencies are not wholly sanitizer-instrumented by that run.
Production library code forbids unsafe; the only added unsafe Rust is the small
fork/waitpid/_exit verification example.

CI wiring, exact-SHA Actions results, integration and production manager selection
remain the integrating task's responsibility. This increment does not complete
whole-runtime issue #1, prove CPU savings, or authorize a first device test.
The first device comparison remains gated on the complete project-owned runtime,
normal startup and the existing log upload path.
