# Rust registration prerequisite

Issue: [#85](https://github.com/bin9208/openpilot-rust/issues/85), under full runtime
[#1](https://github.com/bin9208/openpilot-rust/issues/1). Branch:
`codex/feat-85-registration`. Base: `ce1f59107937a22ff14adfc7b306bca037e9a1b9`.

The native startup policy, signed loopback source comparisons, original Params
and collector evidence are described in
[registration-validation.md](../rust-port/registration-validation.md). The local
ledger is `.omo/evidence/registration/evidence.json` in the issue worktree.
The registration-only cookie correction is tracked separately in
[#88](https://github.com/bin9208/openpilot-rust/issues/88).

Board/modem discovery, the actual spinner, manager integration and complete normal
startup remain pending. No production process selection, device access or public
registration request occurs. Parent integration owns CI and exact-SHA results;
local evidence does not close the issues or authorize a device-test handoff.

Docs-Not-Needed: internal startup prerequisite with existing settings behavior
preserved; no user guide or Wiki setting change.
