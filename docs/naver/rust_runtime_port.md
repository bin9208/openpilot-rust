# Independent Rust runtime port record

The active port belongs to `bin9208/openpilot-rust`, issue
[#1](https://github.com/bin9208/openpilot-rust/issues/1). Source provenance and the
approved complete-runtime scope are recorded in [the design](../rust-port/design.md).
This repository does not synchronize or publish changes to the original fork.

Native model execution is tracked in
[#6](https://github.com/bin9208/openpilot-rust/issues/6), with the
[artifact plan](../rust-port/model-artifact-plan.md) and
[validation ledger](../rust-port/model-artifact-validation.md).
CPU library/model comparisons are intermediate implementation evidence. The user
requested the first device handoff only after the whole runtime can start, log
and use the existing upload flow for comparison with the original implementation.

The [native QCOM ledger](../rust-port/qcom-runtime-validation.md) records the
kernel/argument/packet oracle, KGSL ABI and lifetime checks, and remaining GPU
acceptance. Backend code and host tests do not complete the full runtime gate.

[VisionIPC issue #14](https://github.com/bin9208/openpilot-rust/issues/14) adds
the Rust-owned camera client boundary over the external msgq library. The
[boundary ledger](../rust-port/visionipc-plan.md) records native-server
interoperability, reconnect, copied-frame lifetime and sanitizer evidence.
Camera/model daemon integration and GPU imports remain full-runtime work.
