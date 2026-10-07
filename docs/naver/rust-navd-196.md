# Native navigation runtime: issue 196

The navigation daemon and destination command have a native Rust implementation
with host source comparisons. ARM, composed IPC, exact-head CI and full startup
remain integration gates. This is one stage of [issue 1](https://github.com/bin9208/openpilot-rust/issues/1),
tracked in [issue 196](https://github.com/bin9208/openpilot-rust/issues/196).
Production process selection is unchanged. No vehicle was connected or tested.

## Source and runtime boundary

`rust/crates/navd` ports `openpilot/selfdrive/navd/navd.py`, `helpers.py` and
`set_destination.py`. Original MIT provenance remains in `NOTICE`; the
compensated sum's CPython license is retained in `CPYTHON-LICENSE`. Rust-owned
runtime code does not execute Python. The existing Statsd float parser is moved
unchanged into `runtime-core::python_float` for both callers, including Unicode
decimal digits, underscore syntax, signed zero and nonfinite spelling.

The route engine retains source geometry operation order, nearest-segment and
short-geometry behavior, even-tie route-point selection, maneuver/banner/lane
selection, overlapping speed annotations, partial response state, reroute
backoff and destination/waypoint cleanup. It uses the full cereal schema and
the original `carrotMan`, `managerState`, `navInstruction` and `navRouteNavd`
services. Float32 GPS fields are promoted as in the Python reader.

The native loop retains the lazy first deadline and subsequent one-second
Ratekeeper cadence. UI PID changes schedule a five-second route resend. A
dedicated publisher owns the non-Send PubMaster and serves the current route
snapshot even while the route request blocks. Pending timers finish during
shutdown, matching Python's non-daemon Timer lifetime. SIGINT interrupts sleep
or an HTTP wait; SIGTERM retains process-default termination. Bounded `--frames`,
owned `--mapbox-host` and `--persist-root` options support isolated checks.

The HTTP path uses the pinned Requests headers, per-socket ten-second timeout,
thirty redirects, path/secure cookies, compression and response text/JSON
encoding rules. A progressing body may exceed ten seconds overall. Authentication
retains environment-token precedence, public-key selection for PrimeType zero,
and the original RSA/EC key order and four-week claims. With no complete key pair,
the repository's locked PyJWT 2.14.0 produces an `alg=none` token; the Rust client
preserves that explicit source case. This is not a verified credential or a
claim that the remote service accepts it. Actual RSA/EC test tokens are verified
with their synthetic public keys. No remote routing service was contacted.

`openpilot-set-destination` follows the source URL splitting, float conversion,
default destination/waypoint, write order and console output. Params write/remove
I/O statuses are ignored where the original Cython wrapper ignores them; key
validation errors still propagate. This matters for repeated successful route
calculations after the waypoint key has already been removed.

## Observed host verification

Final executables are frozen in `.omo/evidence/navd-196/native-v4/`; their hashes
are in `SHA256SUMS.json`. Source comparison reports are under `final-host/`.

| Check | Observed result |
| --- | --- |
| Geometry/banner policy | 527 cases match exactly; no numeric tolerance. |
| Mutable route engine | 15 scenarios, 76 steps match source state, Params effects, request URLs, decoded cereal values and diagnostic categories. |
| Actual loopback HTTP | 36 cases match, including redirects, cookie paths, compression, encodings, errors, ten-second stalls and an eleven-second progressing response. |
| Authentication configuration | 26 source/native cases match; RSA/EC signatures and claims verified, no-key tokens distinguished, invalid integer fatal outcomes recorded. |
| Destination command | 75 actual source/native executions match exit status, stdout and raw Params bytes, including directory/write failures. |
| Actual continuous IPC | Five source/native scenarios match selected full payloads and Params: blocked-request timer, latest-route timer, cleared-route timer, SIGINT during HTTP and SIGTERM. |
| Rust checks | Nine Navd tests pass; affected Runtime-core/Statsd tests and all-target Clippy pass. |
| Shared number parser | 598 original Statsd aggregation cases pass after the move. |
| CI policy | 20 checks/223 subtests and five Card routing checks pass with the new required navigation job. |

The source IPC runner compiles the unchanged RouteEngine, main loop and
Ratekeeper definitions from their original files. It uses actual original
message/Params extensions, real socket requests and real Python timers; only
filesystem paths, endpoint and process title are fixture adapters. The unused
CarrotMan import is not executed. Original extension paths/hashes, process maps,
raw wire bytes, requests, stdout/stderr, return codes and comparison reports are
retained. Cross-process clock values and nondeterministic publication counts are
not asserted equal; selected message data and validity are compared exactly.

Earlier failures remain separate evidence. `ipc-v1` exposed a fixture assumption
that the existing reroute countdown had already elapsed; the checker now waits
for the actual additional source iteration before scheduling the timed case.
`ipc-v2` then reproduced the real Rust ENOENT propagation bug after removing
waypoints twice. `ipc-v3` passes after matching the Cython return-status boundary.
The expanded final IPC suite additionally verifies latest/cleared timer data.
`config-v2-red` records the two actual no-key source/native mismatches before
repair. The earlier config fixture's too-long Unix logging endpoint is retained
as a fixture failure, not a production authentication failure.

New Python checkers pass Ruff. The existing CI-isolation test file retains three
pre-existing Ruff findings; the untouched HEAD reproduces them in
`ci-ruff-baseline.log`. No lint rule was suppressed. Builds and executable copies
checked available space first, retained the 25 GiB floor plus estimated growth,
used two build jobs and disabled incremental compilation.

## Remaining integration

The 2026-10-07 dev integration candidate reuses the completed module on the
native Rust IPC transport. All five existing real loopback HTTP/IPC scenarios
pass again: route lifecycle, latest-route timer, cleared-route timer, HTTP
interruption and termination. Build, formatting and CI policy checks pass
(21 tests, 225 subtests). The first build identified two missing public export
lines for the existing registration `response_text` decoder; those lines now
match the preserved integration candidate without changing decoding behavior.
The executed navd SHA-256 is
`680f2e651b86123e2188bdec51954783286562a1e02181d22f6603258fcdc4d0`.
The executable and receipt are retained under
`.analysis/archive/2026-10-07-navd/`. Prior full policy/HTTP/authentication and
destination results are reused; they were not rerun locally.

The required `rust navigation runtime` job runs the host source and actual
transport checks; `rust aarch64 build` includes both native commands and probes.
Their final revision results are still required. The earlier independent host
receipts used the CXX transport; the five new composition scenarios above use
native Rust IPC. Historical receipts retain their actual dependency boundary.
Linux facilities, ZeroMQ diagnostics, routing servers and TLS roots remain
external dependencies. Complete manager startup, existing log upload and the
user's first device comparison remain outstanding under issue 1. No CPU savings
or complete Rust-runtime claim follows from this component's host tests.
