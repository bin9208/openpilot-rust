# Issue #123: continuous native sensord

Tracking: [#123](https://github.com/bin9208/openpilot-rust/issues/123), full runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).

The native daemon owns the LSM6DS3 acceleration, gyro and temperature runtime with unchanged register/self-test, scaling, settling, clock-jump, cadence and shutdown policy. A thin CXX/Linux UAPI boundary retains external kernel dependencies. [Validation](../rust-port/sensord-validation.md) records the seven actual-source comparisons, real controlled ioctl boundary, native C++ ASan/UBSan, safe-core Miri, and actual isolated PID/IPC/logging/error-recovery/shutdown scenario.

Catalog availability identifies a native candidate without changing production selection. No real hardware or scheduling state was modified. Parent integration owns exact-SHA Actions/ARM validation; full runtime startup/upload and device acceptance remain pending.

Docs-Not-Needed: internal runtime replacement; no setting or user-guide behavior change.
