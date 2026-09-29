# M0 validation ledger

Plan: docs/rust-port/m0-plan.md. Spec: design.md. Issues: #1 (whole runtime), #2 (foundation), #3 (CI).

## Decisions

- User approved proceeding and authorized all isolated work that does not affect the original fork. Use native execution and one independent final review; no repeated stage approvals.
- Independent clone, branch codex/feat-2-rust-foundation, baseline f3a92524d87be714f6b8b5f44ecdc8319a8c53d1. Original source remote push URL is DISABLED.
- Existing Rust 1.94.0 tooling in Ubuntu/WSL is used for Linux host validation. No source device is connected or modified.
- CI bootstrap and implementation belong to separate tracking issues but are delivered together because the new workflow requires the Cargo workspace it validates.
- Current production process configuration is untouched. Its current proclogd is already enabled; older route logs without procLog are not a sufficient per-process performance baseline.
- All registered process entries are inventoried, including disabled/conditional entries. Zero production daemons have been replaced by M0.

## RED / GREEN evidence

1. Before implementation, cargo test failed E0432 for missing filters/proc_stat modules.
2. After adding parser/filter code and CLI tests, cargo test failed for absent CARGO_BIN_EXE_cpu-sample because the CLI did not exist.
3. After CLI implementation, cargo clippy --workspace --all-targets --locked -- -D warnings passed.
4. cargo test --workspace --locked passed 11 integration tests: 4 filter, 5 parser/delta, 2 actual-Linux CLI tests.
5. python3 tools/check_reference.py passed 24,000 output comparisons against the actual source Python filter classes; maximum absolute difference 4e-15.

## Pending gates

- GitHub latest-PR Rust and inherited required checks.
- Generic Linux aarch64 build.
- Independent code review and any resulting fixes.
- AGNOS sysroot/ABI and on-device execution: not run.
- Vehicle behavior and full runtime replacement: not done.
- CPU/thermal/deadline improvements and additional YOLO workload: not measured.

Do not close the overall runtime issue based on M0 host tests. Follow-up implementation must preserve the distinctions above.
