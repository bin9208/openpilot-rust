# Updater subprocess original behavior baseline

`original.json` was captured on 2026-10-15 by running the unchanged
`rust/tools/updated_process_source.py` wrapper and its imported
`openpilot/system/updated/process.py` against the native
`updated-process-fixture` command. Each original Python input file was checked
byte-for-byte against Git commit
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a` before capture. Their SHA-256 hashes,
the native fixture's compiled source identity, complete command schema, and
original stdout/stderr/results are recorded in the JSON.

The three normal cases retain the original checks for merged stdout/stderr with
CRLF/CR translation, nonzero status and failure detail, and nested Git commands
preserving existing configuration while disabling automatic maintenance.
The two group cases start a worker that ignores SIGINT/SIGTERM and retains the
output pipe, with its direct leader either alive or already exited. The original
wrapper received SIGINT; it returned `interrupted: true` within four seconds and
the worker stopped within one second. These facts were observed independently
before the native checker used this baseline.

The native checker verifies the exact baseline hash and native fixture source
identity before executing candidate commands. It runs the current candidate for
all five scenarios, using the original 10-second normal-command, 5-second worker
readiness, 4-second interrupt-return, and 1-second worker-stop bounds. Its process
state probe calls the migrated CI helper and retains ENOENT/ESRCH, zombie and
permission-error behavior.

Output artifacts named `*-source.stdout` and `*-source.stderr` are copies of
these frozen original observations. They are not a newly executed Python source
comparison. `source-baseline.json` and the final report explicitly identify this
provenance; source group rows have `baseline: true`, and candidate group rows
have `baseline: false`.

The checker and child fixture execute no Python. Original runtime
`openpilot/system/updated/process.py` remains outside this deletion unit because
other original updater tools/tests still import it. Native Python syntax parity
for `tools/ci/check_changed_files.py` is also a separate pending closure.
