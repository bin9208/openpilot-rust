# Rust boot capture and shared identifier errors

- Runtime component: [#97](https://github.com/bin9208/openpilot-rust/issues/97).
- Discovered shared counter I/O mismatch: [#99](https://github.com/bin9208/openpilot-rust/issues/99).
- Design, source boundaries, commands, dependencies and remaining gates:
  [bootlog validation](../rust-port/bootlog-validation.md).
- Private evidence: issue worktree `.omo/evidence/bootlog/evidence.json` records
  exact commands, immutable binary hashes, original/native comparisons and failures.
- Branch integration, exact-SHA Actions and full normal startup/device comparison
  remain separate parent gates. No issue closure, vehicle access or CPU-saving claim.
