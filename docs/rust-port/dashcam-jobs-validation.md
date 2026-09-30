# Native dashcam upload jobs

Issue [#61](https://github.com/bin9208/openpilot-rust/issues/61) extends the
[active upload transport](web-upload-validation.md) with the job state,
selection/catalog helpers, metadata, completion reports and bounded concurrent
execution used by the active Carrot dashcam service. Source provenance is
`openpilot/selfdrive/carrot/server/features/dashcam/{upload_jobs,upload,catalog,paths}.py`
and `server/services/dashcam_upload_report.py`. The original files are unchanged.

`openpilot-dashcam-upload` executes the orchestration natively. Its JSON-line
driver exposes start, snapshot, cancel, expiration and retained-job inspection
for host validation; the internal `--worker` mode receives the validated job
request through a private pipe. This increment is not the Carrot HTTP server,
full route catalog, watchdog/tmux integration or production manager selection.

## Execution and ownership

The manager owns job snapshots and one child per active job. Each child owns
bounded preparation/upload workers, the native HTTP transport, progress events
and completion notification. The selected root used for admission is carried
into execution, including when runtime settings are loaded. Concurrent segment
limits default to three and clamp to one through six, preserving source integer
conversion and invalid-input fallback.

The state machine preserves monotonic running progress, changed-field revisions,
normalized 60,000-character logs, partial results, cancel requests, the
1,800-second stale threshold and twelve completed-job retention. Logical bytes
use the maximum observed offset per file, while transfer-rate samples include
retransmitted bytes in the source three-second window. Completion payloads
retain selected order despite concurrent completion. Notification failure does
not change file-upload success; cancellation omits completion notifications.

Worker output is processed without requiring another API call. Ended children
are reaped automatically; invalid output/early exit fail an unfinished job.
Manager destruction and stale expiration terminate and reap owned children.
The Linux worker requests SIGKILL when its spawning owner dies and checks parent
identity before accepting the request. The current driver spawns from its
persistent main thread. A future server integration must retain that ownership
lifetime. Actual socket-close tests cover normal completion, killed worker,
owner stdin EOF, SIGTERM and SIGKILL. These are native process-ownership checks,
not a claim that the source async implementation uses child processes.

## Metadata and source compatibility

The runtime reads Params through the
[#64 typed STRING adapter](../naver/rust_params_string_20260930.md). Missing,
empty, unreadable and invalid UTF-8 values preserve the original getter and
`param_text` fallback boundaries. Invalid conversions use the actual logging
producer, including original warning text and honest Rust file/line identity.
The helper catches getter/logging errors as the original `param_text` does.
The worker keeps its logger alive throughout execution.

Serial precedence, repository metadata, saved URL alias/migration and environment
overrides follow the original helpers. Git queries use the original four-second
limit. Discord requests use the native twelve-second total HTTP operation and
retain mention suppression, message flags and source content limits. Validation
uses synthetic credentials and loopback receivers.

Commit links intentionally identify `bin9208/openpilot-rust`, where native
commits exist. The source hardcodes `ajouatom/openpilot`. The report oracle maps
that one repository prefix on the original generated lines before the unchanged
source share-text/Discord length logic executes. Message content, ordering,
grouping and limits otherwise remain source comparisons; upstream files are not
rewritten and Rust commits are not misidentified as upstream commits.

## Host evidence and reproduction

- `check_dashcam_jobs.py`: actual source job functions, 2,099 fixed-clock
  transitions across 21 scenarios, including exact revisions.
- `check_dashcam_catalog.py`: source pathname/file selection, URL quoting and
  report functions; 1,929 names, ten filesystem cases, fourteen URLs and 36
  report payloads. The report repository mapping above is explicit.
- `check_dashcam_metadata.py`: 3,173 actual-source comparisons for concurrency,
  obfuscated-string decoding, saved/environment URL selection and real Git.
- `check_dashcam_params.py`: 93 metadata/getter cases with the actual compiled
  Cython/C++ Params binding, real files and original/native ZMQ log records.
  These include strict UTF-8 warnings, serial/environment precedence and the
  helper's fallback after a closed logging socket.
- `check_dashcam_runtime.py`: original async job functions and original aiohttp
  transport versus the native process and real loopback HTTP. Cases cover
  concurrency one/three/six, session creation, partial failure, notification
  failure, cancellation, stale abort, multi-chunk retransmission and selected
  root propagation. Payload hashes, file sizes, headers, ordered results and
  applicable final job fields are compared. Time-dependent observations remain
  observations rather than fabricated equal clocks.
- The same runtime test captures actual native child writes and replays their
  packets/clocks through unchanged source state functions. Final snapshots,
  including the exact revision counter, must agree. This supplements the real
  independent-clock runs; it does not replace HTTP or execution comparisons.
- `check_dashcam_lifecycle.py`: five native process/connection ownership cases.

The generic GNU ARM64 build passes. Under QEMU, the ARM state/catalog/metadata
and actual typed-Params examples pass the same 2,099/1,929/3,173/93 comparisons.
The ARM upload worker also passes all ten HTTP scenarios and five ownership
scenarios when owned by the host build of the identical manager driver. The
`dashcam_worker_host` example supplies an explicit worker executable to the
existing `Manager` API; it adds no alternate production behavior. The direct
ARM manager's child re-execution fails with a broken control pipe in this host's
QEMU setup, which has no ARM binfmt registration. That failed attempt is retained;
the mixed-architecture fixture is not described as full ARM manager execution.

```sh
cargo build --manifest-path rust/Cargo.toml -p openpilot-dashcam-upload --bins --examples --locked
python rust/tools/check_dashcam_jobs.py --binary rust/target/debug/examples/dashcam_job_trace --output /tmp/dashcam-state
python rust/tools/check_dashcam_catalog.py --binary rust/target/debug/examples/dashcam_catalog_trace --output /tmp/dashcam-catalog
python rust/tools/check_dashcam_metadata.py --binary rust/target/debug/examples/dashcam_metadata_trace --output /tmp/dashcam-metadata
python rust/tools/check_dashcam_params.py --binary rust/target/debug/examples/dashcam_params_trace --binding /tmp/original-params/params_pyx.so --output /tmp/dashcam-params
python rust/tools/check_dashcam_runtime.py --binary rust/target/debug/openpilot-dashcam-upload --output /tmp/dashcam-runtime
python rust/tools/check_dashcam_lifecycle.py --binary rust/target/debug/openpilot-dashcam-upload --output /tmp/dashcam-lifecycle
```

The Params binding is built by `rust/tools/build_params_python.py` with the
original source and locked native dependencies. Runtime packet capture requires
Linux `strace`; lifecycle tests use Linux `/proc` and a test-only subreaper to
collect workers orphaned by the deliberate owner-kill scenarios.

Local native tests do not close full-runtime #1/#6. Full ARM manager execution,
combined exact-head CI, full server/manager integration and eventual device
acceptance remain distinct gates. No vehicle or public upload endpoint is used.

Docs-Not-Needed: internal native orchestration and validation driver; no user
setting or production daemon selection changes.
