# Experimental Rust runtime port

Source baseline: `bin9208/openpilot` dev
`f3a92524d87be714f6b8b5f44ecdc8319a8c53d1` (MIT; original notices retained).
Production daemon selection has not switched to this workspace. It contains
native candidates for the manager, hardware/startup services, logging and upload,
model execution, vehicle interfaces, control/state management, camera, UI,
encoding and navigation. Remaining project-owned processes and integration work
are listed in [port-status.json](port-status.json). This is not yet a complete
normal-startup runtime candidate.

The msgq/VisionIPC implementation is now Rust, including shared queues,
descriptor exchange, buffer ownership and generic/ION allocation. Original C++
peers remain independent test oracles. Host, ARM, sanitizer and memory checks are
recorded in [native IPC validation](../docs/naver/rust-native-ipc-194.md);
individual consumers continue through composition checks. Earlier component
receipts retain the transport boundary used when they were collected.

External libraries remain explicit dependencies: codecs, graphics, numerical
solvers/kernels, ZeroMQ, operating-system drivers and firmware are not rewritten
by the language port. Narrow C/CXX adapters still exist around some external
APIs. Build-time Python generators and original-source comparison programs are
separate from native daemon execution.

Use Rust 1.94.0, a C++17 compiler, Cap'n Proto 1.0.1 and its development headers.
Native features require the pinned libraries and build environment documented
by each component and [.github/workflows/rust.yml](../.github/workflows/rust.yml).
Python oracle dependencies are pinned per check; use the corresponding workflow
environment rather than an arbitrary system installation. Basic checks from
`rust/` include:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 tools/check_reference.py
cargo build --workspace --examples --locked
python3 tools/check_proclog_reference.py
python3 tools/check_params_reference.py
cargo run --release --locked --bin cpu-sample -- 1000
```

Check free space before every build, install or large copy: retain at least
25 GiB plus expected growth, and recover 35 GiB before resuming if below the
floor. Disable incremental compilation and prefer bounded package builds while
developing. The complete required CI matrix remains the integration gate.

`cpu-sample` reads Linux /proc twice, using monotonic sample timestamps and
`getconf CLK_TCK`. It prints TSV to stdout and never publishes cereal or CAN.
100% is one logical core; a multi-threaded process may exceed 100%. `last_processor`
is the last observed main-thread CPU, not affinity or full thread residency.
Vanished/unreadable/malformed records, PID reuse and counter resets are skipped.
Short intervals have coarse tick resolution. The diagnostic itself has overhead;
measure that before running continuous production instrumentation.

Filters accept the tested scalar domain: finite inputs, dt > 0, rc >= 0. They do
not yet replace array-valued Python uses or validate all invalid timing inputs.
The reference test executes the actual source classes on deterministic traces.

Linux host tests and aarch64 GNU builds do not prove AGNOS compatibility, vehicle
equivalence, lower CPU/temperature, or YOLO capacity. An older device glibc may
require a target sysroot; do not install the generic CI artifact on a vehicle.

`openpilot-proclogd --help` describes bounded file/stdout/isolated-publish modes.
Publication requires a `rust-probe-NAME` namespace, uses the canonical 10 MiB
procLog queue, and waits at most three seconds for an isolated subscriber.
The default interval is 2000 ms (0.5 Hz). Rust-owned code never invokes Python.
The self-test only opens temporary Params; it does not use vehicle settings.

The older `rust-c3x-static-probe-pending-device-test` artifact and
[probe document](../docs/rust-port/c3x-probe.md) are intermediate engineering
history. The approved [delivery gate](../docs/rust-port/design.md) requires all
project-owned runtime conversion, normal startup and the existing log-upload
path before asking for the user's first device comparison. No component probe
or generic cross-build satisfies that gate. Device execution, CPU/thermal
improvement and vehicle acceptance remain unverified.

Design and progress: `../docs/rust-port/`. Remaining migration: `port-status.json`.
