# Intermediate UI CI process infrastructure

These fixtures independently execute unchanged functions from
`rust/tools/check_ui_ci.py` and `card_qa/ci.py` with pinned CPython 3.12.15.
Source SHA-256 and source line ranges are retained. `display.json` records seven
actual controlled executable children, including the original real 15-second
startup waits and 10-second TERM-to-KILL wait; every child was reaped.
`display-text.json` adds six actual Unicode/whitespace/malformed stream cases.
`collector.json` records six actual success/failure/invalid-UTF8/spawn cases, including invalid stderr and nonzero status with invalid stdout.
`orchestration.json` captures twelve peer-selection cases (plus seven raw-path/NaN/container-membership cases in `peer-extra.json`) and the then-current
ordered fifteen comparison commands (twelve Python and three native). Subsequent
QR/torque migration changes that caller split to ten Python and five native;
these are historical original captures, not a current dependency inventory.
The capture programs and complete records remain under
`.analysis/scratch/2026-10-15-ui-ci-orchestration/`.

This module is intermediate infrastructure, not retirement of `check_ui_ci.py`
or its comparison family. Python module imports, binding-directory containment,
package-version queries, cereal message construction, source and binary hashes,
graphics/XTEST admission and all pending Python comparison children remain
explicit requirements. No fallback silently bypasses those gates.

The owned Xvfb uses a separate inherited display descriptor; stdin is inherited,
and stdout/stderr share the original log file. Parent descriptors retain CLOEXEC;
command-fds 0.3.3 maps an owned duplicate only in the child. No workspace unsafe
code or parent-side CLOEXEC window is introduced. The parent retains both socket
endpoints through cleanup, matching the original retained pipe's early-exit
startup timeout. Unicode decimal membership reuses the pinned Unicode 15 table.

Original waits remain 15 seconds for startup and 10 seconds after TERM before
KILL/reap. An explicit additional policy bounds the *complete* display line by
the startup deadline and 65,536 bytes: original readline could hang after its
first readable partial byte. Controlled tests exercise this new rejection.
The collector likewise requires an explicit caller deadline and limits each
captured stream to 64 MiB, additions to the original unbounded collector.
Timeout/limit failures preserve only the prewritten command receipt, never false
stream/exit/success receipts; owned command process groups are cleaned up by the
audited native runner. Completed admitted commands preserve strict UTF-8,
universal CRLF/CR newline conversion and original receipt ordering.

The normal suite uses controlled shell peers with shortened configurable waits.
The opt-in ignored `actual_private_xvfb_is_owned_and_reaped` test requires an
explicit `UI_CI_XVFB` private executable path and verifies XTEST with xdpyinfo on
its newly allocated display. It never opens an existing user display.

Validation: the fail-closed stub produced seven independent test failures before
implementation. Twelve ordinary tests and strict focused Clippy passed. The
opt-in actual private Xvfb test passed with an explicitly supplied scratch
executable, XTEST query and owned-process reap; receipts, merged log and
xdpyinfo outputs are preserved in scratch `actual-native/`. No orchestration
caller was switched and no Python comparison source was deleted by this unit.
