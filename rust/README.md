# Experimental Rust runtime port

Source baseline: `bin9208/openpilot` dev
`f3a92524d87be714f6b8b5f44ecdc8319a8c53d1` (MIT; original notices retained).
No production daemon uses this workspace yet. It contains two scalar filters,
Linux process collection, full cereal bindings, raw Params storage and a bounded
procLog producer. The transport still uses original C++ msgq through CXX. This
is not a complete openpilot runtime.

Use Rust 1.94.0, a C++17 compiler, Cap'n Proto 1.0.1 and its development headers.
Python reference checks use NumPy 2.4.6 and pycapnp 2.1.0. From `rust/`:

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

The separate `rust-c3x-static-probe-pending-device-test` Actions artifact is a
checksummed, static aarch64 diagnostic candidate. Unlike the generic GNU build,
it is intended for the user's explicitly requested first target probe. It has
not passed C3X validation. See [C3X probe steps](../docs/rust-port/c3x-probe.md)
and [evidence and limitations](../docs/rust-port/m1-validation.md).

Design and progress: `../docs/rust-port/`. Remaining migration: `port-status.json`.
