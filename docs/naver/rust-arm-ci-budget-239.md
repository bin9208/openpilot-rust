# Generic ARM job capacity: issue 239

The expanded ARM gate at dev `e0c8fed2ea0734d3d7e66ec9f48fbdd1141cfe16`
exhausted its 45-minute job budget while compiling the final Panda target.
[Job 113316729856](https://github.com/bin9208/openpilot-rust/actions/runs/37778910131/job/113316729856)
has an explicit maximum-execution-time annotation. GNU workspace, Card,
selfdrived, navigation, VisionIPC ION and static-probe stages had completed.
The probe stage took 17m36s; Panda began at 13:29:51 UTC and was cancelled
at 13:31:12 UTC. Its incomplete artifact is not a passing Panda build.

Issue [239](https://github.com/bin9208/openpilot-rust/issues/239) increases only
this job's budget from 45 to 60 minutes. Build commands, inspections, artifacts
and required checks stay present. The runtime's timing and validity policies
are unaffected. Exact updated integration-head completion remains the gate.

The same run's separate UI job exhausted 35 minutes after APT fetched 130 MB
in 30m15s at 71.8 kB/s. Its package build succeeded before the time limit
interrupted UI comparisons; later connectivity comparisons were skipped.
That failure is not a UI assertion result. A scoped same-head UI rerun was
requested; its success remains unverified at this checkpoint. No UI budget
or test is changed here.

Raw logs are retained locally in
`.analysis/scratch/2026-10-08-runtime-resume/234-postdev-cancellations/`.
The cancelled dev Rust aggregate remains incomplete; passing PR checks do not
replace it. Host cross-build results do not establish normal startup/upload,
device behavior or measured CPU savings. No device was accessed.
