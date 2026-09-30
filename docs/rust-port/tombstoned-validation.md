# Native crash-file daemon and project Sentry policy

Issue [#76](https://github.com/bin9208/openpilot-rust/issues/76), under full runtime
issue [#1](https://github.com/bin9208/openpilot-rust/issues/1). This increment adds
`openpilot-tombstoned` and the reusable `openpilot-crash-reporting` library. It does
not switch manager selection or establish a complete runtime/device candidate.
No device connection, public Sentry event, deployment or CPU measurement is part
of this validation.

## Provenance and contract

The inherited MIT-licensed sources remain unchanged:

| Source | SHA-256 |
| --- | --- |
| `openpilot/system/tombstoned.py` | `a7a5f32535fe7c3a74593b3252366db5ac1c8b43f64837293e1c192f517527a5` |
| `openpilot/system/sentry.py` | `fec900bf600e7a714460d8494472d6cc648759abf36ff72b79158bff61135fd8` |
| `openpilot/system/athena/registration.py` | `461b5976d0e0ad7849c8096e5372dc3514da24fcd7c1924c97027cdcb62a0924` |
| `openpilot/common/params_pyx.pyx` | `a8cc5e5296dd38a012c55b0867c4563f8e320d25960260d897675dd90f0ea544` |

The oracle loads original tombstoned and Sentry functions, the original compiled
Params binding, and registration's unchanged reporting gate. Only fixture paths,
clock/hardware inputs and the external SDK boundary are substituted. The source
main loop runs on a thread with its five-second sleep gated by the test driver.
Native implementation code does not import or execute these project Python modules.

- Initialize reporting before startup cleanup. Preserve comma-origin, registration
  and non-PC gates, the second DongleId read, project DSNs, version, release/master
  environment, tags, user, disabled default integrations and max value length 8192.
- Clear non-hidden startup files; scan the first 1,000 directory entries, following
  stat links, selecting `tombstone*` or exact-mode `0o100640` `.crash` files. Compare
  filename/integer-ctime pairs, remove eligible new files when reporting is disabled,
  catch per-file errors and advance the observed set even after errors.
- Preserve strict size `> 100000000`, UTF-8 text decoding/read-ahead and newline
  behavior, metadata/ProcMaps/CoreDump filtering, signal names, trace-line choice,
  sanitized/truncated destination names, report-before-copy ordering, copy modes,
  existing-directory and same-inode handling, and permission-denied unlink policy.
- Preserve raw Unix filename bytes through filesystem and positional shell calls.
  UTF-8 surrogateescape codepoints survive SDK extras and structured log packets;
  console output uses backslash escaping for lone surrogates. The logging addition
  is a validated `PythonText` value, with existing `String` producers unchanged.
- The real daemon scans at five-second intervals. SIGINT/SIGTERM cancel an active
  retrace within bounded 50 ms polling/read checks, kill its dedicated process group,
  reap the owned child and exit without reporting/copying the interrupted crash.
  Timeout cleanup also terminates that command group; the normal timeout remains
  30 seconds. `--cycles` bounds host fixtures. Explicit base/apport/log paths isolate QA;
  hardware simulation requires a loopback-only DSN and retains reporting gates.

## External boundaries and deliberate difference

The native Sentry SDK is pinned to `sentry` 0.49.3 (`ureq`/`rustls`, default features
off). Project calls use its native client, queue, transport and flush. A narrow raw
Sentry envelope adapter preserves Python JSON tag types and project string extras.
Its string truncation and `_meta` behavior is checked against MIT-licensed
`sentry-python` 2.55.0 `utils.strip_string` and `serializer.Serializer`: UTF-8 byte
limits, incomplete-prefix decoding, three-dot suffix, codepoint fallback for lone
surrogates and nonfinite number representations. This is not an implementation of
all SDK databag, object, cycle, retry or Python thread-hook behavior. SDK/platform
identity and actual Rust error/backtrace information remain native. Explicit
`Reporter::capture_exception` captures real Rust errors; Python
`ThreadingIntegration` is recorded as requested policy but is not claimed to be a
native automatic thread hook.

Bash, cat, echo and the OS `apport-retrace` utility remain external. The latter can
itself be a Python-based OS package; this increment does not port that package.
Git, libzmq and the existing temporary original C++ msgq dependency remain explicit.
The subprocess keeps the source process-substitution pipeline and 30-second timeout,
including failure/timeout text. One deliberate safety fix, tracked in
[#77](https://github.com/bin9208/openpilot-rust/issues/77), passes the filename as a
positional argument instead of interpolating it into shell code. Three harmless
marker fixtures reproduce source command substitution and verify literal native
filename handling; ordinary filenames retain the same subprocess input.

## Reproduction and evidence

Use a built original `params_pyx.cpython-312-x86_64-linux-gnu.so` as `PARAMS_BINDING`
and the original built msgq Python module directory as `MSGQ_PYTHON`. All paths
passed to the daemon by these scripts are temporary fixture directories. Never run
an unconfigured host daemon against its default `/var/crash` as a QA shortcut.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-logging -p openpilot-tombstoned -p openpilot-crash-reporting -p openpilot-runtime-version --all-targets --locked
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-logging -p openpilot-tombstoned -p openpilot-crash-reporting -p openpilot-runtime-version --all-targets --locked -- -D warnings
cargo build --manifest-path rust/Cargo.toml -p openpilot-tombstoned -p openpilot-logging -p openpilot-logmessaged --bins --examples --locked
uv run --no-project --python 3.12 rust/tools/check_tombstoned_reference.py rust/target/debug/examples/tombstone_trace "$PARAMS_BINDING" "$EVIDENCE/reference"
uv run --no-project --python 3.12 rust/tools/check_crash_sdk_transport.py rust/target/debug/examples/tombstone_trace "$PARAMS_BINDING" "$EVIDENCE/sdk"
uv run --no-project --python 3.12 rust/tools/check_tombstoned_safety.py rust/target/debug/examples/tombstone_trace "$PARAMS_BINDING" "$EVIDENCE/safety"
PYTHONPATH="$MSGQ_PYTHON:.:rust/tools" uv run --no-project --python 3.12 rust/tools/check_tombstoned_runtime.py rust/target/debug/openpilot-tombstoned rust/target/debug/openpilot-logmessaged "$PARAMS_BINDING" "$EVIDENCE/runtime"
```

The source comparison captures requests, both replies, native structured packets and
filesystem bytes/modes. It includes real 30-second timeout, continuous stdout,
nonzero/invalid UTF-8 subprocess results, malformed content, size boundaries,
permission/copy failures, raw filenames, reporting gates and SDK failure ordering.
SDK validation captures actual loopback HTTP envelopes and compares fourteen events
with the pinned Python SDK, then verifies a real native I/O exception separately.
Runtime validation uses original cereal/msgq readers with pycapnp **2.1.0**, checks
six logMessage/three errorLogMessage packets and six persisted records, a raw-byte
filename report/copy/removal, no later duplicates, PC-disabled removal and four
startup failures before cleanup. Earlier pycapnp 2.2 runs are historical only.

`check_tombstoned_shutdown.py DAEMON PARAMS_BINDING OUTPUT` runs unchanged source
main and the actual native daemon with a held apport command and descendant. The
frozen pre-fix binary exceeded the manager's five-second stop budget for SIGINT
and SIGTERM; original SIGINT exited promptly but left a held descendant. The native
regression requires exit zero within five seconds, no active owned descendants,
no HTTP event/copy, and retention of the unprocessed crash. It uses the same
`PYTHONPATH`/pinned dependency setup as runtime validation. The before binary and
red/green process snapshots are retained in the evidence ledger. SDK flush was not
reached in the failing scenario; its existing policy is unchanged.

Logging regression uses `check_logging_producer.py` against original and native
collectors; collector regressions use `check_logmessaged_reference.py` and
`check_logmessaged_native.py`. The latter retains its documented deep-object
resource difference. Evidence ledgers record exact invocations, source/binary
hashes, binary pass/fail observations and artifact paths.

The generic `aarch64-unknown-linux-gnu.2.28` build and QEMU oracle/loopback SDK checks
are host emulation evidence. QEMU's missing-interpreter exec behavior returns the
source's nonzero-command result instead of spawn-time `FileNotFoundError`; the one
case is recorded as a failing comparison with `EMULATION_LIMITS`, never counted as
passing. Hardware, target apport availability, manager/startup integration, first
complete runtime comparison and existing upload-path integration remain separate
full-runtime gates. The user performs the first device comparison after the whole
project-owned runtime candidate is ready.
