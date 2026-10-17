# Shared UI orchestration original captures

`original.json` records 22 executions of the unchanged current
`check_ui_framework.py` (SHA256
`657c638fc5cc2a071f45b8482ecb1866e3b6ee26eef4a04746a52226e5bdfcd3`).
Only subprocess execution and free-disk lookup were injected. The original
chooses every child command and its order, environment, cwd and output; it writes
invocation prefixes and ledgers and handles return codes or exceptions. Cases
include all 13 failing stages, signal return codes, native/Python/EGL spawn
failures, exact disk threshold boundaries, and log/ledger/disk-receipt write
failures. Only roots and interpreter paths are replaced with placeholders.

`process-original.json` adds five actual original process executions with owned
fixture children: all stages pass; widgets, network and EGL fail; scroll receives
SIGTERM. Merged stdout/stderr, invocation prefixes, partial ledgers, fail-fast
ordering and Unix exit status are captured independently. Fixture children are
small Python processes, not the production native subcheckers, and these captures
make no claim about subchecker equivalence.

The current unchanged caller uses native render, Forms and diagnostics, as well as
widgets, scroll, navigation, stack, scroller, polygon, FPS and EGL. Network and application remain explicit Python dependencies. The native draft
accepts `--python` (or `OPENPILOT_REFERENCE_PYTHON`) to select the original
interpreter; otherwise it uses `python3`. No subcheck is removed or represented
by a captured answer. The render caller switch was independently recaptured from this exact source
before updating its command and provenance hash in the native draft.

The registered native command passed four tests covering the22 injected and
five actual process contracts, and scoped strict Clippy. The actual13stage
UI gate passed, including live X11 input/normal exit, recording with FFprobe,
log transport, FPS and EGL. This does not claim Python-free framework closure:
the network and application subcheckers remain Python. Capture scripts and
process logs remain in ignored scratch.

The caller migration is now complete: CI selects `check-ui-framework`, and the
Python orchestrator is removed. `legacy-original.json` retains the seven prior
caller regressions captured before deletion. The complete13-stage native caller
passed again after deletion on an owned Xvfb display; the two explicitly listed
Python subcheckers are still required and are not counted as retired.
