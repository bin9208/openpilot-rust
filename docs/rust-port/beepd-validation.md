# Native GPIO alert beep daemon (#79)

`rust/crates/beepd` ports `openpilot/selfdrive/controls/beep.py` for the normal
`main()` / `beepd_thread(test=False)` path. It remains an optional executable;
production manager selection is unchanged. The original synthetic test publisher
is a test helper, not part of the selected runtime path.

Startup retains the original shell commands to export GPIO42, set its direction
and perform a 100 ms beep. Only export spawn errors are suppressed. Direction
and startup errors propagate. Every `subprocess.run` equivalent ignores nonzero
exit status and discards command stdout/stderr, matching `check=False`.
Production commands still invoke `/bin/sh`, `sudo tee`, and the original sysfs
paths. Tests intercept sudo through an isolated PATH; they never open real GPIO.

The daemon polls original-compatible `selfdriveState` with a nonblocking update
and the source's 20 Hz absolute-deadline Ratekeeper cadence. Monotonic seconds
retain CPython's total-nanoseconds conversion and integral-seconds shortcut;
sleep durations retain timeout rounding upward and the signed nanosecond range. Only updated, changed
alert values print `[BEEP] New alert: N`; duplicate and idle updates do not dispatch.
Outer message validity is not an additional source gate. All 37 current enum
values and unknown wire values retain the original mapping. Each mapped change
starts an independent daemon worker. A later alert, including none, does not
cancel or serialize an earlier pulse train.

Pulse sequences retain the exact on/off order and sleeps: startup 100 ms, engage
50 ms, ding 20 ms, dong 30 ms, beep 40 ms; disengage has two and warning three
10 ms on / 10 ms off repetitions, including the final off sleep. Every on **and**
off operation rereads `SoundVolumeAdjust`. Values at or below 5 force the output
low without skipping any pulse or sleep.

## Integer and error boundaries

The source calls the real Cython `get_int`, which delegates directly to C++
`std::stoi`; it does not call typed `Params.get` or its conversion-warning path.
Missing/empty/unreadable values yield zero. ASCII whitespace (including vertical
tab), signs, decimal prefixes, embedded NUL and signed 32-bit boundaries retain
that parser's behavior. Numeric suffixes such as `6tail`, `6.5` and `6_000` parse
as 6; `0x10` parses as 0. No conversion warning is synthesized.

Malformed or overflowing values expose the inherited exception-boundary defect
tracked in [#82](https://github.com/bin9208/openpilot-rust/issues/82): the real
Cython process aborts with SIGABRT, including when the getter runs on an alert
thread. The native daemon deliberately reports a typed `std::stoi`-category
error and exits nonzero, without a core dump. This preserves the process-fatal
outcome, not the exact signal or diagnostic text. Ordinary worker command/clock
errors stop that worker and print a diagnostic while the daemon continues.

A supervisor receives SIGINT/SIGTERM independently of the startup and runtime
worker. Process exit terminates daemon threads without joining blocked workers
or inventing an off pulse. Test captures verify bounded exit during idle polling,
a held startup command, and a held alert command. The source's already-spawned
shell/tee process can outlive its parent briefly; safe fixture commands are
explicitly released and awaited by the test harness.

## Evidence

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-beepd --all-targets --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-beepd --bins --examples --locked
# PYTHONPATH includes the original msgq binding, repository root, and rust/tools.
python rust/tools/check_beepd_reference.py --binary rust/target/debug/examples/beep_trace --binding /path/to/params_pyx.so --output /tmp/beep-reference
python rust/tools/check_beepd_integer.py --binary rust/target/debug/examples/beep_trace --binding /path/to/params_pyx.so --output /tmp/beep-integer
python rust/tools/check_beepd_daemon.py --binary rust/target/debug/openpilot-beepd --binding /path/to/params_pyx.so --output /tmp/beep-daemon
python rust/tools/check_beepd_clock.py --binary rust/target/debug/examples/beep_trace --output /tmp/beep-clock
python rust/tools/check_beepd_sleep.py --binary rust/target/debug/examples/beep_trace --output /tmp/beep-sleep
```

The unchanged source class and Ratekeeper bodies run with real Cython Params;
only external hardware/clock paths are replaced for policy checks. Separate
continuous process checks use original cereal/msgq messages, real worker threads,
actual shell subprocesses and disposable fake-sudo GPIO records. They cover
all mapping families, unknown values, duplicates, idle updates, nonzero statuses,
concurrent held commands, a volume change while workers overlap, continued pulse
trains after none, 20 Hz dispatch, fatal startup/worker conversion and shutdown.

Exact source/binary hashes, commands, failed attempts, observations and artifacts
are indexed in `.omo/evidence/beepd/evidence.json` in the issue-79 worktree. Generic
GNU aarch64/QEMU evidence is host emulation only. External runtime dependencies
remain original msgq through CXX, OS scheduling/clocks, `/bin/sh`, sudo/tee and
GPIO42 sysfs. No real GPIO, sound hardware, vehicle, C3X, NAS or public-network
operation is used. AGNOS/device acceptance, normal full-runtime integration and
CPU savings are not established. Parent owns CI and integration.

Docs-Not-Needed: isolated optional runtime port preserving the existing setting,
its behavior, and production process selection.
