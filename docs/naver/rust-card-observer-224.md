# Card fixture release ordering (#224)

Issue: https://github.com/bin9208/openpilot-rust/issues/224
Failed head: `ec9753f0225d88978fc197bf8926dedd37c62e7a`.
Run: https://github.com/bin9208/openpilot-rust/actions/runs/37641074242/job/112860664805
Artifact: `11494781316`, `rust-card-can-host-evidence`.

The failing source case is `SUBARU_ASCENT-True-source`. Its pump sent all80
CAN packets at10.126..10.353ms intervals. The first source state drained ticks0
through11 together,114.84ms after the first packet; ticks12..79 each produced
one state. Thus69 output states account for all80 inputs. Both COMMA_BODY
source windows contain80 states. The prior assertion omitted the case path.

The fixture started its independent measured sender while the completed-warmup
source was still SIGSTOPped. The observer then polled the sends file before
resuming that source, allowing queued input to accumulate during observer delay.
The pump now sends SIGCONT to the owned receiver immediately before its first
measured send. The PID comes only from the capture's owned Popen instance.
The observer no longer controls this release. Independent100Hz deadlines,
the80-state requirement, packet/timestamp assertions and timeouts are retained.
The count failure now includes the source/native sends path for direct triage.

A narrow regression runs the existing pump with a real stopped Python child
and pipe, delaying the observer by130ms. Before the fix,80 inputs produced68
steps with13 packets in the first batch; after the fix,80 inputs produce80
steps. All three existing phase-fence tests pass. This verifies fixture release
ordering; new hosted full-source/native comparisons remain pending. No local
native build, complete Card corpus, runtime policy or device change was made.

Docs-Not-Needed: isolated comparison fixture and failure diagnostics; no user
setting or production behavior change.
