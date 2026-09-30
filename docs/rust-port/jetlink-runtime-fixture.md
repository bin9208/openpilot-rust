# Jetlink runtime fixture isolation

Issue: [#63](https://github.com/bin9208/openpilot-rust/issues/63).
Baseline: `e311bf36e5029af44737c5522e353e16e276463d`.
The independent [PR #57 ASan failure](https://github.com/bin9208/openpilot-rust/actions/runs/36682041920)
selected native `7.0` instead of external `2.0` in the active runtime test.
The preceding FunctionFS fixture tests passed; this is separate from #58.

## Diagnosis

The exact unmodified ASan command passed on this host. A fixed matrix then
compared the same binary with one/four test threads and one/two allowed CPUs.
Test-only timestamps captured native fixture construction, entry to the actual
backend seam, completion and Runtime status. They were removed from the patch.
The normal native fixture preparation took roughly 2–9 ms; the complete external
path took roughly 20–24 ms when isolated. Synthetic output preparation was small,
and the peer normally received the request within about 1–2 ms.

Forcing four independent fixtures to run on one CPU reproduced the active source
assertion failure in 12/20 instrumented runs. The same binary with one test thread
passed 20/20 runs. Four threads on two CPUs passed but consumed up to 47.7 ms.
The observed failed status was `Native/Lost`, with the loss latch set and a
deadline/socket timeout detail. This establishes a reproducible contention
mechanism; it does not reconstruct the original CI runner's precise scheduling.

After removing all instrumentation, the original test binary failed 6/20 fixed
one-CPU/four-thread runs with exactly the CI assertion (`7.0 != 2.0`), versus
0/20 failures with one test thread. These are complete fixed-size comparisons,
not retries that stop at the first passing run.

## Change

A test-local mutex serializes independent `Fixture` lifetimes. Acquisition occurs
before any fixture setup; the guard is the final struct field so it remains held
until the real server and runtime workers have been dropped. Each scenario still
runs its actual concurrent owner/RPC workers and original Unix socket transport.

No production code, workflow, fixture data or inference ordering changes. Native
prediction construction remains between `begin` and `finish`. All active source,
frame identity, activation reset, validation rejection, shadow nonblocking,
timeout, stale-publication and loss-latch assertions remain unchanged. The
production 50 ms deadline remains unchanged.

## Validation and limits

Local artifacts are in the isolated worktree's
`.omo/evidence/jetlink-runtime/INDEX.json`. The ledger records exact commands,
exit statuses, SHA-256 hashes and every nonempty log. Key scenarios:

- Original uninstrumented failure: `red.json`, `red-threads4-*.log`; four tests
  per invocation, six active-selection failures among 20 fixed runs.
- Corrected constrained runtime: `green.json`, `green-cpu{1,2}-*.log`; four
  harness threads, 20 fixed runs per CPU count, all four tests pass in all 40 runs.
- Full original ASan selection: lib/socket/runtime/ffs; no sanitizer report.
- Ordinary complete Jetlink crate tests, Clippy with warnings denied, formatting
  and diff checks.

ASan invocation (from the worktree root):

```sh
CARGO_BUILD_JOBS=2 RUSTFLAGS='-Zsanitizer=address -Clink-arg=-Wl,--export-dynamic' \
  cargo +nightly-2026-09-29 test --manifest-path rust/Cargo.toml \
  -p openpilot-jetlink --lib --test socket --test runtime --test ffs \
  --locked -Zbuild-std --target x86_64-unknown-linux-gnu
```

The fixed comparison executes the resulting `runtime-*` binary with
`taskset -c <allowed CPU list> ... --test-threads=1` or `--test-threads=4`.
The patch does not establish performance or acceptance on a vehicle, and it
cannot prevent unrelated host load from exhausting a genuine wall-clock deadline.
Exact-commit Actions integration remains required before closing #63.

Docs-Not-Needed: only test scheduling and engineering evidence change; no user
setting, runtime behavior or experimental CLI behavior changes.
