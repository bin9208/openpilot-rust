# Model USB Params refresh (#310)

Tracking: [#310](https://github.com/bin9208/openpilot-rust/issues/310).

The inherited driving-model loop refreshes USB presence/compiled state every
20 iterations. Each refresh unconditionally persists `UsbGpuPresent` and
`UsbGpuCompiled`, plus `UsbGpuHardwareSeen` when a device is present. Params
persistence includes a temporary-file sync, lock, rename and directory sync.
In the inspected drive, 79 of 114 model publication gaps above 75 ms followed
refresh. This correlation does not establish the cause of all gaps or a CPU
benefit from removing redundant writes.

The scoped correction compares each desired status with its current persisted
bytes and writes only differences. It must retain the 20-iteration cadence,
repair external edits/deletions/noncanonical values, propagate read/write errors
and retry from the current persisted state. Absence must retain hardware-seen
history; refresh must not change activity, loading or startup-failure flags.
Startup model selection, GPU failure handling, scheduling, pairing and validity
thresholds remain outside this correction.

RED: the unchanged absent-device and unchanged present-device tests failed with
`Params(Io(IsADirectory))` after replacing the Params lock file with a directory.
This shows that the old refresh attempted writes even when every status already
matched. Three other behavior tests passed before the correction.

GREEN: `cargo test --manifest-path rust/Cargo.toml --locked -p
openpilot-driving-modeld` passed all 20 top-level integration tests, including
five USB cases. `cargo clippy --manifest-path rust/Cargo.toml --locked -p
openpilot-driving-modeld --all-targets -- -D warnings`, scoped Rustfmt and
`git diff --check` passed. Each build checked the disk reserve with 1 GiB
estimated growth; free space remained above 381 GiB. Builds ran sequentially
in the coordinated cache with incremental compilation disabled and two jobs.

The synthetic host Params fixtures cover unchanged values without a writable
lock, unchanged file inodes, hardware insertion/removal, history retention,
external edits/deletions/noncanonical bytes, read/write failure followed by
retry, and activity/loading/startup-failure flag preservation. The daemon's
existing 20-iteration call site is untouched. The startup and published/failure
paths are untouched. No daemon/GPU/device was started by these fixtures.

The compiled-path discovery still runs every refresh and its cost has not been
measured here. Read/compare/write is not a transaction with other Params writers;
subsequent refreshes reread the persisted state and can repair later edits.
These fixtures cannot establish target timing, normal device startup, CPU
savings, correction of every publication gap, or driving acceptance.

Docs-Not-Needed: internal runtime persistence optimization; no user setting or
user-visible selection policy changes.
