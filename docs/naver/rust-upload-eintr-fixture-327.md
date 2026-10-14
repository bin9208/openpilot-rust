# Upload EINTR fixture child discovery (#327)

Tracking: [#327](https://github.com/bin9208/openpilot-rust/issues/327),
[full retirement #296](https://github.com/bin9208/openpilot-rust/issues/296).
CI source: `430ec72bf4fd5bc53dafb0e262b815e76c38ff37`.
[Failed exact-source job](https://github.com/bin9208/openpilot-rust/actions/runs/38566168156/job/115860106197).

All fourteen upload timeout scenarios passed before the separate EINTR fixture
failed. Its child-discovery predicate read `/proc/.../children` twice: the first
read could contain digits while the second was empty after process disappearance,
causing `int('')` to raise. This is a fixture reliability failure; the traceback
does not establish a runtime upload failure or why that child disappeared.

The helper now reads one children snapshot per polling attempt, converts that
same value, and accepts only one positive ASCII PID distinct from the tracer.
FileNotFoundError/ProcessLookupError during process discovery are retried;
permission and other errors remain failures. An exited tracer is diagnosed with
its real return code. Launch retains executable ownership receipts and captures
a bounded stderr tail on failure, rather than treating early child failure as a
successful/empty discovery.

Launch and recipient cleanup now cover discovery failures. strace's
`--kill-on-exit` binds failed-tracer cleanup to its own tracees; cleanup kills and
reaps only the owned tracer and closes pipes, avoiding signals to a remembered
possibly stale PID. An owned stopped-tracee regression exercises this path.
Peer closes even if launch or later reaping fails. The actual experiment's
SIGSTOP/SIGCONT signals, observed blocking checks, native EINTR requirement,
one-request assertion and all before/after deadline thresholds are unchanged.
No runtime upload implementation, retry limit or fourteen-timeout assertion
was modified.

## RED / GREEN and host evidence

Before the fix, the deterministic changing-file fixture supplied `123` followed
by empty text and reproduced the exact ValueError. A separate RED demonstrated
that launch failure left Peer.closed false because launch preceded try/finally.
The old fixture was executed directly; neither regression is merely a mock of
new implementation behavior.

Eight stdlib unittest cases now pass: single read per attempt, disappearing
process entry, empty/ambiguous/nonpositive/own/non-ASCII PID rejection, exited
tracer failure, real exit17/stderr diagnostics with reaping/pipe closure, owned
stopped tracee cleanup, Peer cleanup on launch failure and Peer cleanup even if
reaping fails. No pytest dependency was added; explicit stdlib-only imports have
narrowly explained Ruff exceptions. The CI job runs these cases before the
unchanged unscaled timeout and original/native EINTR gates.

Commands:

- `python -m unittest discover -s rust/tools/tests -p test_web_upload_eintr_peer.py -v`: eight passed.
- `ruff check rust/tools/web_upload_eintr_peer.py rust/tools/web_upload_eintr_compare.py rust/tools/tests/test_web_upload_eintr_peer.py`: passed.
- `git diff --check`: passed.
- `python rust/tools/web_upload_eintr_compare.py --binary /home/fma1/rust/cache/cargo-target/debug/examples/web_upload_trace --output <scratch>/tracked-full`: passed all four actual original/native before/after-deadline cases.

Parent built the host trace executable from unchanged runtime source430ec72b.
Binary SHA256: `8943a2d3811b69b1fc515cb28ac354b7a9e1538c804382e22ff68c96ae5943c7`.
Original `web_upload.py` SHA256:
`f22ee64e28ba1acd278c8dfc3bdf372648eb4d9ca37f9a1b0722f79f2203501d`.

The prepared scratch version already passed all four actual comparisons
(source/native before and after deadline), one request each, one interruption
before deadline and two afterward. The final tracked helper/compare are rerun
so validation does not rely on matching path-adjusted scratch code. Canonical
existing Python/strace environment was used, without dependency installation or
Cargo/build activity by this worker. This does not retire these remaining Python
comparison tools or establish device/CPU/full-runtime acceptance. New exact-SHA
CI remains parent-owned after integration.

Evidence: ignored `.analysis/scratch/2026-10-14-upload-eintr-327/`.
Docs-Not-Needed: CI fixture reliability and internal evidence, no setting changes.

Final tracked-run results: source before0.044s, native before0.013s (owned-session
success); source after12.047s (ReadTimeout), native after12.387s (receive-body
timeout). Every case made exactly one request; before cases recorded one
interruption, after cases two. Native trace contains EINTR and all unchanged
deadline/signal assertions passed. `tracked-full/result.json` records the exact
binary/original hashes above.

## Follow-up: target identity before fault injection (2026-10-15)

At `2d07cdc988788d8b696e73202283887b02e3cb4a`, [job115933282728](https://github.com/bin9208/openpilot-rust/actions/runs/38587163535/job/115933282728) passed seven fixture tests but the stopped-child cleanup test failed: its patched discovery seam sent SIGSTOP before executable validation, and the selected PID had disappeared (`ProcessLookupError`). The assertion expected its deliberate `owned discovery failure`, so the actual error was correctly rejected rather than accepted as equivalent.

Source inspection also identifies a real helper discovery weakness: launch selected a unique child PID first, then repeatedly read only that fixed PID's executable. If a transient child vanished before target exec, the helper never selected a later owned child and eventually failed its receipt bound. The failing CI does not independently establish that child's executable or the exact strace startup transition.

Discovery now accepts an optional expected executable and checks it in the same polling attempt as the one positive/unique children snapshot. FileNotFoundError/ProcessLookupError or mismatching executable restart the **whole** selection under the original bounded polling gate. Permission/other errors remain fatal; an exited tracer retains its real error/status/stderr. Launch uses this combined gate and returns a target identity only after the executable matches. The cleanup fault seam similarly waits for target identity before stopping it. No remembered PID is used for cleanup: only the owned `Popen` tracer is killed/reaped, with strace kill-on-exit retaining tracee cleanup.

A deterministic regression drives actual launch discovery with child snapshots123 then456, making123's executable disappear and456 match the target. Old code failed its controlled gate bound because it kept reading123; fixed code returns456 after exactly those two executable reads. Additional cases cover both disappearance errors, wrong executable, permission denial, and retain malformed/nonpositive/ambiguous child rejection, one snapshot per attempt, real exit17/stderr/reaping, real stopped-target cleanup, and Peer.close on failure.

Validation:

- `python -P -m unittest discover -s rust/tools/tests -p test_web_upload_eintr_peer.py -v`:11tests PASS; deterministic replacement case RED before helper fix, GREEN afterward.
- Targeted Ruff Python3.12 and `git diff --check`: PASS.
- Full `web_upload_eintr_compare.py --case both` with the current worktree first in PYTHONPATH: all four original/native cases PASS, one request each, one interruption before deadline/two after, native EINTR present. Before source/native0.038/0.013s returned owned-session; after12.029/12.211s timed out under the unchanged11.5≤elapsed<14s assertions. Compare script SHA256 remains `3757f24089ebd1cb4035633d20a55622427f4b0525e24f3ff0680bde6c6750d3`; helper SHA256 `106f4a6f5119a23fd44559faec6dba78a48bb6d79414e22120b50c4de5cd8325`.
- Reused host upload binary SHA256 `8943a2d3811b69b1fc515cb28ac354b7a9e1538c804382e22ff68c96ae5943c7` and original upload source SHA256 `f22ee64e28ba1acd278c8dfc3bdf372648eb4d9ca37f9a1b0722f79f2203501d`; no Cargo/install/runtime changes. Four-case receipts under ignored `.analysis/scratch/2026-10-15-upload-eintr-327/full-current/`.

An earlier invocation inherited canonical env's main-checkout PYTHONPATH and imported that checkout's obsolete helper, failing a stop_child signature mismatch; it was not counted as current-source validation. Explicit current-worktree PYTHONPATH corrected the environment, then the four-case gate ran once. Scoped process inspection found no remaining owned fixture tracees. No rerun-until-pass, relaxed failure assertion, deadline/signal/request change or vehicle access. Fresh exact-SHA CI remains required.


The full workspace Python suite also collects and executes these stdlib tests.
Its existing dependency-install step now explicitly includes strace, matching
the dedicated upload and UI gates, so the real-process controls do not depend
on an incidental runner image package. No fixture is skipped.
