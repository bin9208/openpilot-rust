# Independent application lifecycle captures

`source-original.json` records the five actual original helper modes: render
profiling, startup profiling, paused rendering, global recording and dynamic
recording. The original source remains unchanged and imports real application,
InputBox and Button policies. Captures ran on the worker's dedicated Xvfb display.

Frame/tick counts are 6/6, 0/0, 4/4 with two skipped frames, 9/9 and 9/9.
Both actual ffmpeg paths produced a 536×240 video at 20 FPS containing three
frames. PNG mode/shape, original bytes and i64 samples, JSON histories and ffprobe
streams are retained as fixture metadata. Original raw PNGs/videos/logs are
ignored scratch, not committed captures.

This is capture evidence only. Native checker/reference implementation and
process, live X11, environment, failure and cleanup contracts remain pending.
The original helper/checker are not retired by these five mode captures.

`checker-original.json` captures 32 independent original checker executions with
injected process, clock, Xlib and ZMQ endpoints. The original JSON assertions and
Pillow image operations execute unchanged. Captures cover ordering, inherited
environment, subprocess failures, live input/process identity and cleanup, video
validation, and logging failures. Original RGB comparison ignores alpha and
compares the common image extent when dimensions differ; those successful
original cases are retained explicitly. Four small synthetic PNGs support those
process tests.

`environment-original.json` contains 20 actual original helper executions. This
includes arbitrary mode dispatch, zero/negative render profiling, startup and
burn-in presence semantics, overlays, VSYNC, strict mode, and eager numeric
environment parsing even when recording is disabled.

`recording-policy-original.json` invokes the unchanged original class methods
with injected process/thread endpoints. Sixteen cleanup cases preserve detached
member references before callbacks, idempotence, exception scopes and the exact
30/10/5-second joins/waits. Four dynamic command cases preserve fallback FPS,
queue capacity and the distinct ultrafast encoder policy. These captures are
not evidence that native application replacement has passed; that remains pending.

`render-loop-original.json` captures fifteen executions of the unchanged original
render generator with mechanical graphics, clock, input, queue and widget
endpoints injected. Complete ordered calls bind the yield/resume boundary,
profile exit, paused skips, drawing/overlay ordering, every-third-frame recording,
queue.Full handling, rollover, window/request close, consumer close, and callback
errors. `mouse-original.json` captures the original two-slot mouse transitions,
position scaling, idle/repeated-event suppression and drain behavior.
`argv-original.json` records six actual checker argparse executions.

The new native controller and mouse policy are pending package compilation and
comparison with these captures. The native reference/live X11/encoder owner and
complete checker remain unfinished; original source tools remain maintained.

The generator and recording policy package gates now passed: two generator tests
compare all fifteen ordered phase histories and three mouse histories; two
recording tests compare cleanup scopes and exact global/dynamic encoder argv.
Strict Clippy passed for both policy test targets. This does not complete the
application reference or live checker.

`writer-original.json` independently invokes the unchanged original writer
method in thirteen queue/process cases. It preserves one write call per frame
(including an ignored partial return), sentinel stop, empty-queue stop polling,
draining queued frames even when the stop event is already set, write/read
exception termination, and exceptions raised by the empty-queue event handler.
The new recording test for this API is intentionally pending its missing-API
compile RED and native implementation gate.

2026-10-18 reference integration evidence: native loop2 and recording7 package
checks passed, including actual ordered stdin/EOF/reap and broken-writer queue
retention regressions. The independent native reference now passes all five real
modes and twenty environment cases on its owned DISPLAY92, preserving complete
RGBA samples and state, plus both three-frame native FFmpeg recordings. Six
resource cleanup cases preserve callback order and partial owner state.

`record-output-path-original.json` captures eleven original CPython pathlib
suffix results. Its native test observed the ParentDir mismatch before repair
and passed after repair. `touch-draw-original.json` captures seven original
post-yield touch overlay histories, including exact raylib RED, history capacity,
press reset, truncation and three-second expiry. Native color regression failed
before repair and passed after repair. `source-closure.json` records observed
project imports, actual font-resolution choices and higher-priority absences,
and relevant locked graphics dependencies. Its drift matrix failed before guard
integration and now rejects changed/missing sources, new font/provider layouts,
and changed dependency records.

The latest bounded reference gate passed six tests (82.71 seconds), both prior
raw tests, and strict Clippy. These results do not cover the new native XTest or
complete checker yet: those APIs have observed missing-command/module REDs and
are being implemented. Both original source tools remain retained until the
32-case checker policy and actual five-mode-plus-live interaction proof pass.

The subsequent checker gate now passes all 32 independently captured contracts,
three real native ZMQ/owned-child tests, and three raw boundary tests including
actual XTest keyboard/mouse input and captured BadWindow protocol errors. Strict
Clippy passed. A fresh current-source frozen ELF proof ran all five source/native
modes plus live input on DISPLAY92 using only native FFmpeg/ffprobe on PATH. Both
videos retained three frames; real typing produced `native`, the first Close
click exited cleanly, and all 37 live frames had one tick each. Native process
identity had no mapped libpython; 794 received log records satisfied original
transport checks. Source tools remain until parent caller/CI migration and
post-deletion verification; this evidence is host-only.

Final post-retirement integration (2026-10-18): the framework selects native
check-ui-application; the required UI job builds the independent raw/XTest
boundary and runs all six lifecycle suites before the complete framework check.
Only rust/tools/check_ui_application.py and
rust/tools/ui_qa/application_source.py are removed. The live application,
widgets, assets, import-closure pins and original capture metadata remain intact.
Missing-source admission permits exactly those two retired tools; changed
reintroductions still fail, and every remaining source/font/dependency guard is
retained.

The post-deletion run passed 78 distinct tests across eleven selected suites,
strict Clippy and the three production binary builds. A test-fixture-only failure
for creating a reintroduced retired file in a missing directory was reproduced
and repaired; the drift matrix then passed unchanged guard expectations. All
five real reference modes and twenty original environment cases passed after
deletion. Frozen fresh binaries then passed all six actual checker modes on the
owned DISPLAY92 with a native-only PATH. The two videos contain three frames,
live typing yields native, the first Close click exits, and all 37 live frames
have exactly one tick. The process identity receipt reports no mapped libpython.
These are host results; device acceptance remains user-owned.
