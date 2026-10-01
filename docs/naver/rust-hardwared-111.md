# Native continuous hardwared (#111)

The issue branch implements `rust/crates/hardwared` and the
`openpilot-hardwared` binary from the unchanged
`openpilot/system/hardware/{hardwared,fan_controller,power_monitoring}.py`
bodies. It reuses hardware information (#98), hardware control (#105), native
Params, messaging, logging, stats and the runtime-core temperature filters.
The captured-command helper includes #110's bounded private socket directory.

The production entrypoint starts the bounded one-slot, ten-second network
worker, Panda-polled hardware worker, and (on TICI) nonblocking Linux input-event
worker. Worker failure stops and joins the other workers. SIGINT/SIGTERM and the
optional `--cycles N` stop cleanly; the Params queue drains before exit.
`--root PATH` redirects hardware/proc/input/Params paths for owned fixtures.
The root's TICI/AGNOS marker files select board behavior, so a temporary root is
not a command sandbox. Validation uses a PC root and an empty executable PATH;
no board command, sysfs/GPIO/I2C action, vehicle connection or reboot is run.

The implementation retains 2 Hz thermal updates plus ignition edges, ten-Hz
Panda frame gating, the five-second disconnect timeout, one-second onroad cycle,
source thermal hysteresis and offroad cooling override, device-specific fan PID,
CarParams byte cache/Tesla exceptions, startup and onroad conditions, alert
change suppression, engagement state, voltage filtering and energy integration,
shutdown gates, status packet/uptime intervals, and existing logging paths.
The Python binding's ignored filesystem write/remove return codes and empty
read behavior are applied at the daemon's Params boundary. No settings or
production process selection change.

## Host evidence and limits

See [hardwared validation](../rust-port/hardwared-validation.md) for reproducible
commands. Focused comparisons execute original source bodies with controlled
clock/hardware/Params boundaries; Python is only a test oracle. A separate
scenario drives the actual Rust entrypoint using original Python msgq/cereal
peers and owned temporary proc/Params files, and decodes actual Cap'n Proto
publications. Native tests cover cache refresh/removal/malformed input, Params
errors, CPU counters, power-read failure ordering, shutdown boundaries, touch
ABI decoding and wire integer bounds/status-packet shape.

The native dependencies remain Linux/AGNOS procfs/sysfs/input/I2C, original
hardware commands (including sudo/pgrep/chrt/taskset), the captured-child helper,
CXX/native msgq transport and libzmq. The modem daemon/LPA, manager adoption and
other unported runtime components remain separate. Host fixtures do not validate
physical thermal sensors, fans, power rails, touch devices or AGNOS execution.
Generic ARM/cloud checks are the parent integration gate; no on-device,
loaded-driving, performance or complete-runtime claim is made. Issue #1 stays
open and the user's first device comparison still requires the entire runtime,
normal startup and existing upload path.

Docs-Not-Needed: isolated native daemon candidate; no setting or selected user behavior changes.
