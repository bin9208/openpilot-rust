# Beep reference worker exception reporting (#333)

Tracking: [#333](https://github.com/bin9208/openpilot-rust/issues/333).
Source: `7dd633822fadbd241d357738c55715e75db02c8b`.
Failed [telemetry job116002110424](https://github.com/bin9208/openpilot-rust/actions/runs/38607293564/job/116002110424).

The original/native policy comparison failed at worker-on-failure. The original command4 spawn failure was already recorded; stderr showed the beginning of the worker's default exception traceback, followed by the main fixture's `worker.join(timeout=3)` / `assert not worker.is_alive()` failure. Rust continued to the next alert as expected. This evidence does not prove linecache, source lookup, stderr I/O, or scheduling caused the stall.

An owned deterministic probe executes the unchanged original `Beepd.dispatch_beep` and `engage` methods, injects a command failure, and holds `threading.excepthook` behind an event. The method has already raised, but the worker remains alive until the exception hook returns; releasing the hook permits termination. This proves that the fixture's join includes exception presentation, not that the same controlled hook or any particular formatter operation caused CI's delay.

## Scoped correction

Only `rust/tools/beep_source.py` fixture reporting changes. During policy actions a temporary worker exception hook writes the exception type, message, worker name and thread identifier to stderr without default traceback/source formatting. SystemExit remains silent, matching Python's default thread hook. The previous hook is restored even if the main fixture fails. Live mode remains outside this context, as do integer/rate paths; original Beepd and Ratekeeper methods are still compiled unchanged from their source definitions.

Worker exceptions remain visible. The hook does not catch the worker body or mark it finished. The original three-second join and real `is_alive()` assertion remain. No alert, command, sleep, volume, mutation, trace, stdout, exit-code or stderr-presence comparison changed; all20 existing policy cases remain. A genuinely running worker still fails the same join boundary. No extra timeout allowance or retry-until-pass was added.

## Evidence

- Six focused stdlib tests pass: blocked reporting keeps an already-failed original worker alive; concise reporting bypasses a controlled blocked/default formatter and keeps type/message/name/identifier visible; genuinely running workers remain alive; exact SystemExit stays silent while its subclass remains reported; the hook restores on main failure; isolated `python -I` collection succeeds with native cereal/msgq/Params imports explicitly blocked. The first boundary test executes original methods, not a replacement beep implementation.
- All20 original/native policy scenarios pass, including every alert, worker-on/off/sleep failures, startup failures, volume changes and Ratekeeper catch-up. Original traces/stdout and exit/stderr assertions remain strict. Example source stderr: `FileNotFoundError: injected command spawn failure`, with engage worker identity.
- Real Cython/native integer conversion:278cases pass, including every leading byte and fatal ranges. Monotonic conversion:1011cases pass; sleep conversion:27cases pass.
- Targeted Ruff Python3.12 and `git diff --check` pass.

Canonical environment was sourced, then PYTHONPATH explicitly pointed at this runtime worktree and its existing original Params/msgq bindings under `.analysis/scratch/2026-10-14-generation-287/`. The missing host example was built with incremental0/jobs2 after a capacity guard reportedabout369GiB free against26GiB required; focused `cargo build --locked -p openpilot-beepd --example beep_trace` completed in1.56s. No other Cargo ran concurrently. No production/runtime source changed. The telemetry job now executes the six stdlib regressions before its existing policy comparisons; the complete workspace Python suite also discovers them.

Reproduction commands:

```sh
python -P -m unittest discover -s rust/tools/tests -p test_beep_worker_report.py -v
python -P rust/tools/check_beepd_reference.py --binary BEEP_TRACE --binding PARAMS_BINDING --output OUT/reference
python -P rust/tools/check_beepd_integer.py --binary BEEP_TRACE --binding PARAMS_BINDING --output OUT/integer
python -P rust/tools/check_beepd_clock.py --binary BEEP_TRACE --output OUT/clock
python -P rust/tools/check_beepd_sleep.py --binary BEEP_TRACE --output OUT/sleep
```

Ignored receipts/probe: `.analysis/scratch/2026-10-15-beep-worker-report/`.
Host beep_trace SHA256: `f0ea17c31feaefe2da27146a0a12045e67e9d5981601a5789d84a05c04143f0d`.
Unchanged original beep.py SHA256: `6b0427638dd6cb2279d87e6456eaefaedf2992b09ca317c1d1b7c370cd2060f5`.
Unchanged original realtime.py SHA256: `6337d64508299a53c00f0795397e5043ec7b69c864d6ca8c02604244502aba5e`.
Unchanged comparison script SHA256: `051315d756ee2afd43e94051067283c597c2fbcf83d48d10721c62c591797177`.
Fixture helper SHA256: `ef4a4c7350eb36748eb3a1bb2abca0e3a166dc8103772d795398da95a6baa7bb`.

Fresh exact-SHA CI remains required. This is fixture exception presentation evidence, not a runtime beep fix, device validation or CPU improvement.

The final helper-only edit after the full comparisons clarified its module docstring; executable code did not change. Six stdlib and focused pytest regressions were rerun afterward and pass.

Final combined local validation:202 complete Python workspace tests and9
subtests pass with zero skips (50.21s), using the pinned original model/QCOM
compiler environment. Existing CI isolation/native guard policy:37tests and367
subtests pass. Independent helper/test readback found no Important/Critical
issue. The workflow now runs the six source-worker regressions explicitly in
the telemetry gate; fresh exact-head CI remains required.
