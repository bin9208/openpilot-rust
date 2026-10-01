# Native controlsd candidate (#150)

Tracking: [#150](https://github.com/bin9208/openpilot-rust/issues/150), full runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).

The component now has a continuous Rust control loop and consumed vehicle-control policies, with original inputs, outputs, Params actions, control thresholds and scheduling policy. Its candidate catalog entry does not replace the production process descriptor.

[Scope, numerical contract, commands and evidence](../rust-port/controlsd-validation.md) record unchanged-source host/ARM comparisons, native IPC startup/restart/shutdown, memory checks and external numerical dependencies. PSA source startup gaps are tracked independently in [#156](https://github.com/bin9208/openpilot-rust/issues/156); no source policy fix is included here.

Exact-SHA Actions links and integration acceptance belong to the checked PR into `dev`. The full-runtime normal-startup/log-upload candidate and the user's first device comparison remain open. No vehicle, NAS, real CAN or production selection was touched. No performance or drive acceptance is inferred from these tests.
