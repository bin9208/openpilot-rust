# Issue #214 — bounded upload failure attribution

Tracking: https://github.com/bin9208/openpilot-rust/issues/214, coordinated under
#497 on `codex/fix-497-stale-runtime-defects`. This is a diagnostic correction,
not a claim that the historical redirect303 failure has been explained or fixed.
No commit/push, CI dispatch, device operation or #296 tool conversion belongs to
this worker's change.

## Original evidence and concrete missing attribution

The original failure is source `a4297f8dd8c4867134a7c094f6d7d4c7db1c3ef2`,
[run37605580275/job112740111374](https://github.com/bin9208/openpilot-rust/actions/runs/37605580275/job/112740111374).
Read-only download authenticated artifact11475545604 (`rust-athena-evidence`)
against SHA256
`2788b9afda239949cb5a072b03c53f78648841079354c660a1b82bc270469bba`.
Its upload-edge evidence contains two complete identical native redirect303
PUTs, retry count 0 then 1, one final GET, and the failed exact request-list
assertion. It has no per-request response-write receipt or timestamps. The
comparison `status` was appended before writing the response; it establishes
intended fixture status, not that the response reached the native client.

`uploads.rs` additionally groups connection resets, unexpected EOF, TLS and DNS
errors into its existing `timeout` event and discards the concrete error there.
These relevant sources were unchanged from the failing source before this patch.
A passing rerun does not identify whether fixture write, transport or scheduling
caused the old observation. Neither the trace nor the new error categories can
retroactively reconstruct absent historical evidence.

## Added receipts and preserved behavior

`check_athena_upload_edges.py` now writes `request-trace.json` beside the existing
result/daemon log artifacts, including on comparison failure. It is a separate
schema-1 sidecar capped at 1,024 events, with a dropped-event count. The original
`all_requests` rows, method/body/hash/status fields, exact equality assertion,
redirect matrix, retry/expiration checks and deadlines are unchanged.

Every trace event carries wall/monotonic nanoseconds and one handler request ID,
client port, method, controlled source/native lane, controlled endpoint and final
hop label. Raw arbitrary paths, headers and error text are not copied into the
trace. Events distinguish:

- `request_accepted`: the handler has accepted a parsed HTTP request, before body
  reading; it is not a TCP-handshake receipt.
- `body_complete`: the read returned all declared bytes. Short reads instead
  produce a typed `body_read`/`unexpected_eof` error receipt with both byte counts,
  without changing the fixture's existing row/response behavior.
- `response_flushed`: `end_headers` and explicit `wfile.flush` returned. Only this
  receipt carries the written status. This proves local writes completed, not
  that the peer received or processed the response.
- `error`: fixed error category and fixture phase; the original exception still
  propagates through the server's existing error path.

`upload_http::upload` retains its signature, original return value/error, body
read/callback cadence, method/header/redirect handling and existing agent. A
failed call emits at most one fixed-schema `ATHENA_UPLOAD_FAILURE {JSON}` stderr
line, tested below 512 bytes; successful calls emit none. No primary log events
are added or altered, and `uploads.rs`, transport, websocket, retry policy,
Cargo files and HTTP request bytes are untouched.

Receipt fields are PID, a process-local attempt counter, zero-based redirect
index, upload boundary phase, fixed error category/detail, native-observed HTTP
status and attempt start/end wall/monotonic nanoseconds. `response_status=0`
means no response headers were returned for the failing hop. A received status
is recorded before draining its body, so a drain failure can identify a received
303 independently of the fixture's intended status. No supplied error Display/
Debug text, URL, local path, item ID, header or authentication value is formatted.
Wrapped typed ureq timeouts are classified by type, without error text.

Phases are `prepare_body`, `prepare_request`, `request`, `upload_body_read`,
`progress_callback`, `response_body` and `redirect`. `request` encompasses the
opaque `Agent::run` boundary; it does not assert which socket suboperation failed.
An explicit ureq timeout detail can separately identify its own typed subphase.
Unknown errors use fixed `other` labels. Clock acquisition cannot change the
upload result: unavailable monotonic/wall stamps use -1/0 respectively. Receipt
write failure is ignored, preserving the original result.

The fixture request ID correlates its own lifecycle, and client port identifies
its local connection. Native PID/attempt/hop and shared-host timestamps permit
bounded temporal comparison; they do not invent an exact cross-process request
ID or guarantee a unique pairing when concurrent attempts overlap. No correlation
header is added to the HTTP traffic.

## Focused tests and Actions handoff

The initial runtime and fixture regression cases were written before production
changes; additional boundary cases were added during review. None were executed
locally. The Athena unit test launches the actual test executable as a child and
calls the unchanged `upload` API. Owned loopback peers exercise EOF before
response headers, truncated response body, EOF on redirect hop1, malformed
redirect and success. A failing progress callback and absent input exercise
non-network phases. A connector injecting typed timeout, TLS, reset and nested
ureq timeout errors checks sanitization/classification through the same API.
The parent checks one receipt per failed call, no receipt on success, fixed
keys/size, expected phase/category/hop/observed status, valid timestamps and no
controlled URL/path/header/item-ID/error-text sentinels.

The Python fixture tests run its actual handler on owned HTTP servers. They
check unchanged comparison rows and real 303 method/location/status output,
response-write failure without a false flushed receipt, short body receipt
semantics without changing the prior HTTP response, and trace overflow accounting.
Only imports and standard HTTP peer work in the parent-hosted test invocation;
local work used AST parsing rather than importing these project modules.

Use the existing Athena Actions prerequisites and Rust 1.94.0. The Python tests
need the existing Athena fixture dependencies, including zstandard, requests,
cryptography, websockets and the retained source oracle dependencies. Parent
registers steps and artifact retention; this patch does not edit shared CI.

```sh
cargo +1.94.0 test --manifest-path rust/Cargo.toml --locked -p openpilot-athena --lib upload_http::attribution_tests::failure_receipts_are_sanitized_bounded_and_locate_real_upload_failures -- --exact --nocapture --test-threads=1
python3 -m unittest discover -s rust/tools/tests -p test_athena_upload_attribution.py -v
python3 rust/tools/check_athena_upload_edges.py "$PWD/rust/target/debug/openpilot-athenad" "$RUNNER_TEMP/issue-214-upload-attribution"
```

The final command requires the existing source-matched `openpilot-athenad` and
`openpilot-process-child` binaries from the usual Athena build; it must retain
`result.json`, `request-trace.json` and `daemon.log`. Full existing Athena source
comparisons remain the integration control. The ignored child test is deliberately
invoked by its parent test; it is not an independently accepted dormant test.

For a genuine runtime baseline RED in a disposable Actions checkout, retain the
new Rust test file, restore only production `upload_http.rs` from
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`, append the following declaration, and
run the same selected parent test. It must fail on the missing error receipt,
not a compiler error. Restore the candidate before GREEN, even after RED exits
nonzero. The tests depend only on the existing upload API, not new private helpers.

```rust
#[cfg(test)]
#[path = "upload_http_attribution_tests.rs"]
mod attribution_tests;
```

Local Rust formatting/parsing, Python AST syntax and scoped diff review are the
only validation performed. Parent must record hosted baseline RED, candidate
GREEN, fixture boundary tests and unchanged exact source comparison results.
Historical #214 root-cause resolution remains explicitly unclaimed.

The parent registered the genuine old-upload API assertion RED using only the
old production file plus the test-module hook, then candidate unit/strict-lint
and actual HTTP fixture GREEN in the existing Athena Actions job. The helper
restores candidate bytes in finally and rejects compilation/setup failures.
These are registered checks; execution results are still pending.
