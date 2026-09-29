# Experimental Rust runtime port

Source baseline: `bin9208/openpilot` dev
`f3a92524d87be714f6b8b5f44ecdc8319a8c53d1` (MIT; original notices retained).
No production daemon uses this crate yet. M0 ports two scalar filters and Linux
process CPU measurement; it is not a complete openpilot runtime.

From `rust/`, using Rust 1.94.0, Python 3 and NumPy for the reference test:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 tools/check_reference.py
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

Design and progress: `../docs/rust-port/`. Remaining migration: `port-status.json`.
