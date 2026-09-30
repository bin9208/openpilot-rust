# Rust checkout identity policy (#92)

- Issue: [#92](https://github.com/bin9208/openpilot-rust/issues/92), under #1/#6.
- Source: `openpilot/system/manager/update_status.py`; initial issue baseline
  `355f68ca04e3b12f0c59e6a7ee20aac77f0a1087`.
- Implementation: `rust/crates/checkout-status`, using the existing native
  process helper with an explicit captured-exec mode and owned handshake.
- The prerequisite integration revision `e73b6909` and the parent's held-child
  test correction `8d56c256` were integrated without rewriting history.
- Contract, commands and limits:
  [checkout-status-validation.md](../rust-port/checkout-status-validation.md).
- The issue worktree's `.omo/evidence/checkout-status/resume-verification.json`
  records the final fast checks and verifies the retained pre-pause artifacts.
  Original-source comparisons and child/descriptor cleanup remain in the
  referenced scenario directories; full CI validation is owned by the parent.
- Host and generic ARM evidence is separate from Actions, AGNOS/device
  acceptance and complete manager integration. No production startup, guide,
  setting, GPIO, vehicle or NAS change is included. Parent owns CI and PRs.

Docs-Not-Needed: internal optional checkout policy with existing public runtime
behavior preserved.
