# Active Carrot upload transport

Issue [#52](https://github.com/bin9208/openpilot-rust/issues/52), under full runtime
[#1](https://github.com/bin9208/openpilot-rust/issues/1). Source provenance is
`openpilot/selfdrive/carrot/web_upload.py` and the pinned aiohttp 3.13.3 / requests
2.34.2 behavior. Original licensing remains unchanged.

`openpilot-web-upload` implements the shared active Carrot upload helpers in Rust.
Its blocking API belongs on an upload worker; it must not block a future server
event loop. Ordered metadata uses the existing native logging value type and
Python-compatible JSON formatting. Session payloads retain device-ID fallback,
160-character value limits and purpose. Environment/settings URL precedence,
component quoting, static Bearer overrides and the independent Carrot Logs target
remain separate from the optional legacy comma uploader.

Folder upload preserves automatic sorted non-symlink selection, explicit ordered
selection/deduplication, token/filename error ordering, original file bytes and
1 MiB read/progress/cancellation boundaries. Each file has two logical attempts;
progress starts at zero on each attempt. Successful 2xx plus truthy JSON `ok` and
matching integer remote size are required. A previously successful file stays
uploaded when a later file fails. No xattrs are marked and no files are deleted.
The folder helper never sends completion, including after cancellation/failure.
Completion is an explicit separate operation whose original 2xx acceptance does
not require an `ok` response body.

Native HTTP uses pinned ureq 3.4.2 with verified TLS. A transport adapter retains
12-second total async session/health/completion deadlines, 20-second upload
connect and 180-second per-read upload timeout without a total/write deadline,
12-second synchronous session socket timeout, and 30-second tmux socket timeout.
The source's idempotent GET/PUT disconnect retry is preserved separately from the
two file attempts; exhausted streams remain exhausted on this internal retry.
Redirects preserve source method/body/header behavior. Multipart fields and both
file bodies match requests after normalizing the random boundary.

ureq re-frames a source 1 MiB application chunk into smaller HTTP transfer-coding
chunks. Application payload bytes and progress/cancellation chunks match; identical
wire frame sizes are not a requirement. Both original/native framing traces are
retained. Native socket waits can return slightly after the requested duration; an elapsed
check rejects late data instead of accepting it. Native transport errors have
native diagnostic wording; source response,
selection, cancellation and retry outcomes are compared directly. The native API
uses UTF-8 paths and JSON-compatible ordered metadata; arbitrary Python objects
and undecodable filesystem names do not become uploadable values.

## Validation

`check_web_upload.py` imports and executes the original helper module against the
same local HTTP fixtures as the Rust example. The 99 scenarios check URL/settings metadata,
actual request bodies and headers, progress, symlink/explicit selection, status and
JSON failures, remote-size coercion, two attempts, disconnect retries, partial
folder results, cancellation, callback exceptions, and redirects. Every request
has a raw body artifact or an explicit encoded empty-body artifact; hashes and
chunk traces are recorded. Multipart boundaries are normalized only for comparison.

`check_web_upload_timeouts.py` uses the real unscaled constants: approximately
12-second total failure (including TLS), two 20-second TLS connection failures,
30-second socket failure, and two 180-second file read stalls. Progressing responses lasting over 12 seconds succeed for synchronous
session and file upload. Original/native scenarios run concurrently using separate
loopback receivers. This is a roughly six-minute test, required by the isolated
Rust CI gate in a separate job.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-web-upload --all-targets --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-web-upload --examples --locked
uv run rust/tools/check_web_upload.py \
  --binary rust/target/debug/examples/web_upload_trace --output /path/to/fast-evidence
uv run rust/tools/check_web_upload_timeouts.py \
  --binary rust/target/debug/examples/web_upload_trace --output /path/to/timeout-evidence
```

Generic GNU aarch64 cross-build and the same fast local HTTP scenarios under qemu
are host evidence only. No device connection, real settings/tokens, production
upload endpoint, manager selection, server/watchdog replacement or deployment is
part of this task. Runtime code invokes neither Python nor project shell helpers.
Dashcam job state/catalog/routes, diagnostic capture and startup/watchdog integration
remain necessary before a complete runtime candidate can be handed to the user.
The evidence index is `.omo/evidence/web-upload/evidence.json` in the issue worktree;
exact-SHA cloud validation belongs to the integration handoff.

## Interrupted TCP receive comparison (2026-10-08, issue 237)

The CarrotMan ARM comparison on dev `1afdcc2a6` recorded an interrupted upload
read and missed its final onroad web upload. A controlled local socket test
reproduces the error with the unchanged transport: stopping and resuming its
owned child during `recvfrom` yields `EINTR`, while the original synchronous
session helper receives the response. The CI signal source remains unknown;
this reproduction establishes the error path, not that signal's origin.

The transport now retries only an interrupted non-TLS input operation on the
same connection, using the remaining original monotonic deadline. It sends no
second HTTP request and retains the existing late-completion check. Four source
and native controls pass with retained example SHA256
`6976360cab56fcda5dfe1a93e379b544f90b72fd702b712a563e1d1e4305ca6c`:
an available response succeeds after interruption, and a held response still
times out after two interruptions. The latter takes 12.025 seconds in the
source and 12.339 seconds in native on this host; identical wakeup timing is
not claimed. Each receiver observes exactly one request with the same body.
Traces retain both interruptions, the same socket descriptor and decreasing
receive timeouts. TLS retry behavior is outside this change.

Evidence is under `.omo/evidence/carrot-man-237/` in the issue checkout;
`green-both-restart-gate/result.json` records the passing observations and
earlier failing receipts remain retained. The selected build, two library
tests, strict all-target Clippy, formatting and 19 isolation checks pass.
The existing required upload-timeout job now also runs
`rust/tools/web_upload_eintr_compare.py` and retains its socket traces.
Exact-head CI, full startup/upload and device acceptance remain separate.

Docs-Not-Needed: internal library port preserving existing settings and behavior,
with no active runtime selection change.
