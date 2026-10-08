# Xiaoge HTTP settings comparison phase (#247)

PR #245 head `07cec783d85eeb20502935ea57394a7d008113fb` completed 65
HTTP/TCP comparison cases, but seven encoding responses differed. The original
returned smoothing `0.2505` and lane threshold `0.355`; Rust returned `0.25`
and `0.36`. The [host job](https://github.com/bin9208/openpilot-rust/actions/runs/37815592288/job/113444055728)
failed while the other 60 reported checks passed. The aggregate Rust gate
correctly failed as well.

Both implementations persist rounded integer Params, retain the precise HTTP
settings immediately, and refresh them from Params on the original one-second
interval. The fixture had issued the same request sequence at independent
refresh phases. Using the exact failed CI executable, an actual HTTP A/B/A
control observed precise values, their natural rounded reload, and precise
values again. All 21 complete response pairs matched at matching phases;
deliberately opposite phases reproduced all seven differences in both
directions. Every stored Onnx Params byte matched.

The fixture now primes the same fractional payload and observes its natural
reload before making one compared immediate update. It asserts the precise
response, then observes the next natural reload before the later partial and
encoding requests. It never retries a mismatching compared response, changes
runtime time, rewrites returned values or removes compared fields. The original
65 case bodies, persistence behavior and refresh interval remain unchanged.

The affected 65-case HTTP/TCP comparison passes once after this change, with
both process exits zero, exact Params bytes and the two existing five-second
snapshot deadlines retained. A real HTTP peer that never reloads causes the
new helper to fail after 3.035 seconds and 249 recorded replies. Five existing
Xiaoge CI-helper checks, 22 repository-isolation checks, Ruff and diff checks
pass. No Rust binary was rebuilt and no unrelated model/IPC corpus was rerun.

The reused executable SHA256 is
`a43051a579b1e7092a421356497038cecf2f0fc18ee25cc8f9760923d6d17cb2`.
The failed Actions artifact, phase observations, exact commands and source
identities are indexed by
`.omo/evidence/247-xiaoge-settings/checkpoint/receipt.json`.
The ordinary `rust/tools/check_xiaoge_runtime.py` command runs the corrected
fixture. Fresh exact-head CI remains required before integrating PR #245.
Complete runtime startup/upload and device acceptance remain separate gates.
