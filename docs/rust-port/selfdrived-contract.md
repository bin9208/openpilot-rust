# Native selfdrived contract (#168)

Part of the approved whole-runtime conversion in #1. Source behavior is defined
by `openpilot/selfdrive/selfdrived`, `selfdrive/car/car_specific.py` and the
existing cut-in alert helper. Preserve event sorting and duplicates, category
priority, static event persistence, creation delays, alert replacement/expiry,
all original health and validity gates, Carrot extensions and vehicle-specific
decisions. Existing Korean/English translations remain source data.

The native state machine must match every category combination from each of
the five source states, including the exact 300-cycle soft-disable interval,
50-cycle urgent warning boundary and pre-enable/override priority. The daemon
must retain the 100 Hz carState-driven loop, core6/FIFO53 scheduling, Params
effects, source health snapshots and selfdriveState/onroadEvents wire semantics.
No CAN, pose, DM, camera or safety threshold is relaxed.

Preserve binary64 operation order in policy arithmetic, including strict and
inclusive comparisons. Discrete outcomes, alert text and event order require
exact equality. Pose transformations will reuse established native orientation
code and compare against the original calibration helper before integration;
numeric error must not cross a safety branch. Unknown enum/wire data must fail
explicitly rather than silently enabling control.

Verification proceeds from original-source state/alert/event sequences to the
native continuous executable with owned cereal peers, Params, clock/fault
boundaries and signal cleanup. Generated static alert data may be produced at
build/development time from pinned original source; no original Python runs
inside the runtime candidate. External msgq/VisionIPC and native libraries
remain explicit until their own port stages are complete.

No device/C3X/NAS or real CAN connection is authorized for this work. Component
checks are intermediate evidence. Normal startup, existing log upload, complete
runtime inventory and the user's first device comparison remain gated by #1.

## Host component completion (2026-10-02)

`openpilot-selfdrived` now runs this complete component continuously. It blocks
on physical CarParams, subscribes to nonconflated carState with the original
20 ms timeout, retains previous carState on timeout, updates native SubMaster
health, publishes selfdriveState/onroadEvents and monitors the original 100 Hz
Ratekeeper arithmetic. A native 100 ms Params worker refreshes metric,
experimental-mode and personality values; signal and failure exits join it.
Original core6/FIFO53 configuration remains gated to TICI hardware.

Missing NNFFModelName is retained as a null alert second text through callback
resolution, translation, manager insertion and selection. Rejection occurs only
when the selected alert reaches cereal encoding, matching the source failure
phase and earlier effects. Failed-process logs retain Python set repr as text;
the member order is deterministic, because source set order is unspecified.
The source oracle normalizes only that order and still checks text type/members.

Fresh source comparisons cover all executable SelfdriveD method statements
except the blocking CarParams constructor branch, which is exercised by native
owned-process startup and cancellation. Explicit source exceptions, wire error
paths, continuous native IPC and captured artifacts are recorded in
`docs/naver/rust-selfdrived-168.md`. This host component status does not complete
the whole runtime or establish target-device scheduling/performance acceptance.
