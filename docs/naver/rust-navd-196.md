# Native navigation runtime: issue 196

The navigation daemon and destination command have a native Rust implementation
with host and ARM source comparisons, including native Rust IPC and the extracted
AGNOS loader. Exact-head CI and full startup remain integration gates. This is one
stage of [issue 1](https://github.com/bin9208/openpilot-rust/issues/1),
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

After merging native IPC commit `f148b6b6`, the composed source is
`340cc0b42ff7b214842b460289a013eda6fbcb70`. The host daemon in `native-v5/`,
SHA-256 `f09e736d3b976eb439990a9a26988d8a0226f97496ada15aa6320bda2bc23a54`,
passes all five continuous IPC scenarios in `native-ipc-host/report.json`.
Historical `native-v4/` host comparisons above retain their CXX dependency
boundary; they are not relabeled as native IPC results.

All six commands/probes were built in the release profile for GNU aarch64 with
GNU BFD and frozen in `arm-v1/frozen/`. `arm-v1/receipt.json` records their
source revision, current Cargo artifact selection and hashes. Under QEMU and
the GNU sysroot, the same complete comparison suites pass: 527 policy cases,
15 engine scenarios/76 steps, 36 HTTP cases including actual timeout behavior,
26 authentication cases, 75 destination executions and five IPC scenarios.
No floating-point tolerance was introduced for ARM. Checkers accept an explicit
runner prefix while retaining the same comparison rules and native executable
identity checks through the process mapping.

The frozen ARM daemon, SHA-256
`804c8b9fccae56e05c95e4bf27f673858216547478f261b4a62dc40eb7433201`,
also starts through the loader and libraries extracted from the pinned
AGNOS 19.8-carrot-bt1 image and passes all five actual IPC scenarios.
`arm-v1/agnos-help.json` and `agnos-ipc/report.json` retain those results.
These are emulated user-space/loader checks, not an execution on vehicle
hardware. Runtime diagnostics still depend on external ZeroMQ and its C++
standard library; the project-owned msgq implementation is Rust.

The required `rust navigation runtime` job runs the host source and actual
transport checks; `rust aarch64 build` includes both native commands and probes.
Their final revision results are still required. Other native IPC consumers
continue through their separate composition checks.
Linux facilities, ZeroMQ diagnostics, routing servers and TLS roots remain
external dependencies. Complete manager startup, existing log upload and the
user's first device comparison remain outstanding under issue 1. No CPU savings
or complete Rust-runtime claim follows from this component's host tests.
