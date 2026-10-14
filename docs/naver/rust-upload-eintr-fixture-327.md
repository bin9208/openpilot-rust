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
