# Card pumped IPC receiver stall (#330)

Issue: https://github.com/bin9208/openpilot-rust/issues/330

Source `19f10b83` failed the final pumped Volkswagen ID4 comparison in
[Card CI](https://github.com/bin9208/openpilot-rust/actions/runs/38590123444/job/115943642362).
Artifact11910765983 (`rust-card-can-host-evidence`,201137358 bytes) retains the
source/native captures,80 independent sender records, frequency traces and
raw publication metadata. Verified ZIP SHA256 is
`9fd46ab4256a0749340dc1e62e677c5c4f98033f286d337cdcc47c8980f83c91`.
Functional/platform comparisons preceding this gate
passed; the pumped assertion remains a failure.

## What the artifact establishes

The source produced80 carState/carOutput/sendcan rows. Native produced69 of
each, but the sum of native `radarInput.canPacketCount` is80: no CAN packet
loss is shown. Native output49 contains12 packets, exactly stream ticks49..60,
with first/last timestamps2533730787533/2533842313891 and receive timestamp
2533850791932. Every other native output contains one packet. Sender gaps stay
near10.1ms (maximum10.180539ms); there is no sender catch-up burst. Native
receive48→49 gaps130.069808ms, while frequency frame370→371 gaps131.961243ms.

Original `card.py` drains all queued CAN, as does native `runtime/io.rs`.
Thus one queued12-packet batch explains the eleven fewer publications under
inherited semantics. It does not establish why the receiver stalled. There are
no scheduler/syscall receipts for that CI gap. The earlier100-frame diagnostic
maximum does not cover the final partial window, so it cannot exonerate native
blocking work. Another passing capture (Ford Maverick) has concurrent sender
and receiver gaps around66/199ms, useful context but not proof of VM causality.

## Narrow diagnostic addition

Only the explicitly enabled, bounded `--frequency-trace` path gains monotonic
step-start/end, thread-CPU-start/end, existing receive and ten stage timings,
record-start, and preceding record-flush completion clocks. The normal path
does not take these extra clocks. The fields locate work versus CAN receive,
trace argument preparation, record/flush, and inter-iteration gaps. They remain
in the existing retained frequency JSONL artifact; no thresholds, CAN batching,
priorities, scheduling, delays,80-frame assertions, packet construction or
publication comparison change.

The trace also retains an optional main/current-thread schedstat tuple
`[CPU nanoseconds, runnable-wait nanoseconds, timeslice count]` (field
`schedstat_ns`; the third item is a count). The kernel enabled flag is sampled
once when opening the trace, not proof it remains unchanged. Disabled/unknown
flags and missing/malformed/oversize counter files yield null counters;
actual zero counters remain distinct. Reads are bounded to129 bytes and reject
more than128 bytes. Per-record counter IO occurs after record-start and before
the next recorded flush boundary. Runnable wait does not include every off-CPU
cause or VM steal; low CPU alone cannot distinguish blocking from descheduling.

Receive time is sampled after CAN receive/drain, not kernel packet arrival.
Step wall time includes receive waiting; stage totals exclude it, and the stage
array overlaps (aggregate state and its substages), so it must not be summed.
Record-finished means BufWriter.flush completed, not fsync/durable storage.
The inter-record interval can also include monitoring/fences and scheduling.

## Owned host investigation

Used existing Params/msgq bindings and pinned NumPy2.5.3 numerics; minimum host
Card binary build only, shared cache, two jobs, incremental disabled, capacity
checked before every build. An ignored owned monitor sampled schedstat/state/
wchan every5ms with a60s bound for source/native processes.

- Exactly one instrumented Volkswagen source/native pair passed the unchanged
  strict80-frame comparison and complete Params/field comparison. Measured
  native maximum step10.611112ms (includes CAN wait), CPU1.262658ms, trace
  preparation0.048666ms, maximum timed processing1.059ms. This passing run
  **does not fix or explain the CI stall**.
- Separate deterministic negative control stopped the owned native receiver
  for130.091397ms after stream tick45, leaving the independent10ms sender
  unchanged. It consumed80 packets into68 rows with one13-packet batch.
  The original strict80 assertion rejected it. Its trace shows step135.345468ms
  versus2.436439ms thread CPU and2266µs timed processing. Known STOP receipts
  explain this control, not the earlier CI event.
- Direct trace-schema/previous-flush clock invariant, scheduler parse/error/
  disabled/zero tests, and existing phase fence test:3 PASS. Strict Clippy for
  Card native library/bin/tests:PASS.
  A test-only JSON macro typo was corrected before the successful test run.
- After adding scheduler fields, one additional final-source focused pair
  passed the same strict80-frame/Params/field comparisons. This was justified
  by the diagnostic source change, not repeated until success. Every final
  native timing row passed monotonic/CPU ordering checks; the actual kernel
  flag was0, correctly recorded as false/null rather than zero wait. Both
  original and final focused receipts are retained. This still does not explain
  the earlier CI stall.

Ignored evidence is under `.analysis/scratch/2026-10-15-card-ipc-330/`, including
the original artifact, `focused-once/`, `stop-control/`, monitor script and
logs. Runtime stall cause remains unresolved; exact future CI diagnostics are
needed. No rerun-until-pass, fixture-rate relaxation, device access, or measured
vehicle CPU claim is made.
