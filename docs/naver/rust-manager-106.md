# Native manager initialization and orchestration (#106)

Issue: https://github.com/bin9208/openpilot-rust/issues/106. Full-runtime work:
https://github.com/bin9208/openpilot-rust/issues/1. Source implementation remains
unchanged at `openpilot/system/manager/manager.py`, `camera_config.py`, and
`helpers.py`; existing repository licensing and provenance apply.

Core commit: `3496fd8a`. Native prerequisites integrated from `2f0fdde7`
(#97/#98/#101/#105/#107/#110/#112).

## Implemented candidate

`openpilot-manager` provides `Main::run`, ordered manager initialization and a
continuous transition loop. `NativeStartup` composes native build metadata,
registration, checkout tracking, logging/Sentry policy and process preparation.
`NativeRuntime` uses real SubMaster/PubMaster, Params, catalog predicates,
process supervision and atomic watchdog writes. No Python launch fallback or
production manager selection is added.

Initialization preserves boot snapshot before mutation, four separate Params
clears plus release-only clearing, RecordFrontLock, missing/invalid typed defaults,
wide-camera environment latch, metadata/registration/logging ordering and the
finally-style boot-lock release. Supported-car names are compiled from a build-time
export of all seven unchanged opendbc brand definitions, including duplicate names;
files are sorted with a trailing newline and each brand has its own error catch.
`set_defaults(..., true)` implements the source reset helper separately.

The loop preserves initial offroad Params/ensure, NOBOARD/BLOCK/unregistered/lite
ignore rules, onroad/offroad and known-panda ignition edges, ordered stop-before-start
process control, managerState validity/process fields/checkout reboot flag, the
one-second poll and watchdog exception boundary. All three exit flags are scanned
in source order, leaving the last exit reason, while hardware action precedence
remains uninstall, reboot, shutdown. Cleanup is two complete process passes.
Cooperative SIGTERM/SIGINT observes the next poll boundary (up to one second), runs
cleanup and skips hardware exit. PREPAREONLY avoids constructing runtime IPC/signals.

`boot_lock::release` validates an owned File's device/inode before unlocking and
closing it. A wrong identity leaves the lock owned by the caller. A native launcher
must transfer File ownership; this library does not reinterpret a legacy shell
integer descriptor using unsafe code.

## Focused host evidence

Local evidence: `.omo/evidence/manager-106/` in the issue worktree. Commands below
assume the Rust target directory's `debug` path is `$B`, the Python environment is
`$PYTHON`, and the previously built unchanged Cython Params module is `$BINDING`.
The source oracle imports the actual manager module and uses actual Cython Params;
only external process, hardware, registration, clock, IPC and logging boundaries
are isolated. All temporary Params and subprocesses belong to the fixture.

- `cargo build --manifest-path rust/Cargo.toml -p openpilot-manager --examples
  -p openpilot-process-supervision --bin openpilot-process-child -j 2`
- `$PYTHON rust/tools/check_manager.py $B/examples/manager_trace $BINDING OUTPUT`:
  exact trace, full Params byte maps and environment for normal transitions/all
  exit flags, registration failure/unlock, poll exception/cleanup, PREPAREONLY,
  interruption, typed-default edges and explicit reset-to-default helper.
  `--scenario NAME` runs a focused regression without repeating passing scenarios.
- `$B/examples/manager_native OUTPUT $B/openpilot-process-child`: real isolated
  msgq packets, published live owned PID, child reap on offroad/cleanup, watchdog,
  known versus unknown panda ignition, shutdown boundary, real SIGTERM and explicit
  unavailable-daemon error. Writes `summary.json`; does not call real hardware.
- `$B/examples/manager_adapters OUTPUT $B/openpilot-process-child`: actual
  NativeStartup/NativeBoot initialization using native build metadata, checkout
  tracking, registration with an absent synthetic key (no HTTP), typed hardware
  identity adapter, logging/Reporter and 138 Hyundai doc names; snapshot child sees
  Version before initialization changes it and the owned boot lock is released.
  NativeExit dispatches all three HardwareControl actions into a recording Platform
  and captures and flushes a synthetic exception through the native Reporter. Writes
  `summary.json`; no host board operations or Sentry delivery.
- `cargo test --manifest-path rust/Cargo.toml -p openpilot-manager -j 2`:
  wrong-file boot-lock rejection retains lock; correct file unlocks/closes.
- `PYTHONPATH=. $PYTHON rust/tools/generate_manager_cars.py
  rust/crates/manager/data/cars --check --binding $BINDING`: all 341 source names
  across seven brands match the compiled data.
- `cargo fmt --manifest-path rust/Cargo.toml -p openpilot-manager -- --check`;
  `cargo clippy --manifest-path rust/Cargo.toml -p openpilot-manager --all-targets
  -j 2 -- -D warnings`; `git diff --check`.

The initial malformed-default fixture exposed an overlong fixture logging socket
path, not a source manager defect; the fixture now uses an isolated inproc endpoint.
The initial Clippy bool-comparison finding was corrected. Successful scenarios
were not used as substitutes for fixing these failures.

## Integration boundaries

The candidate is an embeddable manager, not an installable complete runtime. The
full daemon catalog remains explicit: caller-supplied native process bindings are
required and a selected missing implementation returns an error. UI text-window
startup failure display, nonblocking stdout wrapper and native launcher ownership
remain startup integration work. NativeBoot directly calls #97's snapshot worker;
NativeExit directly calls #105's HardwareControl and the native Reporter. The
NativeStartup composition fixture uses #107's hardware-info registration bridge.
The installation must bind the loggerd `bootlog` path to the native candidate and
provide the platform/SDK/launcher objects; those choices are not made automatically.
Because bootlog shares loggerd infrastructure, building this manager also requires
the existing FFmpeg/libclang native development dependencies. The local adapter
build initially lacked FFmpeg pkg-config paths; it passed after reusing the existing
logger native sysroot (no package installation).

Cloud workspace/ARM validation and exact-SHA Actions evidence belong to the parent
integration. No AGNOS artifact installation, vehicle connection, device reboot,
external registration, Sentry delivery or user acceptance was performed. First
device comparison remains blocked on the complete project-owned runtime and
normal existing log upload path. No CPU savings or runtime-completion claim follows
from this host candidate.

Docs-Not-Needed: isolated runtime implementation; no selected production process,
user setting, or user-facing behavior changed.
