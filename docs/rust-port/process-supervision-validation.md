# Native process supervision boundary (#84)

`rust/crates/process-supervision` ports the lifecycle operations in
`openpilot/system/manager/process.py`. It provides `ManagedProcess`, native
executable descriptors, persistent daemon descriptors, `ensure_running`, and
original cereal `ManagerState.ProcessState` encoding. It does not select any
production process or implement the manager main loop.

## Lifecycle contract

`prepare` is the original native no-op. `start` finishes an outstanding stop
before launching again, and an existing handle makes start idempotent even
when its child has exited. A dead handle is retained until stop or
`ensure_running` cleans it up. Signed signal exits and ordinary exit codes
retain the original state fields: name, running, shouldBeRunning, pid and
exitCode. A cleared handle resets the numeric/boolean state fields.

The first nonblocking stop sends the selected signal and returns before
joining. A later stop joins even when it also requests `block=false`. The
join polls at one millisecond intervals for five seconds; retry then sends
SIGKILL and reaps. `retry=false` retains a live, shutting-down handle after
the timeout, including the source's misleading `is dead with None` message.
Explicit signals override the normal SIGINT or configured SIGKILL choice.

`ensure_running` first cleans exited handles independently of the crash-restart
flag, then evaluates enabled/not-run/predicate gates in input order. It stops
undesired processes in this first pass and starts desired entries in a second
pass. The crash-restart branch remains between predicate evaluation and list
insertion: a child can exit during a predicate. Callers supply a predicate by
the registered process name, capturing started, Params and CarParams as needed;
the production catalog and its predicates remain separate integration work.

All entries of one supervisor share a `ProcessLog`, wrapping one existing
source-compatible producer. This preserves stop/start diagnostic ordering at
the collector as well as message text and severity. Metadata identifies actual
Rust callsites, OS process/thread identity, runtime language, source commit and
source-tree state. It does not fabricate Python filenames or line numbers.

## Native child execution

Install `openpilot-process-child` with the translated runtime and pass its
explicit path in descriptors. The manager starts this native helper before
target cwd/exec validation. An invalid target, cwd, empty argv or embedded NUL
therefore fails in a real child with exit 1. The parent retains its handle.
Missing helper artifacts and descriptor-storage failures are infrastructure
errors returned to the caller, not successful target launches.

The descriptor lives in a mode-0600 temporary file, owned with the native child
handle. Byte-preserving argv/cwd serialization avoids using the child's stdin
for a control channel. Ordinary native launches inherit OS stdin, stdout,
stderr and other inheritable descriptors. MANAGER_DAEMON is resolved before
chdir; argv conversion occurs afterwards. Relative cwd is joined to the
supplied repository root; absolute cwd retains normal path-join behavior.

Python's execvp searches candidates after errors and never treats ENOEXEC as
permission to run a shell. The helper preserves that search and error priority
using the safe [nix execve API](https://docs.rs/nix/0.31.3/nix/unistd/fn.execve.html).
There is no shell fallback or unsafe fork in the Rust implementation. Child
startup errors use native diagnostics instead of Python tracebacks.

The exported child entrypoint is only for the fresh, single-thread helper
executable. It replaces the process image and, in persistent mode, closes
inherited descriptors. Supervisors use the descriptors and ManagedProcess API;
they must not call the child entrypoint in their own process.

## Persistent daemons

Persistent descriptors name a native executable, an identity substring and a
known INT Params key (normally AthenadPid). Startup lazily opens Params, checks
the saved PID and `/proc/<pid>/cmdline`, and reuses a matching process across
supervisor instances. Missing/dead/wrong-identity entries launch a new process.
Invalid UTF-8 in a live cmdline remains an error. Nonpositive PID values cannot
identify a `/proc` process and cause a new launch without signaling a group.

The source uses typed `Params.get`, not C++ `get_int`. Decimal strings permit
ASCII whitespace, signs and inter-digit underscores. Invalid bytes emit the
original conversion warning and act as missing; valid integers outside the
OS PID range propagate a range error. The default CPython 4300-digit boundary
is retained. PID writes retain the raw decimal storage form.

Persistent startup creates a separate process group (setpgrp, not setsid),
uses `/dev/null` for all standard streams and closes other inherited FDs in the
single-thread helper. A close-on-exec error pipe preserves parent-visible exec
errors before Params is written. The helper keeps its PID when exec succeeds.
Persistent stop and signal do nothing; manager state has no child handle.
Unwaited persistent children are retained for reaping at later launches, like
Python's Popen cleanup registry. Neither registry stops a live daemon when a
manager exits. Consumers must explicitly stop ordinary managed children;
persistent daemons intentionally survive that cleanup.

The inherited PID-write defect is preserved and tracked in
[#90](https://github.com/bin9208/openpilot-rust/issues/90): Cython ignores a
Params.put I/O failure, so subsequent starts can create duplicate persistent
daemons. The isolated directory-at-AthenadPid scenario demonstrates this in
both implementations and explicitly cleans up both children. This port does
not claim to fix the source defect.

## Validation and boundaries

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-process-supervision --all-targets --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-process-supervision --bins --examples --locked
python rust/tools/check_process_supervision.py \
  --binary rust/target/debug/examples/process_trace \
  --launcher rust/target/debug/openpilot-process-child \
  --fixture rust/target/debug/examples/process_fixture \
  --binding /path/to/original/params_pyx.so --output /tmp/process-supervision-evidence
```

The checker needs the original cereal/msgq binding on PYTHONPATH and actual
Cython Params. It executes unchanged source class/function bodies; only
repository/Params/log paths and the persistent Python executable target are
replaced with isolated native fixtures. Real child processes, signals,
monotonic waits, Params storage, procfs and logging transport remain in use.
Every state packet is decoded with the original cereal schema. Logs, command
records, state packets, exit statuses, elapsed times, readiness records and
empty-descendant cleanup observations are captured. `--group` can select
lifecycle, ensure or persistent scenarios.

The implementation is tested as a reusable host library and generic GNU
aarch64/QEMU boundary. The ARM supervisor and child helper execute under QEMU;
their managed target is the same native host fixture used by the original
Python supervisor. Fixture exit requests use atomic file replacement so the
predicate-race scenario observes a complete request. AGNOS, device behavior,
full runtime startup and CPU savings are not established. No original production process, GPIO, vehicle,
C3X or NAS is used. Exact commands, failed attempts, frozen binary hashes,
source hashes and criterion-to-artifact links live in the issue worktree's
`.omo/evidence/process-supervision/evidence.json`.

PythonProcess preimport/import preparation is intentionally not emulated;
translated applications use explicit native executable descriptors. Python
launcher daemon logging/Sentry tags, uncaught-error capture and
CarrotExceptionParams integration are separate managed-entry work (#86).
Manager initialization, registration, catalog, startup UI and full candidate
integration remain unported here. `manager_status` stays `not_ported`.

External dependencies include Linux process/procfs/filesystem APIs, libc via
nix/rustix, the existing native logging/Params/cereal libraries, and their
current libzmq/CXX transport dependencies. Parent owns CI and dev integration.

Docs-Not-Needed: internal optional lifecycle library with unchanged production
startup, settings and process selection.
