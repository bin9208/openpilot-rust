# #21: diagnostic sink acceptance/close test ordering

Issue: <https://github.com/bin9208/openpilot-rust/issues/21>. Parent: #497.
Source baseline: `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.
Worktree: `/home/fma1/.codex/worktrees/backlog-fixes-497/openpilot-rust`.
Branch: `codex/fix-497-stale-runtime-defects`.

## Finding and inference limit

The parent identified the sporadic CI failure in
`AsyncDiagnosticSinkTest.closeCannotPassAnOfferThatObservedOpen`. Current source
and the local open-issue audit were inspected; no GitHub operation was performed
under the parent's latest restriction.

`offer` holds `acceptanceLock` from the open-state check through queue insertion.
`stopAccepting` waits for the same lock, sets `accepting=false` and `closing=true`,
then invokes `afterAcceptanceClosed`. Only after this hook returns does
`drainAndClose(500)` interrupt the worker and start its deadline. It allows 250 ms
for graceful termination, then publishes terminal cancellation and closes the
consumer. The worker checks terminal state before reserving a consumer call.
A true return from `offer` confirms queue acceptance, not guaranteed delivery
after this cancellation boundary.

The old test released the offer and joined close before checking receipt. It did
not control worker progress during that 250 ms grace interval. The partial-batch
flush interval is also 250 ms. Interrupt placement and worker scheduling can leave
an accepted frame pending at cancellation. Waiting another second for receipt
after close cannot undo the terminal gate. The test conflated acceptance-lock
ordering with an unconditional delivery guarantee beyond the close budget.

This establishes a legal schedule explaining the failed assertion. No retained
thread trace establishes that it was the exact historical CI schedule. No
production defect or broader concurrency correctness is claimed; production
`AsyncDiagnosticSink.java` is unchanged. Hosted execution is pending.

## Deterministic tests

Only `AsyncDiagnosticSinkTest.java` changes:

- The original test still holds the producer after it observed open, starts
  close, observes close waiting for acceptance, and asserts acceptance has not
  closed. It then releases the producer. An opt-in latch in the existing
  `afterAcceptanceClosed` test observer keeps close before its drain budget until
  the worker actually records the frame. The worker never needs acceptanceLock.
  It then releases close and verifies exactly one frame with sequence 1, producer
  and closer exit, worker termination, one consumer close, rejection of later
  offers, and zero storage errors/queue drops.
- `acceptedFramePastCloseBudgetIsCancelledBeforeConsumerReservation` accepts a
  frame and blocks the worker using the existing interrupt-resistant reservation
  observer. Close runs with the same 500 ms budget. The reservation stays blocked
  until close returns and terminal/consumer closure are observed. Only then is it
  released. The test verifies worker termination, zero delivered frames, one
  consumer close, rejection of later offers, and zero storage errors/queue drops.
  The existing zero-budget cancellation test is retained separately.

Both paths release gates in `finally`. The close budget and existing one-second
watchdogs are unchanged. There are no added sleeps, skipped tests or wider
timeouts. These latch dependencies separate pre-budget delivery from deliberate
budget exhaustion rather than relying on repeated successful CI runs.

## Hosted commands and deterministic RED recipe

Run from repository root on GitHub-hosted Actions only, using the existing Java
21, Gradle 9.0.0, Android platform 36/build-tools 36.0.0 setup. Focused checks:

```sh
gradle -p tools/naver_map_patch/dexpatch clean test --no-daemon --dependency-verification=strict -PpayloadMode=diagnostic --tests 'ai.comma.naver.payload.AsyncDiagnosticSinkTest'
gradle -p tools/naver_map_patch/dexpatch clean test --no-daemon --dependency-verification=strict -PpayloadMode=diagnostic -PfieldAcceptance=true --tests 'ai.comma.naver.payload.AsyncDiagnosticSinkTest'
```

For deterministic RED, use a disposable hosted checkout containing the candidate
tests. First restore the original production sink, then change only the new
controlled cancellation case's frame-count expectation to the old test's
unconditional delivery assumption:

```sh
python3 - <<'PY'
from pathlib import Path
import subprocess
root = Path('tools/naver_map_patch/dexpatch/src')
production = root / 'payload/java/ai/comma/naver/payload/AsyncDiagnosticSink.java'
production.write_bytes(subprocess.check_output([
    'git', 'show', 'b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:' + str(production)]))
test = root / 'test/java/ai/comma/naver/payload/AsyncDiagnosticSinkTest.java'
source = test.read_text()
before = 'assertEquals(0, consumer.batches.stream().mapToInt(List::size).sum());'
after = 'assertEquals(1, consumer.batches.stream().mapToInt(List::size).sum());'
assert source.count(before) == 1
test.write_text(source.replace(before, after))
PY
gradle -p tools/naver_map_patch/dexpatch clean test --no-daemon --dependency-verification=strict -PpayloadMode=diagnostic --tests 'ai.comma.naver.payload.AsyncDiagnosticSinkTest.acceptedFramePastCloseBudgetIsCancelledBeforeConsumerReservation'
```

Expected RED: JUnit reports expected **1**, actual **0** after the accepted frame
is held through the actual close budget and the worker terminates. This is a
controlled reproduction of the former test's invalid assumption on unchanged
production, not evidence of a production fix. A compile error, latch timeout or
different failure is not the intended RED. Restore that expectation to **0**
before candidate GREEN and retain both test methods. No production mutation is
needed for GREEN.

The existing full-integration matrix already runs these complete variant checks:

```sh
gradle -p tools/naver_map_patch/dexpatch clean test --no-daemon --dependency-verification=strict -PpayloadMode=diagnostic
gradle -p tools/naver_map_patch/dexpatch clean test --no-daemon --dependency-verification=strict -PpayloadMode=diagnostic -PfieldAcceptance=true
gradle -p tools/naver_map_patch/dexpatch clean test --no-daemon --dependency-verification=strict -PpayloadMode=production
```

Parent owns exact-SHA RED/GREEN, variant/full-integration results and issue
acceptance. No Java compiler or parser is available locally; review and scoped
`git diff --check` are the local validation. No Gradle, tests, fixtures, builds,
commits, pushes, GitHub operations or new agents were used. Docs-Not-Needed:
test scheduling correction only; no APK runtime or user behavior changes.
