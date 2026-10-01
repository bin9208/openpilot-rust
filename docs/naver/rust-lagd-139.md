# Native lag estimator (#139)

Issue [#139](https://github.com/bin9208/openpilot-rust/issues/139), full-runtime
[#1](https://github.com/bin9208/openpilot-rust/issues/1). Source baseline `82c3dba8`.
Implementation and validation are in progress; this is not device acceptance.

## Numerical contract fixed before implementation

`rust/crates/lagd/tests/tolerances.json` fixes the original-source comparison
budgets before implementation: absolute + relative 1e-12 for pose/smoothing,
1e-9 for normalized correlation, 1e-8 for lag/block statistics, and 1e-7 for
serialized Float32 fields. Finite/nonfinite classification and all discrete
policy outcomes must match exactly. A differing candidate run/index, acceptance,
validity/status, recovery/cadence, counter or Params action is a failure regardless
of numerical tolerance. Production thresholds are unchanged. The first numerical
backend candidate is the existing pinned RustFFT 6.4.1; source comparisons decide
whether it is suitable.

No real devices, private routes, NAS or C3X are accessed. Production selection,
locationd/PoseKalman #138, paramsd, UI, CI workflows and user guides are excluded.
