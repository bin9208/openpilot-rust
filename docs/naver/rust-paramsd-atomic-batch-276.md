# Paramsd fixture atomic input batches (#276)

Issue: https://github.com/bin9208/openpilot-rust/issues/276

The [estimation job](https://github.com/bin9208/openpilot-rust/actions/runs/38375035037/job/115221162313)
for PR #275 failed the unchanged persistence assertion: publication 1199 was
expected, while saved bytes matched publication 1198. Its startup observer
confirmed one completed empty wait. All 1202 numerical event comparisons passed,
and the largest interval between publications was 6.903 ms. This is separate
from the initial startup-phase defect fixed in [#250](rust-paramsd-startup-phase-250.md).

The fixture sends each input batch while the daemon is running. Both original
and Rust msgq publishers expose a wrap cursor before committing the next
payload. The 256000-byte livePose queue wraps at input index 1103 for the
fixture's 224-byte packets. A reader awakened by the preceding non-polled
services can observe that intermediate cursor, consume the wrap marker without
a pose, and complete an additional SubMaster update. A long producer gap is
not required. Later preload wait returns remain diagnostic information, not
an inferred update counter.

An owned debugger hardware watchpoint held the actual current peer immediately
after its wrap cursor became 4294967296. Before the fix, the native reader also
advanced to 4294967296. Direct observation of the unchanged executable's
State::frame accessor showed frames 1103 then 1105 across publications 1102
and 1103. All 1202 source comparisons passed before the strict saved-byte
assertion failed at publication 1198.

The fixture now gates every native input wait after the required initial empty
update. It publishes a complete batch while the reader is held, computes the
expected committed pose cursor from the real queue capacity and packet length,
and verifies that exact cursor before releasing the reader. An intermediate
wrap cursor cannot release it. The final barrier is released before exercising
the daemon's real SIGINT or SIGTERM shutdown. No daemon, msgq, estimator,
100 ms timeout, numerical tolerance or persistence policy changed.

The same forced wrap passed after the fix: the writer exposed 4294967296 while
the held reader remained at 255896. All 1202 directly observed publication
frames were consecutive, every source comparison passed, and saved bytes
matched publication 1199. The normal 1202-input run and eight cached restart
comparisons also passed, including external/internal GPS selection and both
shutdown signals. Five focused tests cover normal commits, queue wrap,
cycle wrap, a header-only transition and an incorrect payload cursor.

A wrong expected pose length was injected while the native reader was held.
The fixture rejected it, killed and reaped both owned processes, removed their
shared-memory namespace, and closed the parent gate descriptors.

Evidence is under `.omo/evidence/274-paramsd-phase/`: `controlled-wrap-red`,
`controlled-wrap-green-verified`, `atomic-green`, `unit-green.log`, and
`cleanup-fault-result.json`. Exact-commit CI remains required. This is host
fixture validation; it establishes no physical device, driving or performance
result.
