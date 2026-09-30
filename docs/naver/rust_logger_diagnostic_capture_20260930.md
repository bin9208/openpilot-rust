# Sequential logger diagnostic oracle (#69)

At `f90ebd58b69ca641d7cfef5b65a7ccd8338951a7`, the
[push run](https://github.com/bin9208/openpilot-rust/actions/runs/36688919890)
passed ASAN media/data assertions but its video diagnostic comparison found 109
Rust records versus 110 source records. Only `qRoadEncodeData: has encoderd offset
29` was absent. The [same-head PR run](https://github.com/bin9208/openpilot-rust/actions/runs/36688926163)
passed. Both downloaded artifact ZIPs match their GitHub SHA256 digests. Inspection
of raw packet hex confirms the missing record is not a JSON comparison artifact.

## Mechanism

The fixture's `Peer.send` previously waited for msgq's read pointer. Native msgq
advances that pointer before returning the message for processing. The next
fixture input therefore could send SIGUSR2 to the logger while it was emitting
the preceding input's diagnostic. Both original C++ cloudlog and the Rust native
logging producer use best-effort nonblocking ZMQ sends; both drop records when
a send is interrupted rather than retrying them.

Actual source reproduction, with a read-only `zmq_send` tracing interposer and
SIGUSR2 stress, lost the exact qRoad offset record: the system ZMQ call returned
`-1`, `errno=4` (EINTR). An instrumented ASAN Rust logger independently lost an
offset diagnostic with `Err(Transport(Interrupted system call))`. The collector
captured all successful records in these reproductions. Original CI did not retain
sender errno; these reproductions establish the failure mechanism without claiming
an errno measurement from that CI process. Temporary Rust instrumentation was
removed and the clean logger rebuilt before final validation.

A deterministic regression uses actual native msgq and a worker that processes one
received packet for 100 ms. Before correction, `Peer.send` returned about 1 ms after
the read acknowledgement, roughly 99 ms before processing finished. After correction,
it returns after the recorded completion timestamp. The transferred packet SHA256
is identical. Both timelines and process invocations are retained.

## Fixture correction

After the existing read acknowledgement, sequential `Peer.send` waits until the
logger main thread is back in the native msgq poller's `hrtimer_nanosleep` wait.
This is a Linux `/proc/<pid>/wchan` observation with a bounded timeout and a process
exit check, not a fixed delay. The regression runs automatically at the start of
`check_loggerd_native.py`.

The runtime, logging transport, send/drop policy, receiver queue size and exact
ordered diagnostic comparison are unchanged. Burst and fairness still publish
their complete queues directly while the logger is SIGSTOPped; they bypass the
sequential helper and retain their original backlog/order assertions. Encoder
queue overflow/restart and audio queue assertions also remain unchanged.

## Verification

- The full original/normal and original/ASAN suites pass: ordinary data, 1,000-message
  burst, fairness, video, audio, audio queue, encoder queue/restart, edge cases and
  HEVC. Exact ordered level/message/context/callsite diagnostic checks pass.
- Normal real-time fallback checks still enforce their original 60/72-second
  thresholds. No input or runtime timing threshold is relaxed.
- Comparing the seven principal scenarios against the failed CI artifact verifies
  13,912 scenario packets across 28 source/native suite comparisons retain
  exact service, order and bytes. Only existing `logger-qa-barrier` timestamps are
  excluded from this cross-run byte check.
- Both 10,000-message throughput/counter cases and diagnostic suppression checks
  pass. Measured fixture rates are not device performance claims.
- Native raw/remux write-error checks and ASAN remux ownership checks pass.
- Focused Python lint and the recorded red/green processing-completion regression
  pass. No production source, workflow, uploader, shared logging or #61 edits are
  included.

Evidence: `.omo/evidence/logger-diagnostics/evidence.json` in the issue worktree.
It records CI ZIP digests, actual packet captures, before/after timelines, sender
EINTR traces, original/native/ASAN executable hashes, source hashes and invocations.
Frozen #68 evidence is preserved. No device access or push occurred. Parent
integration owns the next exact-SHA cloud run and the full-runtime device gate.
