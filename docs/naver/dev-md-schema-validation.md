# dev-md additive schema foundation (#338)

Source: `43203371004e035bdb70a00a8dad29a4b657c6c3..e8e152dbcfd9574a1791ed12026d1880e87689b3` (`carrot-source/carrot-wip`). Native comparison base: `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`. Scope: schema definitions and wire regression only; no producer/consumer behavior, model selection, build tool or device changes.

## Compatibility decision

Native `OnroadEvent.EventName.jetlinkLost @126` predates this import and must keep its wire value. Upstream uses the same ordinal for `driverMonitorFallback`. Keep native `jetlinkLost @126`, retain source `systemReady @127`, `impactDetected @128`, `impactDashcamReboot @129`, and deliberately allocate native `driverMonitorFallback @130`. The inline schema comment and literal wire regression record this exception. Raw upstream enum numeric values are therefore **not** interchangeable: future source-log/replay adapters must identify source provenance and map by semantic event name, never directly copy the numeric 126. No existing native field or enum value is renamed/reused.

Other final-source fields are additive and preserve upstream ordinals. Existing native `JetlinkFrameStatus`, `DrivingModelData.jetlink @8`, `ModelDataV2.jetlink @27` and `CarrotMan` diagnostics @33..42 remain intact. No other ordinal collision was found in the inspected source schema delta.

## Imported contract

| Schema | Additions | Old-message behavior |
| --- | --- | --- |
| `car.capnp` CarState | SteeringTouch @94 (available/valid/touched/sampleMonoTime/rawStatus/rawTouch1/rawTouch2), OEM/ONNX split blindspots @95..99, stalk counters/lever/lamp @100..103 | unavailable/invalid/false and zero counters; split sources untrusted |
| `car.capnp` RadarData | displayPoints @5, hasDisplayData @6 | empty display points; unsupported display data |
| `car.capnp` CarControl.HUDControl | driverMonitoringAlert @22 | none (0) |
| `car.capnp` AudibleAlert | systemReady @37 | existing values unchanged |
| `custom.capnp` CarrotNaviState.Lane | roadCategoryValid @13 | false; category zero alone does not establish source evidence |
| `log.capnp` PandaState | wakeOnCan @40 | false, separate from ignition |
| `log.capnp` ModelDataV2.MetaData | laneChangeBlinkerHold @20, leverTurn @21 | no lane direction/false |
| `log.capnp` LongitudinalPlan | signalDetectionSuspended @64, signalObservationColor @65, signalAssistAction @66 | false/unknown/no action |
| `log.capnp` ProcLog.Process | memPssMonoTime @20 | unavailable (0) |
| `log.capnp` DriverMonitoringState | cameraUnavailable @15, dm2Experimental @16, dm2StrictTimeRemaining @17, dm2WheelTimeoutFactor @18, dm2ForwardAttentionScore @19, dm2ForwardRecovery @20, dm2InteractionCredit @21, dm2VisionTimeoutFactor @22, dm2InteractionGraceRemaining @23, dm2Disabled @24 | false/zero except both timeout factors explicitly default 1 |
| `log.capnp` Event union | customReservedRawData1 @154 | old union discriminants unchanged |

## Generation and verification

`rust/crates/cereal/build.rs` stages the actual repository `openpilot/cereal/{log,custom,deprecated}.capnp` and `opendbc_repo/opendbc/car/car.capnp` into OUT_DIR and runs pinned capnpc; generated bindings are not hand-edited or checked in. Existing generic annotation normalization remains untouched.

Prepared `rust/crates/cereal/tests/upstream_wire.rs` before schema mutation. Tests use real Cap'n Proto serialization/readback, a literal null-root old-message fixture and a literal previously native EventName 126 fixture. They exercise default timeout factors, explicit zero overriding the nonzero default, independent blindspot/touch/stalk fields, radar display/control separation, freshness/category evidence, signal defaults and wake vs ignition, and the deliberate event collision mapping.

Validation (2026-10-15, coordinated Cargo slot):

* RED: `cargo test --manifest-path rust/Cargo.toml -p openpilot-cereal --test upstream_wire` exited 101 before schema edits, reporting 85 missing new generated accessor/variant errors (E0531/E0599). This demonstrated the absent schema contract; it was an expected compilation failure, not a runtime assertion failure.
* GREEN: `cargo test --manifest-path rust/Cargo.toml -p openpilot-cereal` exited 0: all 10 integration tests passed, plus crate/doc-test targets (zero tests). Repeated after final test-name cleanup with the same result.
* `cargo clippy --manifest-path rust/Cargo.toml -p openpilot-cereal --all-targets -- -D warnings` exited 0.
* Scoped `rustfmt --edition 2021 --check rust/crates/cereal/tests/upstream_wire.rs` and `git diff --check` passed.

Cargo used `CARGO_TARGET_DIR=/home/fma1/rust/cache/cargo-target`, incremental disabled, dev/test debug=0, two jobs, native-sdk bin appended to PATH. Disk was checked before every build: 366 GiB available. No workspace-wide test or device validation was run by this schema subtask; parent integration gates remain outstanding.

## Remaining runtime work

These schemas only make fields representable. Card/vehicle interfaces must populate steering touch and split BSD/lever signals; control/planning/UI must consume them. Radar producers and display/replay consumers must honor separate display capability/points. Pandad must separately update firmware health v17 decoding and publish wake evidence. DM2/fallback monitoring, planner signal helpers, model metadata/desire producers, navigation road-category presence, proclog PSS scan timestamps and impact event/state producers require separate ports. `customReservedRawData1` signal payload producers/consumers must agree on their actual data encoding. No end-to-end parity, runtime optimization or device acceptance is established here.

## Bounded downstream sound compatibility repair

The new `AudibleAlert.SystemReady` exposed an exhaustive match failure in `beepd/src/lib.rs` (E0004). Source `openpilot/selfdrive/controls/beep.py` has no branch for this alert: the Panda beeper must remain silent. Added the explicit ignored variant and a regression. Source `openpilot/selfdrive/ui/soundd.py` maps `systemReady` to existing `prompt.wav`, one play, `MAX_VOLUME`; the asset already exists locally. Added ordinal 37 to the soundd asset table with one loop and the existing full asset-gain branch. A real asset-loading regression compares the resulting samples to Prompt while engage volume is reduced, catching accidental engage-volume scaling or missing mapping. This does not add system-ready event production or readiness state logic.

RED: beepd package test compilation failed E0004 for missing SystemReady; soundd playback regression then failed at the missing SystemReady asset mapping (after correcting the test's Logger constructor to the actual Factory API). GREEN: `cargo test --manifest-path rust/Cargo.toml -p openpilot-beepd -p openpilot-soundd` passed all six integration tests and crate/doc targets. Disk guard remained 366 GiB available; same coordinated target/jobs/debug configuration as above. Scoped diff check passed. Static search found beepd as the only direct exhaustive AudibleAlert match; soundd uses a numeric asset table. Workspace compilation remains the parent integration gate for other schema consumers.
