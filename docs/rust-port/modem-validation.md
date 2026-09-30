# Native serial modem and PPP daemon

Issue: [#113](https://github.com/bin9208/openpilot-rust/issues/113), under full-runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).
Source: `openpilot/system/hardware/tici/modem.py`, unchanged from the issue branch base `d02e5358`.
Implementation: `rust/crates/modem`, binary `openpilot-modem`.

The daemon retains the five source states, one-second state cadence, 60-second ICCID cadence, five-second AT read timeout, initialization commands and echo check, identity/SIM/registration observations, APN/roaming policy, three-failure PPP retry limit, data-port DTR reset, radio observations, IPv4 route/DNS setup, byte counters, atomic mode-0644 JSON publication and shutdown cleanup. The shared advisory lock uses the same Linux flock semantics and default `/dev/shm/modem.lock` path as LPA. Native owned children are reaped after termination while retaining their exit status for reconnect handling.

Failed startup propagates before stop, matching source main order. External commands use the native process launcher with descriptor closure and exec-error handshakes; PPP/sudo streams are discarded and ip stdout remains a temporary file under the two-second deadline. The focused process-boundary gate captures inherited-descriptor absence, ENOEXEC without shell fallback, and failed-startup command absence against the source.

The runtime uses serialport 4.10.1 (default features disabled), native filesystem/process APIs and rustix flock/monotonic time. Existing workspace dependency versions remain unchanged. `sudo`, `pppd`, `/usr/sbin/chat`, `ip`, `resolvectl`, `killall` and conditional `systemctl` remain explicit external native dependencies. There is no Python runtime fallback. Default paths and commands address actual device resources; the fixture gate always supplies its own configuration and executable paths.

## Focused host gate

Run from the repository root with Python 3.12 and pyserial 3.5 installed only for the unchanged-source oracle:

```sh
RUSTUP_TOOLCHAIN=1.94.0 cargo build --manifest-path rust/Cargo.toml -p openpilot-modem -p openpilot-process-supervision --bins --examples --locked -j2
RUSTUP_TOOLCHAIN=1.94.0 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-modem --all-targets --locked -j2 -- -D warnings
python rust/tools/check_modem_process_boundary.py --target rust/target --evidence /tmp/modem-evidence
python rust/tools/check_modem.py --binary rust/target/debug/openpilot-modem --trace rust/target/debug/examples/modem_trace --evidence /tmp/modem-evidence
```

The differential gate imports the actual source, substitutes only filesystem/serial timeout/executable locations, and runs source and native code against independent owned PTYs and harmless process fixtures. It compares complete state snapshots (excluding the variable monotonic timestamp), AT command order, PPP arguments, route/DNS commands, transition results and failure counters. It covers normal connection, modem errors/timeouts, shared-lock contention, echo/identity retries, absent SIM, blocked roaming, malformed observations, PPP give-up, invalid IPv4, route failure/retry and DNS failure.

The continuous native process gate verifies its `/proc/PID/exe`, then exercises startup and connection, APN reconnect, ICCID reconnect, interface disappearance, AT-port disappearance, SIGTERM, state-file removal and child reaping. Captured JSON also records modem-manager mask/restore and DNS commands. Fixtures never execute real sudo, pppd, ip, systemctl, resolvectl, route operations or hardware. PTYs do not emulate modem DTR hardware; the retry tests verify the source-compatible unavailable-data-port recovery path, while physical DTR effects remain unvalidated.

Artifacts: `results.json`, per-scenario source/native JSON, `lifecycle.json`, `daemon.stderr`, plus build/clippy logs. Local evidence is recorded under `.omo/evidence/modem-113/` in the parent workspace. Runtime source modules each own a single concern; the state machine is 249 nonblank/noncomment lines, near the module size limit, so additions should split its handlers. JSON configuration and operations are typed at ingress; state/operation dispatch is exhaustive and no project unsafe code is used.

## Remaining integration and validation

Add the same Python gate to the startup-runtime Actions job, alongside pyserial and the native binary/examples build. Generic ARM workspace compilation includes the new crate. Exact-SHA Actions/ARM results are owned by the integration task and were not run locally for this slice.

Production process selection is unchanged. LPA profile provisioning remains separate. This host evidence does not establish physical serial/PPP/network behavior, AGNOS ABI compatibility, complete runtime startup/upload, vehicle acceptance, CPU savings or thermal improvement. No device was contacted and no first-device-test handoff is requested.
