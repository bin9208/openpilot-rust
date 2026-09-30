# Jetlink FunctionFS sanitizer fixture timing

Issue [#58](https://github.com/bin9208/openpilot-rust/issues/58), under
[#56](https://github.com/bin9208/openpilot-rust/issues/56), addresses the actual
[PR #57 ASan failure](https://github.com/bin9208/openpilot-rust/actions/runs/36679307435/job/109771060569).
The failing base is `384e8098a2c3e628c780fca4f9cb74e6e70aff67`.

`native_functionfs_client_preserves_original_wire_and_model_contract` returned
`Error::Deadline` at `ffs.rs:169` with its existing 50 ms inference deadline.
The seven preceding library tests and the other two FunctionFS tests passed.
This was a deadline assertion failure; the captured log does not report an ASan
memory violation.

## Diagnosis and correction

The synthetic peer built all 18,452 output floats element by element after
receiving the inference request. That fixture work consumed the same deadline as
the real FunctionFS transport/client. It also allocated the expected warped-input
vector during the request assertion.

The exact sanitizer command passed on the local host without added contention.
Test-only phase instrumentation then measured these ranges over a predeclared
12-run matrix for each condition:

| ASan fixture | CPU condition | Inference duration | Synthetic response encoding |
| --- | --- | --- | --- |
| Before | Idle | 30.705–37.074 ms | 13.087–17.675 ms, inside deadline |
| Before | One same-core contender | 51.426–59.729 ms | 25.129–31.154 ms, inside deadline |
| After | Idle | 16.387–18.712 ms | 12.853–13.419 ms, before deadline |
| After | One same-core contender | 28.713–34.792 ms | 24.832–28.869 ms, before deadline |

Independent uninstrumented binaries reproduced the same outcome in serial runs:
original idle 12/12 pass, original contended 0/12 pass; corrected idle and contended
both 12/12 pass. The CPU contender is a controlled local reproducer, not a claim
about the original CI host's scheduling. An accidentally overlapping pair of
profiling runs is retained but excluded because their CPU loads were not isolated.

The correction prepares the expected warped bytes and synthetic output payload
before spawning the peer. The peer still frames its response with the received
sequence, uses real FIFOs and the production transport/client, and checks every
original request field, input byte, float, returned output and descriptor.
The 50 ms deadline and the independent 10 ms blocked-reader deadline test remain
unchanged. No production transport, timeout, scheduler or workflow was edited.
The measurements establish a fixture timing hazard; they do not establish a
production device-performance defect or device acceptance.

## Validation and evidence

The full failing ASan command is rerun after the correction:

```sh
CARGO_BUILD_JOBS=2 \
RUSTFLAGS='-Zsanitizer=address -Clink-arg=-Wl,--export-dynamic' \
cargo +nightly-2026-09-29 test --manifest-path rust/Cargo.toml \
  -p openpilot-jetlink --lib --test socket --test runtime --test ffs --locked \
  -Zbuild-std --target x86_64-unknown-linux-gnu
```

Native all-target Jetlink tests, warnings-denied Clippy, formatting, diff and mapped
user-doc checks are also recorded. Frozen before/after test binaries, instrumented
source snapshots, per-run logs, timings and the original CI log are indexed at
`.omo/evidence/jetlink-asan/evidence.json` in the issue-58 worktree. The index records
actual exit codes, complete commands and content hashes. Profiling instrumentation
is evidence-only and is absent from the committed test. Exact-SHA cloud validation
remains part of the parent integration gate.

Docs-Not-Needed: test-fixture preparation only; no user-visible behavior or settings.
