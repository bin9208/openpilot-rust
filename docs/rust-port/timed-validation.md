# Native timed support daemon (#70)

`rust/crates/timed` ports `openpilot/system/timed.py`,
`openpilot/system/timezone_helper.py`, `openpilot/common/time_helpers.py` and
`get_gps_location_service` into an optional native executable. Production manager
selection remains unchanged. This is an intermediate full-runtime increment,
not a complete runtime or a device-test candidate.

The daemon selects `gpsLocationExternal` only when `UbloxAvailable` is true,
publishes `clocks` after each one-second SubMaster update, and preserves the
source's outer-GPS-validity-independent, updated/fix/two-second freshness checks.
Clock correction retains the ten-second difference threshold, integer UTC epoch
command, inclusive GPS date bounds, strict wall-clock bounds and ten-second sleep.
Local date interpretation includes systemd mtime plus one day and DST behavior.

Timezone priority remains app > wifi > gps > unknown. Unset/unknown sources retry
after strictly more than 30 seconds; GPS sources after more than 300 seconds.
Internet failure permits fresh-GPS longitude fallback, using Python ties-to-even
rounding and reversed Etc/GMT signs. Equal symlink targets skip commands while
still persisting name/source. Failed commands retain source side-effect order.
The HTTP lookup preserves the original endpoint, user agent, redirects and
five-second per-I/O timeout. `http-transport` extracts uploader's existing socket
timeout adapter without removing its post-syscall monotonic deadline check;
uploader retains ten-second timeouts and slow-progress response behavior.

## Validation

The executable fixture uses the actual runtime functions with injected clock,
paths and loopback endpoint. Its PATH contains only a Python command recorder;
no actual sudo, date, rm or ln command runs. The unchanged-source oracle substitutes
only external clock, Params, command and I/O seams. Continuous scenarios use the
original native msgq bindings, actual cereal packets and ZMQ PUSH/PULL logging.

```sh
cargo build --manifest-path rust/Cargo.toml -p openpilot-timed -p openpilot-uploader --bins --examples --locked
# Set PYTHONPATH to original msgq binding, repository root and rust/tools.
python rust/tools/check_timed_reference.py --binary rust/target/debug/examples/timed_fixture --output /tmp/timed-policy
python rust/tools/check_timed_http.py --binary rust/target/debug/examples/timed_fixture --output /tmp/timed-http
python rust/tools/check_timed_logging.py --binary rust/target/debug/examples/timed_fixture --output /tmp/timed-logging
python rust/tools/check_timed_daemon.py --binary rust/target/debug/examples/timed_fixture --output /tmp/timed-daemon
python rust/tools/check_timed_lifecycle.py --binary rust/target/debug/openpilot-timed --output /tmp/timed-entry
```

Evidence is indexed in `.omo/evidence/timed/evidence.json` in the issue-70 worktree.
Policy scenarios cover thresholds, priorities, errors and local-time boundaries;
HTTP scenarios include real five-second stalls/late replies and a progressing
response exceeding five seconds. Closed-log scenarios preserve command/Params
failure order. Continuous daemon scenarios exercise both GPS topics, stale/no-fix
rejection, actual ten-second sleep and command recorder output. The production
entrypoint is separately exercised without GPS, with app timezone selected, for
clock publication, bounded exit, signals and CLI errors. Generic GNU aarch64/QEMU
execution establishes host emulation only. Uploader HTTP and unscaled ten-second
timeout oracles guard the shared extraction.

Remaining external dependencies include original msgq/CXX, libzmq, zoneinfo,
systemd metadata, OS clocks, privileged sudo/date/rm/ln and Rust HTTP/TLS libraries.
No host clock/timezone changes, vehicle access, real logs or public-network requests
are part of these checks. Real privileged commands, AGNOS execution, normal full
startup and upload integration, CPU savings and the user's first C3X comparison
remain unvalidated. Cloud checks belong to the root integration handoff.

Docs-Not-Needed: internal optional runtime port preserving existing settings and
production process selection.
