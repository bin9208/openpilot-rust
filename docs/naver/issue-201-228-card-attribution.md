# Card shutdown and input attribution diagnostics (#201, #228)

Status: implementation and host regression definitions prepared; Actions execution
pending. This is diagnostic coverage, not a causal fix for historical CI stalls.
No local functional test, project import, build, CI dispatch, commit or push was
performed. Work is confined to the backlog-fixes-497 checkout and this lane.

## Evidence and scope

[#201](https://github.com/bin9208/openpilot-rust/issues/201) observed original
Python Card remaining alive after SIGINT and a completed KeyboardInterrupt
traceback. That does not establish native teardown or a stuck Params reader.
The maintained shutdown check lacked live timeout attribution before outer cleanup.

[#228](https://github.com/bin9208/openpilot-rust/issues/228) includes genuine CAN
backlogs and separate inferred control-admission overlaps. The latter comparison
precondition is already repaired by #335/PR336: complete actual topic identities
must match before outputs are compared. Its deterministic original-only proof and
exact-head acceptance are recorded in
[#335's final receipt](https://github.com/bin9208/openpilot-rust/issues/335#issuecomment-6181098888)
and `rust-card-input-admission-335.md`. Historical receiver stalls still lack a
causal scheduler/syscall explanation. #330's native wall/CPU/stage/flush diagnostics
remain available. No new CAN/control joining or runtime control policy is proposed.
The active #296/PR343 producer-stop #495 is excluded.

## Shutdown failure capture

`runtime_pumped_shutdown.finish` still observes the same empty-CAN counter/bounds,
sends the same SIGCONT then SIGINT, and waits exactly five seconds. Only a real
runtime `subprocess.TimeoutExpired` invokes attribution before existing cleanup.
The exact original exception is re-raised even if collection, imports, writes or
the collector itself fail or are interrupted. Successful shutdown and the pump's
existing five-second wait/phase transition are unchanged.

The new stdlib-only `card_qa/runtime_attribution.py` first atomically saves
`shutdown-timeout.json`: PID/source flag, original timeout, signal order, observed
frame count, last nonempty/first empty CAN identities and counters, plus counts
and four bounded summary rows per partial publication topic. Existing stdout,
stderr and frequency artifact names are linked. No successful lifecycle receipt
is written for this failure. If attribution raises, a best-effort
`shutdown-diagnostic-error.json` records that failure without replacing the timeout.

A separate owned diagnostic worker writes `shutdown-process.json` using read-only
`/proc` access. It validates the PID stat's parent against the harness, then reads
PID/TID stat, wchan, schedstat and available kernel stack files. Limits: 32 task
entries, 4096 bytes per file, 128KiB total raw stored bytes, a 0.5-second proc-read
budget and a one-second worker wait. Receipt JSON is capped at one MiB. Files are
opened nonblocking/no-follow; missing, denied, truncated, disappeared/foreign PID
and exhausted-budget cases are explicit. Failed/expired workers leave the earlier
partial receipt and their status. Ordinary receipt filesystem writes remain OS IO;
the worker timeout does not extend the successful peer shutdown allowance.

There is no extra peer signal, ptrace, GDB attach, scheduling change or stack-handler
installation. Python stacks are explicitly unavailable because this peer has no
established safe faulthandler registration/protocol; userspace stacks are likewise
unavailable without attachment. Existing stderr traceback and available kernel
stacks are retained. Only a timed-out diagnostic worker may be terminated by its
own `subprocess.run` timeout; peer cleanup remains owned by the existing harness.

## Source timing attribution

Only enabled `--frequency-trace` adds step-start/end monotonic clocks and thread
CPU clocks, record-start and previous flush-completion timestamps. `timing.receive_ns`
copies the already computed `card_diag_recv_ns`, sampled immediately after CAN
drain, and `timing.stage_us` copies the already computed named stage dictionary.
The source Car methods, Params thread and monitor/fence order are unchanged.

These fields join existing actual admitted Event IDs and receive frames with CAN
receive bounds and prior-actuator publications. They do not establish kernel packet
arrival or the reason for an off-CPU interval. Stages overlap and must not be summed.
No diagnostic value participates in admission, frequency/readiness, output equality
or retry eligibility. All measured 80 frames, CAN timestamps/counts, independent
cadence, output fields/validity and shutdown checks remain unchanged.

## Parent Actions registration and verification

Append **`tools/tests/test_card_attribution.py`** to the existing Card pytest command
inside its `working-directory: rust` lane, preserving existing test arguments/files:

```sh
python -m pytest -c /dev/null -p no:cacheprovider --confcutdir=tools/tests tools/tests/test_card_attribution.py -q
```

The new tests exercise a real owned Python child that ignores SIGINT: the original
five-second timeout, exception identity, signal order, live proc receipt and absence
of false success must all hold. Other cases cover collector/interrupt/disk failures,
worker expiry with partial evidence, unchanged successful shutdown, proc byte/task
limits, denied/missing/symlink/foreign-PID reads, and actual source-main JSON with
controlled boundary objects. The latter checks timing placement, old frequency/
admission fields and absence of added diagnostic clocks when tracing is disabled.
These definitions have not been executed locally.

For behavioral RED on the prior head, copy only the test file and select
`test_real_shutdown_timeout_keeps_original_exception_and_captures_owned_process`
and `test_source_frequency_trace_preserves_fields_and_attributes_each_actual_step`.
Both target existing entry points: missing timeout receipt and missing timing JSON
are observable failures, rather than new-helper import failures. The full proposed
test file requires the new helper. Preserve the real child cleanup in both runs.

Run the existing Card admission/phase-fence tests and the next necessary unchanged
full source/native IPC gate in Actions. Retain raw failed as well as passing evidence.
There is no standalone dispatch or repeated successful matrix requested here.
Parent owns workflow registration; trusted x64 routing follows current runner policy,
and actual ARM CPU checks remain on GitHub ARM.

Owned paths:

- `rust/tools/card_runtime_source.py`
- `rust/tools/card_qa/runtime_pumped_shutdown.py`
- `rust/tools/card_qa/runtime_attribution.py` (new stdlib helper/worker)
- `rust/tools/tests/test_card_attribution.py` (new host regressions)
- This document.

No #296 tool conversion, primary checkout, controlsd/PSA policy, shared workflow,
AGENTS, core runtime or other worker path is changed.

Static receipt (2026-10-23): four Python files pass `ast.parse` and 160-column
review; scoped whitespace checks pass for all five owned paths, including three
new files. No functional or causality claim is inferred from these checks.

The parent registered the same real owned five-second shutdown test against the
old production finish function. RED requires the exact missing-receipt assertion
after the original timeout; setup/collection failures are not defect proof. The
candidate is restored in finally before the complete GREEN tests, in both the
focused vehicle lane and the existing Card Actions job. Results remain pending.
