# Jetlink socket fixture deadline isolation

Issue [#72](https://github.com/bin9208/openpilot-rust/issues/72), under
[#56](https://github.com/bin9208/openpilot-rust/issues/56).
Baseline: `b3dc34f39bf4aa81be9a73af5f81b9ab1047f8ac`.
The [original ASan run](https://github.com/bin9208/openpilot-rust/actions/runs/36694692175)
failed `malformed_generation_does_not_destroy_idle_owner` at `socket.rs:131`:
the valid recovery inference returned `Error::Deadline`. The preceding
FunctionFS and runtime suites passed; no ASan memory violation was reported.
This is separate from FunctionFS response preparation (#58) and runtime fixture
isolation (#63).

## Reproduction and diagnosis

The unchanged original ASan selection passed locally. A fixed matrix of the
frozen, uninstrumented socket binary passed 20/20 single-CPU/eight-thread runs,
20/20 single-CPU/one-thread runs, and five isolated recovery runs. Two CPUs with
eight threads produced one native socket inference deadline failure in 20 runs.

A second, predeclared matrix added one SHA256 CPU contender on the same CPU.
Eight parallel test threads failed 11/12 complete suite runs, including the exact
CI recovery assertion. One test thread passed all 12 runs with the same CPU load.
These are complete fixed-size matrices, not retries until a passing result.

Temporary test-only timestamps showed that recovery reached the synthetic
backend with more than 40 ms remaining. Synthetic output allocation took about
0.74–0.88 ms. Parallel recovery elapsed times were 47.65–59.28 ms; failed recovery
still observed `ready=true` and an empty owner error. Serial recovery under the
same contender took 33.33–39.45 ms and returned all 18,452 floats in all 12 runs.
This supports CPU contention between independent socket scenarios, rather than
an owner poisoned by generation rejection or dominant synthetic allocation cost.
A second instrumentation pass measured actual production owner response
encoding at 35.49–41.49 ms with parallel fixtures versus 24.51–25.15 ms with
serial fixtures, under the same ASan/CPU contender. Proxy output decoding took
3.66–9.00 ms. Rejection occurred before the backend call, and the valid backend
call began with 44–49 ms remaining. These are instrumented host stage costs,
not device performance results. All instrumentation was removed before final
validation of the correction.

The controlled contention reproduces the failure mechanism, not the original
GitHub runner's exact scheduling or host load.

## Correction and preserved coverage

A test-local mutex serializes the eight independent socket scenarios. Each
scenario acquires its guard before setup and retains it through local cleanup.
Its actual Unix socket client and owner/peer threads still execute concurrently.
No production source, workflow, deadline, fixture input or output assertion is
changed. Generation rejection followed by valid recovery, current frame output,
reordered frame rejection, stale reply rejection, partial packet deadlines,
oversize framing, saturated listener bounds and owner shutdown checks remain.
The production 50 ms inference budget and all negative deadline tests are intact.

## Validation and evidence

The corrected uninstrumented binary passes all 24 fixed contender runs. A later
CPU 0/1 matrix still had nine deadline failures, including with one test thread;
mutex poisoning also made subsequent tests fail. These failures are retained.
A fresh matched comparison on CPUs 24/25, while cooperating validation avoided
those CPUs, passed all 65 corrected no-contender invocations and all 24 corrected
contender invocations. The original binary passed all 65 no-contender invocations
but failed all 12 parallel contender invocations; its 12 serial contender runs
passed. The earlier failures are not evidence that the correction can withstand
arbitrary host scheduling. The complete original ASan selection is:

```sh
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 \
RUSTFLAGS='-Zsanitizer=address -Clink-arg=-Wl,--export-dynamic' \
/home/bin9/.cargo/bin/cargo +nightly-2026-09-29 test \
  --manifest-path rust/Cargo.toml -p openpilot-jetlink \
  --lib --test socket --test runtime --test ffs --locked \
  -Zbuild-std --target x86_64-unknown-linux-gnu
```

Native all-target Jetlink tests, warnings-denied Clippy, focused formatting and
diff checks are recorded with commands and exit codes. Local evidence lives in
the assigned issue-72 worktree at `.omo/evidence/jetlink-socket/INDEX.json`:
frozen before/after/profile binaries, source snapshots, per-run logs, exact
commands and SHA-256 hashes. The profiles are diagnostic evidence only.

Exact-commit Actions and parent integration remain required before closing #72.
Isolation cannot prevent arbitrary unrelated host load from exhausting a real
wall-clock deadline. No device, vehicle, C3X, production performance or complete
runtime acceptance is established by these host tests.

Docs-Not-Needed: test scheduling and engineering evidence only; no user-visible
behavior or setting changes.
