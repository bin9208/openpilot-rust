# Hardware-control port record (#105)

- Tracking: [#105](https://github.com/bin9208/openpilot-rust/issues/105), hardware
  integration [#103](https://github.com/bin9208/openpilot-rust/issues/103), full
  runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).
- Implementation and repeatable validation commands:
  [hardware-control validation](../rust-port/hardware-control-validation.md).
- Host scope: 35 unchanged-source policy comparisons, temporary-filesystem
  native setter/Panda-pin scenario, real child stdout/stderr/status scenario,
  package test, formatting, clippy and Python lint.
- Native production adapters exist; hardwared/manager adoption, Actions ARM
  verification and the complete normal-startup/log-upload candidate remain
  separate work. No production selection, board/device access, vehicle acceptance
  or CPU savings are claimed.
- Exact integration SHA and Actions URLs belong to the #105/#103 PR record after
  the committed component is integrated. Local evidence stays private.
