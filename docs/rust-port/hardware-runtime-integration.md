# Native hardware and boot support

[Issue #103](https://github.com/bin9208/openpilot-rust/issues/103) integrates
boot capture #97/#99, hardware information #98, amplifier #101/#102,
board controls #105 and the registration hardware adapter #107 after startup
prerequisites #94. Production daemon selection remains unchanged.

The required `rust hardware and boot runtime` job compares real compressed boot
artifacts and upload selection with the unchanged C++ source, source/native Params
snapshots and shared counters, hardware observations, board-control policy and
controlled I2C transactions. It checks actual child stdio and file ownership with
ASan. Existing workspace checks and generic ARM builds include the new packages;
the aggregate gate rejects absent, skipped, failed or cancelled hardware results.

Completed component results are reused: 136 hardware-information scenarios,
2,034 amplifier policy cases, 27 controlled I2C cases, 35 board-control scenarios
and 12 registration adapter cases. Parent corrected snapshot character-device
copying, inherited descriptors and private permissions (#112), then passed four
Rust tests and ten actual Python/native snapshot scenarios. The shared helper
uses an owned `0700` control directory with a bounded socket path (#110).

Local metadata resolution, diff checks and seven CI policy tests pass. Broad
combined source/runtime, workspace and generic ARM results are recorded in the
PR and separate post-merge Actions; they remain pending at initial publication.
The user's 2026-10-01 Actions-first request avoids repeating successful local
matrices merely for another artifact copy.

External kernel/sysfs/ioctl, serial and codec interfaces remain explicit.
Manager orchestration #106, hardwared #111, modem #113, SIM LPA and the remaining
runtime still require integration. No actual device, board mutation, deployment,
CPU saving or complete-runtime acceptance is claimed by this increment.

Docs-Not-Needed: native internal runtime support and CI; no user settings change.
