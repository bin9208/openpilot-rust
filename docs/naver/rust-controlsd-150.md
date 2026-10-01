# Native controlsd candidate (#150)

Tracking: [#150](https://github.com/bin9208/openpilot-rust/issues/150), full runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).

The component now has a continuous Rust control loop and consumed vehicle-control policies, with original inputs, outputs, Params actions, control thresholds and scheduling policy. Its candidate catalog entry does not replace the production process descriptor.

[Scope, numerical contract, commands and evidence](../rust-port/controlsd-validation.md) record unchanged-source host/ARM comparisons, native IPC startup/restart/shutdown, memory checks and external numerical dependencies. PSA source startup gaps are tracked independently in [#156](https://github.com/bin9208/openpilot-rust/issues/156); no source policy fix is included here.

Exact-SHA Actions links and integration acceptance belong to the checked PR into `dev`. The full-runtime normal-startup/log-upload candidate and the user's first device comparison remain open. No vehicle, NAS, real CAN or production selection was touched. No performance or drive acceptance is inferred from these tests.

The integration adds a required control-runtime CI job for all policy identities,
neural assets, full-loop and startup-rejection comparisons, native IPC lifecycle,
package tests, ASan and Miri. The full workspace tests receive the same pinned
numerical artifact before execution; the existing host and ARM builds remain
required. Fresh exact-SHA PR and post-merge results are still pending.

A clean checkout needs generated DBCs to construct the unchanged source
interfaces. `rust/tools/stage_control_dbcs.py` calls the original generator in
its temporary-copy mode and stages only absent outputs, preserving every
checked-in DBC. Directly invoking the generator's destructive refresh mode is
not part of CI. No DBC or source vehicle policy is changed by this integration.

Independent review approved component revision
`02f7eba01b64ed993d59ca5e4df04193bb95ee94` after reproducing and repairing the
PSA/FingerPrints rejection gaps. The exact component receipt and both review
reports are retained in the private `.omo/evidence/controlsd-150/` directory.
