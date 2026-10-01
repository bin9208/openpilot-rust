# Wi-Fi lifecycle callback completion fixture (#174)

Issue: [#174](https://github.com/bin9208/openpilot-rust/issues/174), found while
validating [PR #167](https://github.com/bin9208/openpilot-rust/pull/167) at
`5fa3c01da74bcc9bccd3762a92e2965d8351c115`.
The downloaded artifact from [push run36866904799, job110385311483](https://github.com/bin9208/openpilot-rust/actions/runs/36866904799/job/110385311483)
has equal source/native snapshots and D-Bus mutations, with six source events
and seven native events. The only difference is the final `Forgotten: B` callback.
An earlier `Forgotten: B` already appears during connection, so event membership
alone cannot identify final-operation completion.

The original `WifiManager.forget_connection` worker waits for its D-Bus Delete
reply and then enqueues the callback. Its monitor processes ConnectionRemoved
independently and can remove B from the saved-network list before callback
enqueue. The fixture previously stopped as soon as B disappeared, discarding the
pending source callback from its collected event stream. This was a test
completion boundary defect, not an observed source/native policy difference.

The fix snapshots the count of `Forgotten: B` callbacks before the final Forget
command and waits for both saved-network removal and a count increase. Exact
snapshot, mutation and ordered event comparisons remain intact. The existing
polling deadline is unchanged. Native and original production Wi-Fi policy,
profile selection, real NetworkManager, devices and services are untouched.

A test-only source-peer gate can pause only that final callback immediately
before invoking the original `_enqueue_callbacks`. The focused checker releases
it after observing saved-network removal and verifies the original callback is
then delivered. Earlier connection-time callbacks remain ungated. The gate is
opt-in through the owned checker protocol; normal lifecycle QA uses no gate.

## Reproduction and validation

Local evidence base:
`/home/bin9/openpilot-rust/.omo/evidence/wifi-callback-174/`.
The existing Python runtime and native binaries were reused without builds or
installs. The focused invocation is:

```sh
PYTHONPATH=.:rust/tools \
  /home/bin9/.cache/uv/environments-v2/check-wifi-runtime-0dcdad22ff3f28ad/bin/python \
  rust/tools/check_wifi_callback_ordering.py \
  --binary /home/bin9/openpilot-rust/.analysis/scratch/2026-09-30-rust-telemetry-integration/worktree/rust/target/debug/examples/wifi_native \
  --launcher /home/bin9/openpilot-rust/.analysis/scratch/2026-09-30-rust-telemetry-integration/worktree/rust/target/debug/openpilot-process-child \
  --binding /home/bin9/openpilot-rust/.analysis/scratch/2026-09-30-rust-startup-integration/combined-inputs-3/startup-params-binding/params_pyx.cpython-312-x86_64-linux-gnu.so \
  --output /home/bin9/openpilot-rust/.omo/evidence/wifi-callback-174/green-gated-final
```

Use a fresh output directory. Replacing the script name with
`rust/tools/check_wifi_runtime.py` and using a fresh output directory runs the
complete existing/create lifecycle suite without instrumentation.

| Criterion | Exact scenario and invocation | Binary observable | Captured artifact under evidence base |
| --- | --- | --- | --- |
| Controlled failure reproduces CI difference | Original lifecycle checker with `--defer-final-forgotten`, prior completion predicate | Exit1; snapshots/mutations equal; source6 events, native7; source gate blocked, B absent, count1 | `red-2.log`, `red-2/source/existing/{result,callback-gate,calls,execution}.json`, `red-2/native/existing/result.json` |
| Focused regression fails without the fix | Focused invocation above, temporarily restore only prior completion predicate | Exit1 with source/native callback completion mismatch; fixed bytes restored in finally | `red-focused.log`, `red-focused/source/existing/result.json`, `red-focused/native/existing/result.json` |
| Fixed completion waits for the new callback | Focused invocation above with fixed predicate | Exit0; exact full lifecycle equality, source/native7 events, exactly2 ForgottenB callbacks; blocked gap observed before release | `green-gated-final.log`, `green-gated-final/comparison.json`, `green-gated-final/source/existing/callback-gate.json`, both `result.json` files |
| Original full lifecycle stays exact without the gate | Original checker with the same binary/launcher/binding, fresh `green-original` output; no defer flag | Exit0; existing/create source/native private D-Bus snapshots, methods, settings, signals and cleanup match exactly | `green-original.log`, `green-original/comparison.json`, both sides' existing/create result/calls/execution files |
| Fixture syntax/lint and owned scope | Ruff on both checkers and the fixture directory; AST compilation of modified modules; `git diff --check` | Each exit0; no production files changed | `ruff.log`, `syntax.json`, `diff-check.txt`, `validation.json`, `receipt.log` |

The focused comparison records source/checker/native/launcher/binding hashes.
The original CI capture is retained in `ci-original/`; the initial local missing
PYTHONPATH setup failure is retained separately as `red.log` and is not counted
as the scheduling reproduction. The debugging journal records the hypothesis and
red/green toggle. Root owns adding the focused invocation to the Rust workflow
and running the subsequent exact-SHA cloud checks; no new CI success is claimed
by these local checks. Device and whole-runtime acceptance remain separate.

Docs-Not-Needed: this change repairs an owned test fixture completion boundary;
it adds no user-visible setting and changes no runtime setting behavior.
