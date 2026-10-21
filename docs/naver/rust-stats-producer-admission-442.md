# Stats producer historical source admission (#442)

Issue: https://github.com/bin9208/openpilot-rust/issues/442. This is a #296 developer-checker defect, not evidence of a vehicle runtime failure.

At20a8f52e `sources()` omitted absent retired `check_stats_producer.py` and `stats_source_producer.py`; later verification only revisited admitted files. A helper reappearing during one of the four worker scenarios could therefore leave `result.json` reporting PASS. The new guard keeps absent paths and real parent-directory identities, rechecks them through CLI output/handler teardown, and only commits its own inode-bound no-clobber result. Rollback preserves a foreign replacement receipt.

The focused regression uses the actual controller with a held private worker: all four mode outputs must exist, then the old controller must fail the specific assertion because it accepted reappearance. Compilation errors, missing inputs and unrelated failures do not count as old behavioral evidence. Current tests include unchanged admission, reappearance, parent replacement and foreign receipt rollback; the existing actual StatLog fork/drop/pressure/types comparisons remain mandatory.

The focused proof passed in
[run39530693250/job118995637520](https://github.com/bin9208/openpilot-rust/actions/runs/39530693250/job/118995637520).
Its one-time workflow push was `842ede1e81b2e7dae10d7d9fbb689546fd891ba5`,
while the checked-out source was `6ed383f01c8f7783824e84333ea9abdb13e1fc80`.
The old controller source was pinned to `20a8f52e83d85c7c5113c5484179b0c016a034eb`;
only the current focused test was copied into that old checkout. The old test
compiled and exited101 from the expected assertion after producing all four
reconnect/drop/pressure/types stdout records and a false PASS receipt. Current
source produced byte-identical four stdout records, rejected the reappeared
historical source, and left no `result.json`. The current finalization unit and
all seven focused tests passed, including real producer parity; strict Clippy
with `-D warnings` completed. The
[`stats-442-old-red-current-green` artifact #12316005404](https://github.com/bin9208/openpilot-rust/actions/runs/39530693250/artifacts/12316005404)
has ZIP SHA-256 `0dfceb7d3ed15e96d25345f6f2ab04a027f1769181c7f89290f295b8128d776b`.

Local review was source, log and artifact inspection only. Final filesystem
checking narrows the window; it does not claim atomic protection from mutation
after commit.
