# Native hardware information and runtime Paths (#98)

Tracking: [issue #98](https://github.com/bin9208/openpilot-rust/issues/98),
full runtime [issue #1](https://github.com/bin9208/openpilot-rust/issues/1).
This implements the read-only information layer from
`openpilot/system/hardware/{base.py,hw.py,pc/hardware.py,tici/hardware.py}`
and the `tici/iwlist.py` scan parser. Original source licensing and provenance
remain in place. It does not select a daemon or complete hardwared.

## API and boundaries

`openpilot-hardware-info` exposes `HardwareInfo`, its base defaults, `Pc`,
`Tici`, runtime selection, `paths::Paths`, hardware/platform flags,
`ThermalZone` and `ThermalConfig`. `HardwarePaths::default()` names the source
filesystem paths; `HardwarePaths::under()` and public path fields support an
explicit alternate filesystem. `Commands` permits an external-command adapter;
`NativeCommands` runs the existing read-only `sudo cat` and `iwlist` command
contracts. WPA uses real Linux abstract client addresses and Unix datagrams,
8192-byte receives, replacement UTF-8 decoding, unsolicited-event filtering,
per-operation timeouts and interrupted-call retries. No Python runtime is called.

The library includes serial/cmdline parsing, cached device model, OS version,
modem JSON, IMEI/SIM/network information, default routes, cellular and Wi-Fi
strength, metered-network policy, power/GPU/brightness observations, encoder
boot readiness and thermal discovery/configuration. Runtime Paths retain raw
Unix environment bytes, per-call environment reads, empty-override rules and
the source's path normalization and trailing separators. Hardware/platform
flags are captured separately from those environment reads.

Modem getters reread the state file. Missing files and JSON syntax failures
return an empty object; permission, invalid UTF-8, directory and JSON integer
length errors retain their distinct exception boundaries. JSON scalar types,
truthiness, arbitrary integers within the source decoder's limit, NaN/Infinity
and lone surrogates use the existing Python-compatible native JSON parser.
A numeric/list/null IMEI remains that value; there is no invented identifier or
string coercion. The current registration trait in #85 accepts optional strings,
so integration must widen or adapt that boundary with original-source tests.
This change does not modify registration.

`Number` preserves integer versus floating observations. Python integer parsing
uses Unicode 15.0 decimal digits, underscores, signs and the 4300-digit decimal
limit. Power defaults are applied only where the source catches read/parse
errors; later overflow still propagates. GPU and brightness catches retain their
own source defaults. Thermal discovery caches each zone index, including the
source's rescan behavior for a negative index; missing temperature files return
zero, while discovery/type/permission/parse errors are not hidden. Successful
model reads are cached across instances for the configured path; failures are
retried. Network keyfiles preserve strict parsing, DEFAULT inheritance,
continuations, source directory/file order and the first matching metered value.
SSID escapes retain the source's UTF-8 → unicode_escape → Latin-1 behavior.
The generated Latin-1 name/alias table carries the Unicode license; all entries
were checked with CPython 3.12's Unicode 15.0 lookup.

## Validation and limits

The oracle imports the unchanged original hardware modules and real Cython
Params binding. It redirects filesystem roots and the WPA endpoint, supplies
controlled uptime, and uses restricted `sudo`/`iwlist` executables. It does not
replace getters, parse functions, caches or base defaults with modeled answers.
Both implementations use actual temporary files, permission changes, subprocess
boundaries and local Unix datagram peers. Input files and command/socket captures
are synthetic and retained outside Git.

The scenario suite checks values **and scalar types**, exact finite float bits,
exception categories, actual command order/arguments and request/reply bytes.
It covers missing/truncated/malformed/permission failures, modem scalar edge
cases, model and thermal cache lifetime, route ordering, keyfile/SSID behavior,
WPA events/timeouts/truncation, and path environment bytes. Initial native tests
failed for the missing API, then passed after implementation. A source comparison
also exposed the `HOME=//` expanduser mismatch; the regression was retained and
the normalization corrected. Focused modem regressions retain the Python 3.12
uppercase results for U+0264 and U+1C8A, whose Rust Unicode 17 mappings differ.

Host validation invocations, source/binary hashes and artifacts are recorded in
`.omo/evidence/hardware-info-98/final/evidence.json` and its `host/manifest.json`.
The updated host matrix passed 136 scenarios, 3,110 observations and 70
datagram requests per implementation. Package fmt, clippy, four tests and the
example build passed locally. Generic GNU
aarch64 build and source comparison are assigned to GitHub Actions; they are
not claimed as completed local validation. No AGNOS or device result is implied.

Board writes, reboot/shutdown/uninstall, power saving and IRQ configuration,
GPIO/panda reset/recovery, display/IR setters, amplifier control, SIM LPA,
`modem` daemon and the hardwared loop remain unported here. Native dependencies
include filesystem/process/socket/clock APIs, `sudo`/`cat`/`iwlist`, existing
Params and the native JSON parser's current transitive dependencies. No user's
device, host sysfs writes, real modem or public API was exercised. Full manager
startup, actual AGNOS/device behavior, full-runtime upload and performance
acceptance remain separate and pending.
