# Miri core-library provenance

`original.json` records 30 executions of the exact Python expression formerly
embedded in the RadarCAN Miri workflow, using CPython 3.12.15. Each entry records
filesystem construction, environment-root spelling, process status and raw stdout
bytes. Expectations were captured before implementing the native command.

Coverage includes nonempty and empty selection, hidden architecture directories,
component ordering, raw filesystem bytes/Unicode/control characters, binary and
empty libraries, symlinked directories/files, broken links, cycles, directory
matches, non-directory ancestors, dot/parent components, absolute/double-slash
roots and missing/empty environment variables. Errors leave stdout empty; Python
traceback wording is not a native CLI contract. The command still fails before
publishing any JSON when any selected library cannot be read.

`actual-proof.json` compares the actual existing pinned Miri sysroot with the
original expression and native command, including source/binary hashes and exact
stdout/stderr agreement. The native process had PATH=/nonexistent. This is a hash
collector comparison, not a new execution of the Miri numerical tests; all four
existing Miri levels and their filters remain required in the workflow.

Independent review found that directly opening `root/lib/rustlib` bypassed
Python 3.12's directory scans of the literal prefix components. Five captured
unreadable-component cases now require the same rejection, using the already
verified Python glob selector. A sixth case proves a read failure after a valid
selection still publishes nothing. Permission cases run as the unprivileged
Linux user used by development and CI.

`deleted-cwd-original.json` is an additional independently captured original
success with an absolute sysroot after its current directory was removed. The
native collector must not add a `getcwd()` prerequisite. Its regression launches
the native command in an owned deleted directory without Python on PATH.
