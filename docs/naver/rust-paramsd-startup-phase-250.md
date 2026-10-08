# Paramsd persistence fixture startup (#250)

The post-merge Rust estimation job for dev
`0d844b72cb7d8e7fe679304991c78d40992d58ee` failed its strict persistence
comparison: the saved event was publication 1198 instead of 1199.
[Failing Actions run](https://github.com/bin9208/openpilot-rust/actions/runs/37837364409).
The other 40 individual Rust jobs passed; this failure also failed the aggregate
Rust gate. Fast checks, user-doc checks and the separate integration workflow
passed. These results do not establish complete runtime or device acceptance.

The fixture supplies an initial empty update to its source oracle, then 1202
input batches. Its native readiness check previously established only that
queue readers were registered. It did not establish that exactly one empty
update had completed before the first input. Both original and native
SubMaster update counters advance when a poll completes without a message.

A small fixture-only preload observer records the actual main-thread wait
boundary after removal of the owned startup GPS marker. With first input
withheld until two waits completed, the unchanged current native executable
reproduced the exact 1198 assertion failure. All 1202 event comparisons otherwise
passed. A separate live original main/SubMaster control observed frames 0 and
1: frame 0 had no updated services, and frame 1 consumed the queued pose.

The corrected fixture gates entry to the second wait, after one completed
empty update. It queues the first input, checks that the actual pose queue write
pointer advanced, and releases the gate. The 100 ms runtime timeout and strict
saved-byte comparison with publication 1199 remain unchanged. The observer's
later wait returns are diagnostic information, not an inferred empty-update
count: a successful wait can return when new input is already queued.

Focused validation with the current native executable passed 1202 continuous
source comparisons, exact persistence at publication 1199, and eight cached
restart comparisons. Internal/external GPS selection and output files matched;
SIGINT and SIGTERM both exited successfully. No daemon, estimator, messaging
runtime, safety threshold or persistence policy was changed.

Local evidence is retained under
`.omo/evidence/250-paramsd-startup/`: `source-live-initial`,
`controlled-current-red` and `green-initial`. The retained older executable
uses the previous C++ messaging transport and is not the final native proof;
the focused current build supplies the relevant ppoll boundary. Existing
unchanged estimator corpus results are reused. Exact-commit branch and
post-merge CI remain required after this local checkpoint.
