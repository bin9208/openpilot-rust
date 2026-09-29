# M0 validation ledger

Plan: [m0-plan.md](m0-plan.md). Spec: [design.md](design.md).
Issues: [#1](https://github.com/bin9208/openpilot-rust/issues/1) (whole runtime), [#2](https://github.com/bin9208/openpilot-rust/issues/2) (foundation), [#3](https://github.com/bin9208/openpilot-rust/issues/3) (CI).

## Decisions

- User approved proceeding and authorized isolated work that does not affect the original fork. One independent implementation review was completed; the Ubuntu resume reuses that exact-head review.
- Independent repository baseline: `f3a92524d87be714f6b8b5f44ecdc8319a8c53d1`. The Windows source remote push URL was disabled. The Ubuntu checkout has only the independent repository as origin.
- Rust 1.94.0 Linux/WSL validation was obtained before handoff. The new Ubuntu checkout did not redundantly rerun completed tests. No device was connected or modified.
- CI and implementation have separate tracking issues but were delivered together because the workflow requires the Cargo workspace.
- Production process configuration is unchanged. The existing proclogd is already enabled; historical logs without procLog are not a per-process performance baseline.
- All 63 registered process entries, including disabled/conditional entries, are inventoried. Zero production daemons have been replaced by M0.

## RED / GREEN evidence

1. Before implementation, cargo test failed E0432 for missing filters/proc_stat modules.
2. After parser/filter code and CLI tests, cargo test failed for absent CARGO_BIN_EXE_cpu-sample because the CLI did not exist.
3. cargo clippy --workspace --all-targets --locked -- -D warnings and fmt passed.
4. Initial cargo test passed 11 integration tests. The broken-output-pipe regression first reproduced a panic, then passed after the fix: final total 12 tests (4 filter, 5 parser/delta, 3 Linux CLI).
5. python3 tools/check_reference.py passed 24,000 comparisons against actual source Python filter classes; maximum absolute difference 4e-15.
6. The handoff records 6 CI-policy tests, the user-docs validator and an independent review PASS for `be9d4c061112dd77151fb7aa6d9d76b0499967ce`. This was a local review, not a GitHub approval. Ubuntu diff inspection confirms the BrokenPipe handling/regression and repository guards remain present.

## M0 integration and exact revisions

[PR #4](https://github.com/bin9208/openpilot-rust/pull/4) merged into dev on 2026-09-29.

- Tested PR head: `be9d4c061112dd77151fb7aa6d9d76b0499967ce`.
- Merge commit: `643973beed330c96790b35808a93933b25f25288`.
- Both implementation commits and the feature branch are preserved.

All five required checks succeeded on the tested PR head:

| Check | Exact-head run |
| --- | --- |
| fast checks | [36582395331](https://github.com/bin9208/openpilot-rust/actions/runs/36582395331) |
| integration gate, including release build | [36582395732](https://github.com/bin9208/openpilot-rust/actions/runs/36582395732) |
| check mapped user docs | [36582395054](https://github.com/bin9208/openpilot-rust/actions/runs/36582395054) |
| rust checks and generic aarch64 build | [36582394918](https://github.com/bin9208/openpilot-rust/actions/runs/36582394918) |

Separate push runs for the merge commit: [fast](https://github.com/bin9208/openpilot-rust/actions/runs/36584640177), [integration](https://github.com/bin9208/openpilot-rust/actions/runs/36584640642), [docs](https://github.com/bin9208/openpilot-rust/actions/runs/36584640192), [Rust host/aarch64](https://github.com/bin9208/openpilot-rust/actions/runs/36584640050). Their conclusions are recorded in #2/#3 after completion; PR success does not substitute for these runs.

## Repository isolation readback (2026-09-29)

- dev protection was read back in GitHub: PR required, all five named checks required, strict up-to-date checks, conversation resolution, and no administrator bypass. Approval count is zero; local independent review is recorded separately. Force pushes and branch deletions are disallowed.
- GitHub registered Sync Upstream, Carrot Routes image and cp-set-wiki as active when the workflows entered dev. Their source-repository guards already prevented execution in this repository. All three were then disabled at repository level, and REST readback confirmed `disabled_manually`. Required CI workflows remain active.
- No deployment secrets were added, no source repository was modified, and no deployment was performed.

## Remaining acceptance and next issues

- [#5: IPC, Params and procLog](https://github.com/bin9208/openpilot-rust/issues/5) defines staged transport/storage parity and an opt-in producer with exclusive topic ownership.
- [#6: model runtime feasibility](https://github.com/bin9208/openpilot-rust/issues/6) separately tests a Rust-owned runtime without hidden Python execution.
- AGNOS sysroot/ABI and on-device execution: not run. Generic GNU aarch64 artifacts are not device installation packages.
- Vehicle behavior and full runtime replacement: not done.
- CPU/thermal/deadline improvements and additional YOLO workload: not measured.
- The Ubuntu checkout still contains an LFS pointer at openpilot/selfdrive/modeld/models/driving_supercombo.onnx; required model assets must be fetched and verified before model or device work.

Keep overall runtime issue #1 open. Host software gates do not satisfy device, performance or vehicle acceptance.
