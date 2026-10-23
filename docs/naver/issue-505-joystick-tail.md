# Issue #505: finite joystick checker drain

Issue: [#505](https://github.com/bin9208/openpilot-rust/issues/505).
Parent: [#497](https://github.com/bin9208/openpilot-rust/issues/497),
[PR #500](https://github.com/bin9208/openpilot-rust/pull/500).
Source investigated: `afed1a4638a8d0ec32b4f2744c47a80253748179` on
`codex/fix-497-stale-runtime-defects`, isolated backlog worktree.

Status, 2026-10-23: checker/helper/regression source prepared; local syntax and
scoped diff checks only. No functional tests, project imports, fixtures,
binaries or builds were executed locally. Candidate Actions evidence is pending
parent integration. No commit, push, workflow edit or GitHub mutation was made
by this worker. #208 remains unchanged. This is independent of the separately
owned #296 Rust development-tool conversion.

## Actual failure and evidence limits

The [afed control-tools job](https://github.com/bin9208/openpilot-rust/actions/runs/39841989845/job/120045870564)
failed in `check_joystickd_runtime.py`'s final drain (`while receive() is not
None`, original line 150), at the original two-second publication timeout
(line 121). The `rust-joystickd-evidence` artifact, ID `12455391827`, contains:

- Source/direct comparison: 8 cases, 1,805 steps, zero mismatches.
- All five actual IPC phases matched: fresh at paired record 5, expired 26,
  recovered 30, override 34, disabled 38.
- 239 valid paired records, spanning 2.380133282 seconds for `carControl` and
  2.379960479 seconds for `controlsState`.
- Paired `controlsState` timestamps about one frame after `carControl`:
  9,869,730..10,200,639 ns (median 10,028,868 ns). Each controlsState except the
  final record is only 19,957..66,304 ns after the next paired carControl.
- Empty daemon stdout; stderr only reports waiting for CarParams. There is no
  final runtime receipt or later startup/signal/CLI result from this failed job.

The old failure path saved only complete pairs. It did **not** save a partial
message from the failing receive or the daemon's exit status before cleanup.
A final unmatched carControl with successful producer exit is a supported
hypothesis, not a captured fact. The patch does not manufacture that evidence.
The downloaded artifact zip SHA-256 is
`d7f0cbbab7decffb06e126e3a9e0f075df94492a954c9e1aae1d1e0cc1906ab5`;
it was 49 KB compressed / 1,031,842 bytes extracted, after checking free disk.

The checker, joystick runtime and relevant messaging/Params sources were
unchanged between stable `b55552dbb` and `afed1a4`. The original #208 record
already documents 239 observed pairs after a 240-iteration publisher takeover.
Neither this failure nor subscriber warmup establishes a new runtime output
regression caused by the backlog batch.

## Failure mechanism and change

The old nested receiver held one local message from each subscription. It
recognized producer exit only when **both** locals were empty. A completed
producer with one unmatched message therefore waited for an impossible
counterpart until the unchanged two-second deadline. Each msgq subscription
has its own reader registration/reset point; the checker cannot assume they
observed identical startup frames. The exact reset interleaving in the failed
job was not traced.

`joystick_publications.py` keeps each channel's native messages and serialized
evidence independently. It returns FIFO pairs to the unchanged phase snapshot
comparison, then retains any unmatched messages separately. A successful EOF
requires observed producer exit code zero followed by an empty read of both
subscriptions after exit was observed. That extra pass also covers a producer
publishing and exiting between an empty read and the first completed poll.
All queued pairs are consumed before successful EOF.

The intentional behavior difference is that a successful finite producer can
leave validated unmatched messages without causing a false timeout. A running
producer with only one channel still times out at two seconds; progress does
not refresh the deadline. A nonzero exit fails after queued pairs are drained.
Empty/premature exit fails the existing phase checks or minimum counts. More
than 240 observed messages on either channel fails explicitly; retained
messages remain bounded at 240 per channel.

Preserved gates: the two-second receive deadline, three-second process waits,
at least 235 complete pairs, each channel independently at least 235, the
requested 240-frame cap, all five phases with three consecutive matches and
80 attempts, valid publications and positive timestamps, controlsState
debugState, the paired carControl duration of 2..4 seconds, startup behavior,
signal exits and invalid CLI checks. Unmatched messages receive the same
validity/timestamp/debugState checks and must contain the subscribed payload.
No source-equivalence exception, runtime change or threshold waiver is added.

## Evidence compatibility and failure capture

- `publications.json` remains the legacy list of complete
  `{carControl: ..., controlsState: ...}` records. `matches.json` is unchanged.
- `channel_publications.json` retains every observed message up to the
  per-channel cap, including an invalid message that triggers failure.
- `receiver.json` includes actual channel counts, validated/retained counts,
  paired count, unmatched counts and payloads, drain status, last return code
  observed by the receiver, and `returncode_before_cleanup`. JSON null means
  the producer was still running when polled before cleanup, not an invented
  successful exit. It separately records the return code after cleanup.
- The original failure type/message and any cleanup failure are saved even if
  the three-second cleanup wait fails. Cleanup failure cannot convert a check
  into success or replace the original receive failure.
- A successful final `receipt.json` preserves integer
  `publications_per_service` as the historical **paired count**. New
  `channel_counts`, `unmatched_tail_counts`, `returncode_before_cleanup` and
  `receiver_evidence` fields explicitly describe the independent subscriptions.
  Per-channel counts are not silently substituted for the legacy integer.

## Parent validation handoff

Tests were authored before the helper/receiver replacement. The stdlib suite
uses controlled channels and a virtual monotonic clock; it imports no cereal,
msgq or source oracle and launches no project processes. The saved fixture is
the actual original nested receive function from checker blob
`fbf949431043b611eee0c928f0e1323e06f1e23c`. A controlled 240/239 startup offset
requires the old function to hit its original two-second timeout and the new
receiver to retain 239 pairs plus the real controlled tail. This is a
counterexample to the old function, not a claim about missing afed evidence.

Other controls cover the reverse offset, delayed channel with a retained
backlog, dead/empty EOF, exit during drain, the empty-read/exit race, live
partial/empty timeouts, a fixed deadline despite progress, nonzero/premature
exit, 241-message overrun, invalid/timestamp-zero/payload/debugState tails,
and the actual checker's failure finalizer via AST with controlled process
status and cleanup timeout. No copied replacement cleanup logic is tested.

Run in the next necessary registered `bin-actions` job, after the separately
owned #502 foundation is ready, from the repository root:

```sh
PYTHONPATH="$PWD/rust/tools" python -m unittest discover -s rust/tools/tests -p 'test_joystick_publications_505.py' -v
```

Retain the existing full control-tools job and its source/native binding
setup. Its actual IPC command, after that setup and the unchanged source
comparison, remains:

```sh
export PYTHONPATH="$RUNNER_TEMP/joystickd/msgq-binding:$PYTHONPATH"
python rust/tools/check_joystickd_runtime.py --binary "$PWD/rust/target/debug/openpilot-joystickd" --binding "$JOYSTICKD_PARAMS_BINDING" --source "$RUNNER_TEMP/joystickd/source" --output "$RUNNER_TEMP/joystickd/runtime"
```

The existing always-upload `runtime/**` artifact path includes the new channel
and receiver evidence without a workflow edit. Parent owns unit-test
registration, full-job execution and exact candidate SHA/Actions links. Actual
success must include all five phases, 235..240 messages on **each** channel,
235..240 complete pairs, successful finite drain with pre-cleanup exit zero,
validated tails, duration, and the later waiting/malformed/CLI/active-signal
checks. Do not classify setup or assertion failures as proof of a fix.

All future necessary Actions execution uses registered `bin-actions`
(`self-hosted`, `Linux`, `X64`, `dev-md-local`), with operation-specific disk
admission and no retired fixed reserve. Any ARM checks remain in separate ARM
user space through the #502 QEMU/PRoot foundation; record emulation and host/
guest architectures, never physical ARM acceptance. No GitHub-hosted fallback
or rerun of unchanged successful checks merely to change runner is requested.
Workflow routing and ARM foundation files remain owned by the parent/other
workers. Device/drive acceptance is not claimed by these checker tests.
