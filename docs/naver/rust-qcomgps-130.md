# Native Qualcomm GNSS host candidate (#130)

Issue: [#130](https://github.com/bin9208/openpilot-rust/issues/130). Whole-runtime delivery remains [#1](https://github.com/bin9208/openpilot-rust/issues/1).

The `openpilot-qcomgpsd` package ports project-owned policy from
`openpilot/system/qcomgpsd/{qcomgpsd,modemdiag,nmeaport,structs}.py` at baseline
`5afd79cb`. Original licensing and provenance remain in the repository. The Rust
build script reads declaration tables from `structs.py` as data; neither the build
script nor the runtime executes Python. Python is used only by the independent
source-comparison and synthetic hardware test drivers.

## Implemented scope

- Native HDLC escaping, CCITT CRC, fragmented/coalesced diagnostic reads, serial
  configuration, advisory exclusive locking, AT shared lock, echo/terminal parsing,
  retries, modem readiness and disconnect/error handling.
- NV OEMDRE enable/read, firmware log-range discovery and masks, cold/hot-start AT
  commands, GNSS configuration, UTC assistance time, OEMDRE enable, and teardown.
- The five source-selected reports: GPS and GLONASS measurements, OEMDRE
  measurements, OEMDRE satellite polynomials, and position. Cap'n Proto topics,
  validity, status bits, timestamps, units, velocity axes, invalid GPS-week sentinel,
  clipped vertical accuracy and source field omissions follow the source.
- A separate native assistance worker: stale-file removal, optional alternate file,
  bounded streaming HTTP, temporary-file rename, retry wait, `mmcli` injection,
  ignored injection failure after five attempts, late reconfiguration, fix-driven
  cancellation and worker cleanup.
- GPIO34 antenna setup and signal teardown, existing native logging transport,
  continuous `qcomGnss`/`gpsLocation` publication through the existing msgq adapter.
- Separate `nmeaport` debug executable, typed GNCLK/GNMEAS values, file reopening
  after parse/open errors, idle signal shutdown, and setup behavior. Its process
  guard recognizes both the original `qcomgpsd` name and the native executable name.

The primary daemon reads diagnostics only. The NMEA helper is not a second producer
and does not replace diagnostic GPS publications.

## Preserved source quirks and boundaries

The NMEA checksum helper checks delimiter position but never compares checksum
bytes. Consequently an otherwise well-formed sentence ending in `*ZZ` is accepted.
The debug helper's setup compares a bytes operand with `at_cmd`'s nonempty string
result and raises a type error. That failure is preserved as a native protocol
error, including its message. All-empty replies follow the source configuration
and reboot path with exit code 2. These are existing source behaviors, not fixes
claimed by this port. The debug reader is independently usable for diagnostics.

Malformed diagnostic framing/report lengths and unsupported report versions are
fatal, rather than silently retried. Unsupported opcodes/log types are ignored as
in the source. Modem-readiness retries, setup retries and NMEA-file reopening are
separate source retry paths. The position timestamp retains the source's fixed
18-second GPS leap offset; this port does not change that policy.

Serial polling makes native waits signal-responsive. The assistance process is
owned and reaped on native termination; successful GPS fixes stop future download
attempts. Firmware/serial timing, physical GPIO, AGNOS ABI and a vehicle have not
been tested. No production daemon selection changed. Generic host/aarch64 builds
are not a device installation package or performance measurement.

## Focused validation

Local evidence is under `.omo/evidence/qcomgps-130/` in the issue worktree. The
ledger records exact invocations, observables and artifact paths. The principal
results are:

| Scenario | Observable | Artifact |
| --- | --- | --- |
| Source diagnostic publication comparison | 204 source/native cases agree; all five reports, status bits, versions, truncation, sentinels, timestamp edges | `publications.json`, `reference.log` |
| Native PTY/msgq daemon | All five message types agree with source; actual event timestamps are recent; primary and assistance executables are native | `daemon/*/publications.json`, `daemon/*/*.capnp`, `daemon/*/result.json` |
| Complete setup and teardown | AT/NV/log-mask/OEMDRE transcript equals unchanged source, normalizing only current UTC text | `daemon/setup-source.json` |
| Late assistance and signal stop | Late HTTP file causes reconfiguration/injection; SIGTERM clears antenna and log masks; exit 0 | `daemon/late-assistance-signal/` |
| Injection failure | Five failing external-command attempts are ignored; loop and teardown still succeed | `daemon/injection-failure/` |
| Serial/report failure | Corrupt CRC, length mismatch and PTY disconnection each cause exit 1 | `daemon/{crc,length,disconnect}/` |
| Assistance worker | Alternate replaces stale file, HTTP 404 body follows source handling, oversized data is not promoted, failed connection retries | `assistance/*/result.json`, `assistance.log` |
| Separate NMEA helper | Source values/checksum behavior; missing-file retry, parse-error reopening, idle stop; nonempty setup failure and empty-response reboot | `nmea/{reference,runtime,setup}.json`, `nmea.log` |
| Bounded Rust checks | Package tests, format and warning-denying Clippy | `test.log`, `fmt.log`, `clippy.log` |

The test drivers create only owned PTYs, loopback HTTP listeners, private files,
private GPIO surrogates, isolated msgq namespaces and external-command fixtures.
No real modem, device, C3X, vehicle, NAS or route log is accessed.

## Portable CI commands

Run from the repository root, with the existing Rust/C++/Cap'n Proto build
prerequisites and Python `pycapnp` environment. Initialize the repository's cereal,
msgq and opendbc source dependencies. `CARGO_TARGET_DIR` may point to a coordinated
cache. Check free disk before each build and keep the repository's 25 GiB reserve
plus estimated growth.

```sh
export CARGO_INCREMENTAL=0
cargo fmt --manifest-path rust/Cargo.toml -p openpilot-qcomgpsd -- --check
cargo test --manifest-path rust/Cargo.toml -p openpilot-qcomgpsd -p openpilot-manager-catalog --locked -j2
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-qcomgpsd --all-targets --locked -j2 -- -D warnings
cargo build --manifest-path rust/Cargo.toml -p openpilot-qcomgpsd --bins --examples --locked -j2
export PYTHONPATH=.
target_dir="${CARGO_TARGET_DIR:-rust/target}"
python rust/tools/check_qcomgps_reference.py "$target_dir/debug/examples/qcom_trace" evidence/qcomgps/publications.json
python rust/tools/check_qcomgps_daemon.py "$target_dir/debug/openpilot-qcomgpsd" "$target_dir/debug/examples/qcom_subscribe" evidence/qcomgps/daemon
python rust/tools/check_qcomgps_nmea.py "$target_dir/debug/examples/nmea_trace" "$target_dir/debug/nmeaport" evidence/qcomgps/nmea
python rust/tools/check_qcomgps_assistance.py "$target_dir/debug/openpilot-qcomgpsd" evidence/qcomgps/assistance
```

The leader integrates these focused scenarios with the existing required Actions
checks and generic aarch64 build. Exact-SHA cloud checks remain separate from the
local host evidence recorded here.

## Remaining integration and dependencies

Native external dependencies remain: Linux UART/termios/flock and GPIO sysfs,
Qualcomm/Quectel firmware, ModemManager/`mmcli`, HTTP/TLS, and the existing original
msgq C++ adapter. `manager-catalog` records candidate availability only; its source
process selection remains unchanged. UBlox #129, UI, CI workflow editing, user
guides and production selection are outside this change.

Do not close the whole-runtime delivery gate or request a device comparison from
this component's host results. Complete project-owned runtime selection, normal
startup and the existing upload path must be ready first; the user performs the
first device comparison.

Docs-Not-Needed: isolated experimental runtime component; no production selection,
user-visible setting or public guide behavior changes.
