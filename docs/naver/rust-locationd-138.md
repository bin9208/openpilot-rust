# Native locationd / PoseKalman (#138)

Tracking: [#138](https://github.com/bin9208/openpilot-rust/issues/138), [full runtime #1](https://github.com/bin9208/openpilot-rust/issues/1). Source baseline `759e4e0a`; branch `codex/feat-138-locationd`.

`rust/crates/locationd` owns the native locationd loop, PoseKalman model expressions, transforms, Params and livePose serialization. The approved boundary retains unchanged external rednose/Eigen numerical and rewind code through owned CXX handles. Python generation and the actual source Cython oracle are development/test dependencies only.

See [numeric contract, reproduced edge cases, commands, evidence and limits](../rust-port/locationd-validation.md). Parent integration records the adopted commit, dev PR and exact-SHA Actions results. The candidate does not change production selection, thresholds or priorities. paramsd/lagd and whole-runtime startup/upload remain separate work; no device comparison is requested from this component.
