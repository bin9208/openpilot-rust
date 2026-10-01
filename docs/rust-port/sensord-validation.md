# Native sensord and LSM6DS3 drivers

Issue [#123](https://github.com/bin9208/openpilot-rust/issues/123), full-runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1). Source: unchanged `openpilot/system/sensord`, called SMBus/GPIO functions in `openpilot/common/{i2c,gpio}.py`, and the relevant realtime/Ratekeeper helpers, based on `19f4eaf1`. Source licensing/history remain intact.

`rust/crates/sensord` provides the continuous `openpilot-sensord` daemon, Rust register/self-test/conversion/settling/IRQ/polling policy, full cereal serialization and native logging. The thin C++ boundary owns Linux ioctl, GPIO event descriptor, poll/read and scheduling calls. It contains no sensor policy. Existing `i2c-linux-sys` block reading does not match the source's zero returned-length fallback and clamping, so this boundary uses the actual zero-initialized Linux UAPI structures. CXX is the only locally allowed unsafe declaration scope; Rust policy remains safe and can run without native features under Miri.

The runtime preserves core 1/FIFO priority 1 on TICI, all reset/init/shutdown register transactions, 104 Hz IMU ODR, 2 Hz temperature Ratekeeper, chip-specific temperature/scaling and the existing axis mapping. Acceleration self-test requires `LSM_SELF_TEST=1`; gyro self-test runs whenever that variable exists, including `0`. Both positive and negative self-tests retain their read counts, waits and limits. Each sensor starts its own strict >0.5-second settling window after its first successful read. Temperature polling skips Ratekeeper while settling, as the source does.

IRQ handling requests both edges on gpiochip0 line 84, prefers IRQ 336's affinity path then 335, and uses only the first event from each read batch. It retains realtime-to-monotonic offset conversion and skips batches when the offset changes by more than 10 ms; equality is accepted. Data-not-ready is silent, per-sensor read/send failures are logged and the next sensor/sample proceeds, and malformed GPIO reads terminate that worker. Failed temperature initialization does not start its polling worker; IMU initialization failures do not remove sensors from the IRQ worker. There is no invented device-reopen loop. SIGINT sets the stop condition, joins workers and disables interrupts/ODR before owned descriptors are released.

Messages preserve source enum, Int64 sample timestamp, Float32 values, valid flag, float-derived `logMonoTime` and the unchanged deprecated schema defaults. Default runtime uses normal msgq/logging paths. `--root PATH` maps hardware resources to an owned fixture root and requires an isolated `rust-probe-*` IPC namespace. `--launcher` supplies the existing native process helper for the source-compatible IRQ permission fallback. Manager catalog availability is updated; production process selection is unchanged.

## Focused gates

```sh
RUSTUP_TOOLCHAIN=1.94.0 cargo build --manifest-path rust/Cargo.toml -p openpilot-sensord --bins --examples --locked -j2
RUSTUP_TOOLCHAIN=1.94.0 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-sensord -p openpilot-manager-catalog --all-targets --locked -j2 -- -D warnings
python rust/tools/check_sensord.py --trace rust/target/debug/examples/sensord_trace --evidence /tmp/sensord-evidence
python rust/tools/check_sensord_kernel.py --binary rust/target/debug/examples/sensord_linux --evidence /tmp/sensord-evidence
python rust/tools/check_sensord_daemon.py --target rust/target --evidence /tmp/sensord-evidence
```

Use the existing Python source-oracle environment with pycapnp, pyzmq and zstandard, plus Clang for the controlled native boundary and sanitizer peer. Python is only an oracle/fixture dependency. The source adapter imports the actual drivers and executes unchanged AST function bodies for the IRQ/polling loops, message creation and Ratekeeper, substituting only bus, clock, poll and publisher boundaries.

Seven source/native comparisons cover both chip types, normal initialization/conversion/shutdown, both self-tests, gyro-only environment behavior, self-test and chip-ID failures, settling equality, IRQ timeout/flags/batched events, clock-jump equality/rejection, data-not-ready, read-error recovery, short GPIO data, and temperature cadence/error handling. Result rows, complete register transactions, waits, logs and decoded cereal packets match exactly, including legacy defaults and Float32 rounding.

The real ioctl comparison runs unchanged Python fcntl and native CXX calls against explicitly selected owned regular files. It checks normal/forced slave selection, byte operations, zero/oversized/short block-return lengths, 32-byte and zero-byte transfers, invalid requested length, EINTR, GPIO structure/label/flags, returned event bytes and descriptor closure. Scheduling calls are recorded but intercepted: the host's actual scheduler and affinity remain unchanged.

The identical production C++ kernel implementation also runs under AddressSanitizer and UndefinedBehaviorSanitizer with the controlled ioctl fixture. This is a focused native-boundary sanitizer check, not a claim that the whole runtime was instrumented. The pure Rust conversion/settling/cereal test passes default Miri and strict provenance/symbolic-alignment Miri with native features disabled; Miri cannot execute the C++/kernel boundary.

The actual native daemon test verifies `/proc/PID/exe`, real isolated msgq publications and logging, settling, sample timestamps/schema, approximately 0.5-second temperature intervals, injected IMU read failures and recovery without reopening, SIGINT exit zero, interrupt/ODR shutdown writes, descriptor closure and the owned IRQ-affinity file. The observed fixture run produced 81 acceleration, 81 gyro and 3 temperature messages; first publication was about 0.70 seconds after startup. These are fixture observations, not physical-device timing or performance results.

Evidence is under `.omo/evidence/sensord-123/` in the parent workspace: policy pairs/results, kernel pairs/results, sanitizer and Miri logs, actual packet/log captures, daemon results, catalog metadata, build/lint results and source/binary/dependency hashes. A close-observation fixture bug was corrected by checking the descriptor before opening the trace file could reuse it; the full affected kernel and daemon scenarios then passed.

## Dependencies and remaining acceptance

Native dependencies remain explicit: Linux SMBus/GPIO/poll/scheduling, C++17/CXX, original msgq transport, native logging/ZeroMQ, and the existing hardware-control IRQ write/permission fallback. No registry version is upgraded. No real I2C/GPIO, scheduling adjustment, physical IRQ placement, sensor or vehicle was exercised. The hardware tests in the original sensord suite were read but not run against a device.

Physical initialization/self-test, interrupt timing, loaded-device performance, ARM/AGNOS ABI and vehicle acceptance remain unvalidated. Parent integration owns broad exact-SHA Actions and generic ARM checks. Complete normal startup/log upload remains a whole-runtime gate; this component is not a device-test handoff.
