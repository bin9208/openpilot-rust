# Native Wi-Fi manager

Issue [#135](https://github.com/bin9208/openpilot-rust/issues/135), shared UI [#125](https://github.com/bin9208/openpilot-rust/issues/125), full runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).

`rust/crates/wifi` ports `openpilot/system/ui/lib/{wifi_manager,networkmanager}.py` into a typed Rust library. Original source and licensing remain intact. The UI owns `WifiManager`, submits typed commands, reads snapshots and drains callbacks on its own thread. Runtime policy and settings construction do not execute Python.

Two private libdbus connections preserve the source command/monitor separation. A current-thread Tokio runtime owns asynchronous requests, scan and monitor tasks. Shutdown cancels pending replies, aborts owned tasks, closes connections and joins the worker. The command queue has 256 slots and returns an explicit full-queue error; snapshots and callback delivery remain synchronized. Methods retain the source's unbounded reply wait during normal operation.

The implementation preserves adapter retry, scan cadence and sorting, duplicate SSID selection, missing access-point handling, security flags, Unicode SSIDs, saved connections, authentication callbacks, epoch checks across connection lookup, IPv4 metadata, metering, hotspot settings/password/reactivation and forwarding policy. The existing process supervision helper runs the source sysctl command. Native external dependencies are libdbus, NetworkManager and its system D-Bus API, Linux networking, Params, logging and the command helper.

## Evidence

`check_wifi_policy.py` executes unchanged source state handlers and compares 781 cases containing 2,322 transitions, including connection races, scan boundaries and security combinations. The comparison passed. Focused package build, warning-denying Clippy and Rust transition tests passed before the private-bus comparison.

`check_wifi_runtime.py` starts an owned `dbus-daemon` and a NetworkManager protocol fixture. It runs both the actual Python manager with the original Params extension and the native executable, confirms the native `/proc/PID/exe`, and compares nine snapshots plus mutation arguments and callbacks exactly. Both existing-hotspot and create-hotspot scenarios passed. Scenarios include saved/new activation, hidden-network credentials, metering, wrong password, tethering activation, password change/reactivation, deactivation and forgetting. UUID text alone is normalized. Scan callback repetition is excluded because scan scheduling is wall-clock dependent; deterministic scan policy is covered separately. All other callbacks and mutation settings are compared.

The sysctl executable is replaced only within each owned child environment by a recorder; the observed command is `sysctl net.ipv4.ip_forward=0`. No host NetworkManager, wireless adapter or forwarding setting is modified. The source and native workers both exit zero and the private bus is reaped. Local artifacts are retained under `.analysis/scratch/2026-10-01-rust-wifi/private-runtime-1/`, including source/native results, calls, stderr, executable identities and shutdown times.

```sh
export CARGO_INCREMENTAL=0
cargo build --manifest-path rust/Cargo.toml -p openpilot-wifi -p openpilot-process-supervision --bins --examples --locked -j2
PYTHONPATH=. uv run rust/tools/check_wifi_policy.py --binary TARGET/debug/examples/wifi_trace --output EVIDENCE/policy.json
PYTHONPATH=.:rust/tools uv run rust/tools/check_wifi_runtime.py --binary TARGET/debug/examples/wifi_native --launcher TARGET/debug/openpilot-process-child --binding PARAMS_BINDING.so --output EVIDENCE/runtime
```

Check available disk against the repository reserve before builds or dependency installs. `PARAMS_BINDING.so` is produced by `rust/tools/build_params_python.py`. The runtime checker declares its Python dependencies inline and requires `dbus-daemon`. Exact-SHA Actions and shared UI integration remain parent gates. Host comparisons do not establish device networking, AGNOS ABI, full normal startup/upload or CPU savings. The user's first C3X comparison remains deferred until the complete runtime candidate is ready.

Docs-Not-Needed: isolated shared UI library; no production selection or user-visible setting changes.
