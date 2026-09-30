# Native hardware controls

Issue [#105](https://github.com/bin9208/openpilot-rust/issues/105) implements the
remaining board-control policy under [#103](https://github.com/bin9208/openpilot-rust/issues/103)
and the [full-runtime design](design.md). The source is the existing
`openpilot/system/hardware/{base,pc,tici}/hardware.py`, `common/gpio.py` and
`common/utils.py` at the inherited project revision. Original licensing remains
in place. Read-only hardware information (#98), modem/LPA, the hardwared loop,
and manager adoption remain separate integration work. GPIO read/export and
gpiochip event APIs are outside the helpers required by these control methods.

`openpilot-hardware-control` exposes `HardwareControl::pc()` and
`HardwareControl::board(model)`, with the cached device model supplied by the
hardware-info owner. Its public methods preserve:

- Display power and percentage brightness writes, with their swallowed failures;
  IR ordering and propagated failures, including the tizi exclusion.
- Lazy amplifier selection: mici skips the Params lookup; HardwareC3xLite skips
  the amplifier. Other boards reuse `openpilot-amplifier`. A false amplifier
  result does not stop initialization; an error does.
- Power-save transitions, original CPU online/governor/frequency values, GPU
  IRQ core 7 and camera IRQ core 6. Initialization keeps fan GPIO, IRQ,
  GPU/devfreq/VIDC writes and SPI scheduling command order unchanged.
- GPIO error messages and continuation, complete IRQ discovery before writes,
  cached IRQ actions, and PermissionError-only privileged-write fallback.
- Internal Panda reset/recovery pulse ordering, the 120-second encoder readiness
  boundary, reboot exit checking, ignored shutdown exit status, and reset marker
  then sync then reboot during uninstall. PC retains base no-ops and messages.

`LinuxPlatform::new(Path::new("/"), ProcessCommands { launcher })` is the native
adapter. `launcher` must point to the built `openpilot-process-child` executable.
It uses native filesystem, time and sync operations, Rust Params and the Rust
amplifier adapter. Commands retain their original argv/shell text. The shared
launcher adds `spawn_stdout()` to capture stdout while inheriting stderr, as
Python `check_output` does. No Python interpreter is required in production.

The alternate filesystem root redirects file and I2C paths; it is not a command
sandbox. Tests inject a command fixture when using temporary board paths. They
never initialize real GPIO/sysfs/I2C, invoke sudo, sync the host, or reboot.
The real subprocess check only runs harmless shell/stdio/exit fixtures.

## Validation

Focused host evidence covers 35 unchanged-source comparisons for tici, tizi,
mici, PC, C3X Lite, normal transitions, lazy exclusion, amplifier false/error,
CPU/IRQ/IR failures, GPIO continuation, permission fallbacks, display failures,
readiness boundary and command failures. Each case compares all ordered effects,
return/error outcomes and resulting fixture files. A native temporary-filesystem
test observes setter outputs and Panda pins. The real helper test captures
131,072 stdout bytes, inherits stderr, and observes normal/nonzero status and
missing-executable errors. Package tests, formatting, clippy and Python lint
pass locally. Generic ARM and broader checks remain for Actions.

Reproduce from the repository root (choose fresh output directories):

```sh
cargo build --manifest-path rust/Cargo.toml -p openpilot-hardware-control --examples --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-process-supervision --bin openpilot-process-child --locked
cargo test --manifest-path rust/Cargo.toml -p openpilot-hardware-control --locked
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-hardware-control -p openpilot-process-supervision --all-targets --locked -- -D warnings
cargo fmt --manifest-path rust/Cargo.toml -p openpilot-hardware-control -p openpilot-process-supervision -- --check
python3 rust/tools/check_hardware_control.py rust/target/debug/examples/hardware_control_trace /tmp/hardware-control-source-output
rust/target/debug/examples/hardware_control_command rust/target/debug/openpilot-process-child
```

The command fixture emits JSON with status 7, stdout_bytes 131072, stdout_zero
true, call_status 3, shell_status 5 and missing_is_io true; stderr is exactly
`inherited-stderr`. The source checker accepts repeated `--runner` arguments
for generic architecture runners. Cross-building alone does not establish board
behavior. Production selection, complete startup/log upload, device acceptance
and CPU savings are not claimed by this component.
