# Continuous driver-monitoring daemon validation

Issue [#38](https://github.com/bin9208/openpilot-rust/issues/38), building on the
[policy port](monitoring-validation.md) under full-runtime #1/#6.

## Source loop contract

`openpilot-dmonitoringd` runs continuously; `--frames N` counts driver-frame
publications, including invalid publications which retain the policy state.
It subscribes to `driverStateV2`, `liveCalibration`, `carState`, `selfdriveState`
and `modelV2`, polling only `driverStateV2` with the source 100 ms timeout. An
auxiliary-only update does not publish. Native endpoints use the original
service-catalog capacities. On TICI the process requests cores 0..3 and FIFO 5.

The policy runs when the complete subscription passes source `all_checks`, or
when driver-view demo mode is active and the driver message is valid. The latter
does not make the output valid: output validity still comes from `all_checks`.
Otherwise the existing policy state is published without a policy step.

Live `AlwaysOnDM` and `IsDriverViewEnabled` values are read after the actual
publication when driver frame ID modulo 40 is 1. The initial driver-view mode is
false, matching the source. Saved handedness is written after publication only
on a frame ID divisible by 6000, outside demo mode, with more than 300 filtered
wheel samples and agreement between the selected side and the filtered mean.
This persistence condition has no additional validity gate. `DriverTooDistracted`
is read during initialization and is never written by this daemon.

The Params library adds runtime path resolution and exact native boolean helpers.
`PARAMS_ROOT` overrides the root; otherwise the source TICI/PC paths apply.
An unset prefix uses the `d` Params namespace, while an explicitly empty prefix
uses the root itself. The existing explicit `Params::open` namespace guard is
retained. Boolean reads accept exactly the byte `1`; writes serialize `1` or `0`.

The policy input boundary now retains source array lengths. Empty arrays return
before coordinate validation. Only the selected driver's required coordinates
are checked, unused extra coordinates are ignored, and nonempty position
uncertainty arrays need no second coordinate because the source never reads it.
A truncated selected input returns an error and stops the process before another
publication. No alert, confidence or calibration threshold changed.

## Verification

The native harness uses actual original Python cereal/msgq publishers and
subscribers. Its oracle executes the original `dmonitoringd_thread` initialization
and loop statements, original SubMaster state and original policy classes. Inputs
are decoded through the complete cereal schema before reference arithmetic,
preserving Float32 promotion. It does not reimplement policy or loop expectations.

The final pinned host executable produced 437 packets with 16,169 decoded fields
matching exactly, including all floating-point values. Cases cover auxiliary-only
silence, invalid state retention, demo gating, post-publication toggle timing,
saved lockout/RHD, empty arrays, extra/unused/truncated coordinates, bounded exit
and four SIGINT/SIGTERM waits. Handedness was written only after the qualified
frame 6000, even though that driver message was invalid. File inode/mtime checks
also verify that no early same-value rewrite occurred and that the daemon did
not write `DriverTooDistracted`.

These native scenarios use `SIMULATION=1` to make freshness admission deterministic
from validity flags. Real-time alive/frequency semantics remain covered by the
[message-state source/native tests](messaging-validation.md); this test is not a
vehicle timing result. The source policy comparison was rerun after changing the
array boundary: all 209,144 states and complete packets still matched with zero
observed difference. Focused array, controller and boolean regressions, complete
workspace tests, strict Clippy and formatting passed.

```sh
cargo build --manifest-path rust/Cargo.toml -p openpilot-dmonitoringd --locked
python rust/tools/build_msgq_python.py --output /path/to/native-msgq
PYTHONPATH=/path/to/native-msgq:.:rust/tools python rust/tools/check_dmonitoring_daemon.py \
  --binary rust/target/debug/openpilot-dmonitoringd --output /path/to/fresh-evidence
PYTHONPATH=.:rust/tools python rust/tools/check_monitoring_reference.py \
  --binary rust/target/debug/monitoring-probe --output /path/to/fresh-policy-evidence
```

Local native evidence is retained in the issue worktree's `.omo/evidence/dmonitoring-daemon/`
index and its sibling `qa-native-final` directory, including binary/source hashes
and complete expected/actual packets. A workspace build relinked the first tested
binary without a source edit, so final QA used a separate immutable binary copy
and recorded its SHA256. CI repeats both policy and native daemon comparisons,
retaining reports and logs. Exact-head cloud/aarch64 and inherited checks remain
required before integration.

## Remaining acceptance

Production manager selection is unchanged. Whole-runtime startup, remaining
services, normal route logging, existing uploads and first user device comparison
remain open under #1/#6. No vehicle was accessed or tested, and this host evidence
does not establish CPU savings, AGNOS scheduling behavior or driving acceptance.

Docs-Not-Needed: internal runtime and host validation; existing production
settings and user-visible behavior are unchanged.
