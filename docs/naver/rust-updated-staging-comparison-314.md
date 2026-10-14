# Updater staging-preservation comparison correction (#314)

Tracking: [#314](https://github.com/bin9208/openpilot-rust/issues/314).
The PR #311 Rust startup-services job
[115813010950](https://github.com/bin9208/openpilot-rust/actions/runs/38552326535/job/115813010950)
failed in `check_updated.py` on the source `0992e61fcadff2957c32705efc16ada9584557ed`
(Actions merge `61b0119e19f44b0892b15930ff220d8273f02f10`).

The captured `rust-startup-services-evidence` artifact `11887213115` shows five
identical snapshots, including all Params bytes, waits, branches, consistency
and connectivity state. AGNOS calls also match. The sole difference is five
original `sudo rm -rf <ROOT>/data/stage` commands absent from the native trace.
The native updater intentionally preserves the transaction lock and complete
rollback checkouts, recreating only disposable overlay directories. Reinstating
whole-staging deletion would violate the source/package transaction contract
from #290/#297.

The comparison now permits only omission of that exact command with null cwd.
Every other command, argument, cwd, order, snapshot and top-level field remains
strictly compared. Each live fixture additionally seeds a complete rollback
sentinel and a transaction lock with recorded bytes/inode. Native execution must
retain both; the original must demonstrate deletion. An open descriptor prevents
inode recycling from hiding lock replacement. Both implementations must leave
active Git HEAD unchanged, including failure scenarios.

Checker regressions independently reject changed snapshots/Params/AGNOS calls,
changed or extra commands/cwd, destructive native removal, removal of a different
source path, corrupted rollback/lock bytes and a replaced lock inode. CI runs
these regressions before the original/native trace comparison. The initial RED
was the exact-SHA Actions failure; checker tests initially also failed because
the narrow comparison function did not exist. All 16 parametrized pytest cases pass, including each negative control.
Applying the new comparison to the captured CI JSON accepts exactly five omitted
commands and retains all 139 native commands and five snapshots.

The full local original/native rerun passed all five scenarios: 19 snapshots and
322 ordered native commands. These include missing-branch failure, metered fetch,
connectivity thresholds and both AGNOS boundaries. Every case independently
retained native rollback/lock bytes and inode while leaving active Git HEAD
unchanged. Twenty further negative controls on these actual traces reject the
unmodified original as native peer, altered Params, altered commands and a native
whole-staging removal. Captures and results are in ignored
`.analysis/scratch/2026-10-14-updated-ci/local-pairs/`.

The bounded host `updated_trace` build passed in 2.90 seconds with 379 GiB free;
the canonical environment selected the existing Python 3.12 Params binding for
the independent original-source peer. No runtime source was changed by this
correction. The existing native package/finalization failure tests remain
mandatory and were not rerun by this scoped fixture correction. This
record is fixture/host evidence, not complete candidate CI, package retirement,
device startup, recording/upload or drive acceptance.

Docs-Not-Needed: internal validation correction for the approved runtime update
transaction; no user setting or guide behavior changes.
