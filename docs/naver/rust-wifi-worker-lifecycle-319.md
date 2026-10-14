# Wi-Fi essential worker ownership — #319

Tracking: [#319](https://github.com/bin9208/openpilot-rust/issues/319).

Previously, initialization started the scanner and signal monitor, awaited
connection/hotspot setup, and then logged completed worker errors while waiting
for the remaining workers. A failed monitor could therefore remain hidden behind
a healthy infinite scanner or pending setup call. The outer manager also treated
initialization failure as an ordinary failed user command and kept accepting work.

Initialization now supervises setup and essential workers concurrently. A worker
error preserves its original error; a panic or unexpected successful exit is also
a failure. Before returning, it aborts and drains every surviving sibling. The
manager separately owns initialization, so its failure ends the owner rather than
being swallowed among command results. Existing command failures remain nonfatal.
An internal shutdown notification lets requested stop or D-Bus transport failure
cancel adapter discovery, setup, scanner and monitor before the owner joins its
children and closes its connections. No network reconnection, credential,
scan-cadence or generic D-Bus deadline policy was introduced.

The original Python Wi-Fi implementation owns separate daemon initialization,
scanner and monitor threads. This repair deliberately strengthens failure
ownership; healthy connection/settings behavior must still match that source.
Existing setup warnings retain their previous handling.

Validation on 2026-10-14:

- Three deterministic native lifecycle tests first failed against the old
  join-and-ignore supervision behavior. After the repair they pass for monitor
  failure with a live scanner after setup starts, both completed and pending
  setup, unexpected success, panic, and requested stop with two healthy workers.
  Oneshots establish worker/setup readiness; drop receipts must already be ready
  when supervision returns, proving sibling cleanup rather than merely scheduling
  cancellation. No timing sleeps establish ordering.
- The complete Wi-Fi package passes all seven tests, including four unchanged
  stale-auth/connection-epoch/forget/scanner-boundary regressions.
- Strict all-target Wi-Fi Clippy passes. The canonical coordinated cache used
  incremental compilation disabled and two jobs, with more than 374 GiB free
  before each build.
- The existing original/native policy comparison passes 781 cases and 2,322
  frames exactly. The unchanged private D-Bus comparison passes both existing
  hotspot and new-hotspot scenarios, including methods, settings, signals,
  snapshots and cleanup. No original fixture assertions were weakened.
- The unchanged callback-ordering/private-D-Bus comparison passes with exact
  seven-event sequences and two B-forgotten callbacks. It observes profile
  removal before gate entry, a blocked callback, then explicit release before
  comparing final completion.

Ignored local evidence is under
`.analysis/scratch/2026-10-14-wifi-lifecycle-319/`. These are host task ownership,
policy and private D-Bus checks, not device Wi-Fi or AGNOS update acceptance.

Docs-Not-Needed: internal worker lifecycle repair without settings changes.
