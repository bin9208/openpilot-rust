# Complete ARM build budget (#331)

Issue: https://github.com/bin9208/openpilot-rust/issues/331

At source19f10b83, [ARM job115943642126](https://github.com/bin9208/openpilot-rust/actions/runs/38590123444/job/115943642126)
started2026-10-14 15:34:09UTC and ended cancelled17:04:46UTC,90m37s later.
No successor branch push cancelled it. The configured job budget was90minutes;
the timing and active compilation strongly identify that deadline. The two
check annotations are not readable through the connected tool, so this is an
inference from the job timing/configuration and retained log.

All preceding GNU and musl workspace, static QEMU probe, native Card,
selfdrived/navigation, Panda, generic/ION camera and pinned-codec encoder steps
completed successfully. The static FFmpeg/full-musl workspace stage alone took
about35minutes. Generic UI finished its release build at17:04:22, then the ION
UI variant was compiling when the runner emitted `The operation was canceled`
at17:04:41. The log contains no compiler failure for this job. ION UI completion
and final UI artifact checks were therefore not established.

Only the `arm64` job timeout changes from90 to120minutes. All commands, targets,
feature variants, source pins, reserve guards, two build workers, incremental
off and artifact requirements remain identical. This is additional wall-clock
capacity for the existing work, not a skipped or relaxed compiler/functional
gate. The complete exact-head job must succeed; the earlier cancelled job is
not counted as success. Separate exact-merged-dev-PUSH gates remain required.
Generic ARM builds and emulated static-probe checks do not establish AGNOS or
vehicle acceptance. No device was connected.

Validation: existing CI isolation/native guard policy tests, native documentation
validation and diff checks are run before publication; fresh complete cloud
execution remains required. Docs-Not-Needed: internal CI execution budget only.

Local results:37 CI isolation/native-bootstrap tests and367subtests PASS;
native documentation validator and diff check PASS. Independent readback found
no Important/Critical issue and verified the workflow diff contains only the
single90→120minute bound. The new complete cloud job remains pending.
