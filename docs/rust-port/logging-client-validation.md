# Structured logging producer and runtime diagnostics

Issue [#45](https://github.com/bin9208/openpilot-rust/issues/45), under the full
runtime port [#1](https://github.com/bin9208/openpilot-rust/issues/1).
Provenance: unchanged `openpilot/common/swaglog.py`, `openpilot/common/logging_extra.py`
and `openpilot/common/runtime_diagnostics.py`; original licenses remain.

`openpilot-logging` supplies ordered Python-compatible ASCII JSON records and
native ZeroMQ PUSH delivery to `ipc:///tmp/logmessage${OPENPILOT_PREFIX}`.
The packet retains its one-byte severity prefix. Structured event severity
depends on the presence of `error`, then `debug`, including false/null values.
Field order, duplicate-key last values, nonfinite floats, integer maxima,
exception text and the source's seven tested LOGPRINT configurations are kept.
The shared Python float formatter is moved unchanged out of the collector into
runtime-core; original collector formatting/rotation is tested again.

A clonable factory holds process-wide context. Each logger owns its socket and
local context and cannot cross threads. Scoped guards restore context during
normal return and unwinding; global keys override local keys as in the source.
Metadata records actual Rust file/module/function and line, OS PID/TID/thread
name, hostname and realtime. Default context identifies Rust, actual build Git
SHA and clean/dirty/unknown source state; it never fabricates Python filenames
or treats a host build as a device deployment.

Transport connects lazily, preserves the 10 ms linger and default queue limit,
and uses nonblocking sends. Queue-full drops remain distinct from other typed
transport errors. The source console handler runs before IPC. A console I/O
failure is suppressed and still allows an IPC attempt, matching StreamHandler;
the real `/dev/full` source/native regression verifies both delivery and return.
PID changes after fork abandon only copied
inherited native handles, matching pyzmq's PID guard, and reconnect in the child;
the parent retains its own connection. Production Rust uses no unsafe blocks;
the isolated fork test uses a documented libc boundary.

RuntimeDiagnostics keeps one aggregate per named metric per interval, filters
nonfinite inputs, preserves first-seen metric order and source rounding, and
resets the interval before calling the sink. Sink errors are suppressed to keep
inference/planning running. The live adapter reads the creating thread's
`/proc/self/task/TID/schedstat`, the kernel schedstats switch, monotonic time and
process ID. Missing or malformed scheduler samples omit the delta, as in the
source. Communication snapshots use the receiver's actual moving-average
filters, receive time, validity/aliveness/frequency results and ignore flags;
unknown services are skipped. No validity or frequency threshold is changed.

## Evidence and reproduction

Host observations on 2026-09-30:

- 2,472 formatter packets and 20 event packets match the actual source byte for
  byte, with four matching duplicate-event argument errors.
- Native producer tests cover thread/global/scoped context, OS identity,
  backpressure drops, close and default signals, fork reconnect/inherited drop,
  and exact stderr bytes for seven LOGPRINT values.
- Both actual original logmessaged and Rust logmessaged receive four logMessage
  and two errorLogMessage publications and persist three disk records from the
  producer fixture. Each collector's shutdown outcome is checked independently.
- 18,314 diagnostic steps match types, field order and floating-point bits,
  including 12,011 rounding/nonfinite inputs and integer/overflow boundaries.
- 11,205 original SubMaster updates produce exact communication snapshots,
  including ignored checks, stale/invalid inputs, clock offsets, missing and
  repeated service names and the source zero-interval error state.
- The moved float formatter still passes the existing 6,020-record/nine-rotation
  original collector comparison. Live schedstat sampling and sink failure
  recovery have separate Rust tests.

From the repository root with the existing oracle Python environment:

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-logging --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-logging --examples --locked
python rust/tools/check_runtime_diagnostics.py --binary rust/target/debug/examples/diagnostics_trace --output /tmp/runtime-diagnostics
PYTHONPATH=.:rust/tools python rust/tools/check_communication_snapshot.py --binary rust/target/debug/examples/communication_trace --output /tmp/communication-snapshot
cargo build --manifest-path rust/Cargo.toml -p openpilot-logmessaged --bins --locked
python rust/tools/build_msgq_python.py --output /tmp/logging-msgq
PYTHONPATH=/tmp/logging-msgq:.:rust/tools python rust/tools/check_logging_producer.py \
  --binary rust/target/debug/examples/logging_probe --fork-binary rust/target/debug/examples/logging_fork \
  --collector rust/target/debug/openpilot-logmessaged --output /tmp/logging-producer
```

The required Rust workflow repeats these source/native checks and includes the
crate in its workspace ARM build. Exact-head CI and parent integration evidence
remain distinct from the host observations above. Native dependencies remain
the locked zmq/libzmq stack described in the collector's third-party record;
the runtime invokes no Python formatter or helper process.

The library is not yet wired into every daemon. Original callsite content,
frequency and failure boundaries must be ported with each loop, then checked
through route logging and upload. No production selector, device scheduling or
vehicle behavior changes; no CPU/thermal improvement or complete runtime is
claimed. Existing Python traceback text is accepted as fixture input only;
Rust failures must use real Rust error context.

## Native C++ logging prerequisite for loggerd (#41)

`logging::native::Logger` separately implements the `cloudlog_e` producer from
unchanged `openpilot/common/swaglog.cc`. Its process-wide runtime instance uses
one mutex-protected PUSH socket, the original 100 ms linger, nonblocking sends,
and the normal prefixed logmessage IPC endpoint. Clones share socket/context;
explicit `new` instances allow isolated embedding and validation. The first
nonempty emission captures native environment context. Empty text does not
initialize the socket. C-string message truncation, stdout routing, severity
prefix, sorted json11 key order, spacing, Unicode and control escaping match
the source. Native LOGPRINT defaults to warning even for unrecognized values.

Records retain the source `ctx`, `levelnum`, `filename`, `lineno`, `funcname`,
`created` and `msg` fields. Filenames, functions and lines are real Rust
callsites; timestamps use realtime. Three additional context fields identify
Rust and the build's actual source commit/tree status. Device/version are
provided by the embedding runtime. UTF-8 context is required; invalid native
environment encoding returns a typed error instead of fabricating text.
Serialization does not change the process-wide numeric locale.

The C++ producer ignores console and send errors. Rust suppresses console
errors and reports queue drops separately from other typed transport failures;
source-equivalent runtime callsites must handle emission errors best effort,
without interrupting daemon work. The owner must call `close()` after its final
diagnostics during orderly shutdown to apply the 100 ms drain. Rust statics do
not have the original C++ exit destructor. Closing an initialized logger affects
all clones; closing before first use is a no-op. Arbitrary fork after native
initialization is unsupported, as in C++; Python-producer PID guards are separate.

One `rate::RateLimit` belongs to each original rate-limited callsite. The default
preserves `cloudlog_rl(2, 100, ...)`: CLOCK_BOOTTIME nanoseconds, strict expiry,
the following call's window restart, and the suppression-warning count emitted
before the admitted message at the same callsite. The counter returns a typed
overflow error beyond the source's defined signed-integer range. Callers own
synchronization if a rate-limited callsite is shared across threads.

The native oracle compiles the unchanged producer and source rate macro using
the exact `uv.lock` json11 wheel and hash. Only hardware/version/IPC adapters and
the rate-test boot clock are supplied by the fixture; realtime remains original.
Link wrappers observe actual libzmq calls, flags, linger and send results. The
comparison normalizes only real callsite/time metadata and the three additional
Rust context fields. Captured packet bytes otherwise match. Native dependencies
remain libzmq; json11 and C++ are oracle-only and are not linked into Rust runtime.

```sh
cargo build --manifest-path rust/Cargo.toml -p openpilot-logging --example native_logging_probe --locked
python rust/tools/check_native_logging.py \
  --binary rust/target/debug/examples/native_logging_probe --output /tmp/native-logging
```

Host evidence covers seven LOGPRINT/context configurations, all supported
levels, Unicode/control/NUL text, empty initialization, 516 rate-boundary inputs,
64 concurrent emitters, real backpressure (1,000 accepted/4,000 dropped by each
producer), failed stdout with successful IPC, explicit close, prefixed runtime
IPC and default SIGINT/SIGTERM termination. This remains an isolated prerequisite;
loggerd callsite integration, full-runtime startup/upload and device comparison
are separate delivery gates. No vehicle has been contacted.

Docs-Not-Needed: isolated Rust logging library and validation tools; no user
setting or production selection behavior changes.
