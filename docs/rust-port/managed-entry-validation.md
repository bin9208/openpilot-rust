# Native managed child entry boundary

Issue [#86](https://github.com/bin9208/openpilot-rust/issues/86), alongside manager
supervision [#84](https://github.com/bin9208/openpilot-rust/issues/84), under the
full runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).

`openpilot-managed-entry` is an **in-process** API for translated daemon entrypoints.
An exec-based supervisor cannot catch an exception raised inside another process.
This increment supplies the reusable boundary and a real child validation example;
existing daemon mains, direct executable behavior, manager initialization and
production process selection are unchanged. Entrypoint adoption remains integration
work. This is not a complete runtime or a device-test candidate.

## Source contract and native API

The unchanged MIT-licensed `openpilot/system/manager/process.py:launcher` is the
source policy. Its file SHA-256 is
`c824eded96e88533f4d150a1b2ea0262cfd076cffea0692fad52ef4ba3bf5c8e`.
The oracle executes that original function definition with controlled import/main
bodies, original logging classes, the original compiled Params binding, original
`messaging.reset_context`, and unchanged `openpilot/system/sentry.py`. The Sentry
source SHA-256 is
`fec900bf600e7a714460d8494472d6cc648759abf36ff72b79158bff61135fd8`.
Other source/binding hashes are included in each test manifest.

`launch(reporter, process, daemon, prepare, reset_context, body)` preserves this
ordering inside one diagnostic boundary:

1. Prepare the statically linked entrypoint (the native counterpart of import).
2. Set the calling main thread's Linux process name.
3. Invoke the caller's context-reset callback. The fresh native owner it returns
   is consumed by the body. Native msgq owns queues directly and has no inherited
   Python global Context; the API does not manufacture a no-op global reset.
4. Bind the daemon to local log context, preserving inherited local/global merge
   precedence, and set the Sentry daemon tag.
5. Run the body and return `Outcome::Returned` on success.

`StepError::Interrupted` at preparation, context reset or body produces the source
`child <process> got SIGINT` warning and `Outcome::Interrupted`. `Sigint::install`
provides a cooperative flag that actual child code can observe; it does not unwind
arbitrary Rust code or promise cancellation of blocking operations. Its handle
unregisters only its own signal registration when dropped.

A returned concrete Rust error becomes `EntryError::Raised` with its stage and
original typed cause. `NativeError` captures the real Rust type, message, error
chain and Rust backtrace through the existing crash-reporting library; no Python
frames are invented. Validation enables `RUST_BACKTRACE=1`; callers retain the
existing backtrace environment policy. Rust panics and errors in other threads or
processes are not intercepted. Python `ThreadingIntegration` is not claimed to be
implemented by this API.

The existing `Reporter::capture_exception` supplies source ordering: emit `crash`,
read/write `CarrotExceptionSent`/`CarrotException`, then SDK capture/flush. Params
behavior remains active when the fork/registration/hardware reporting gate is
false. Logger or Params failures before SDK calls remain visible as
`EntryError::Reporting`, retaining both the initial native error and the reporting
failure. SDK capture/flush failures keep their existing caught-error log policy.
An interrupt-warning logging failure propagates without entering the crash path,
matching the source's separate `except KeyboardInterrupt` branch.

The caller supplies an initialized Reporter. This API does not move Sentry manager
initialization into the child entry boundary, change gates/DSNs or add an automatic
error hook. Existing crash-reporting and logging code is reused unchanged.

## Process identity and external boundaries

Linux `PR_SET_NAME` exposes at most 15 bytes in `/proc/<pid>/comm`; registered
module identifiers are ASCII. The native executable remains visible in argv.
Python's `setproctitle` rewrites argv; this API deliberately does not claim that
identity behavior or perform unsafe argv rewriting. Full catalog identity remains
an explicit supervisor/integration concern; the complete daemon name is carried
in log context and Sentry tags.

Native dependencies include existing logging/libzmq, Params, the Sentry SDK and
HTTP/TLS stack, Linux naming/signal APIs, and the temporary original C++ msgq
boundary used by the validation child's fresh IPC owner. The production API has no
Python dependency. Python 3.12, pycapnp **2.1.0**, original msgq/Params bindings,
sentry-python 2.55.0 and setproctitle 1.3.7 are test-oracle dependencies.

## Reproducible validation

Build the native child and existing collector, then supply `PARAMS_BINDING` and
`MSGQ_PYTHON` from the existing original bindings. `OUTPUT` must be a fresh directory.
The checker creates temporary Params/build metadata, unique IPC namespaces and
loopback-only Sentry receivers. No host Params, vehicle, public Sentry project or
production publisher is used.

```sh
RUSTUP_TOOLCHAIN=1.94.0 cargo test --manifest-path rust/Cargo.toml -p openpilot-managed-entry --all-targets --locked
RUSTUP_TOOLCHAIN=1.94.0 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-managed-entry --all-targets --locked -- -D warnings
RUSTUP_TOOLCHAIN=1.94.0 cargo build --manifest-path rust/Cargo.toml -p openpilot-managed-entry -p openpilot-logmessaged --examples --bins --locked
PYTHONPATH="$MSGQ_PYTHON:.:rust/tools" uv run --no-project --python 3.12 rust/tools/check_managed_entry.py "$CHILD" "$COLLECTOR" "$PARAMS_BINDING" "$OUTPUT"
```

The matrix runs 22 scenarios through both the original and native collectors,
comparing 44 source/native pairs (88 isolated child executions): normal return,
explicit and real SIGINT, I/O and chained errors, disabled reporting, preparation,
name and context failures, early interrupts, daemon-tag failure, SDK capture/flush
failures, closed actual logger sockets, Params open/put failures, sent-flag policy,
global context precedence and Unicode daemon tags. The SDK failure cases inject
errors only at the external SDK call seam. Enabled non-fault scenarios use actual
Python/native SDK HTTP envelopes at loopback, with real exceptions and Params.

The child callbacks prepare a real file and create a fresh native msgq publisher;
the original callback verifies replacement of the actual Python Context. Both
publish the same body payload to an original msgq receiver. Fixture handshakes
establish subscriber readiness and drain diagnostics before observing the real
child exit; they do not substitute for the entry/error implementation. Tests retain
real signal response times, stdout/stderr, SDK calls with Params values at each
call, original cereal packets, persisted collector records, HTTP envelopes and
exit codes. Native location/thread/backtrace/SDK identity is retained and assessed
as native; it is not normalized into fictitious Python identity.

A generic GNU aarch64 build and QEMU execution are host evidence only. The runner
prefix can be supplied with `--runner QEMU -L SYSROOT`; the original Python oracle
and collectors continue to run on the host. Source and executable hashes, exact
commands, explicit observations and artifact paths are recorded in the evidence
ledger. Manager initialization, daemon adoption, complete normal startup/upload,
AGNOS/device execution and performance acceptance remain separate gates. The user
performs the first device comparison after the full project-owned runtime is ready.
