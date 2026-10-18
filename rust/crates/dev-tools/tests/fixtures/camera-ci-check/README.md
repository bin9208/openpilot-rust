# Camera CI orchestrator original contracts

`original.json` pins the unchanged Python orchestrator and its process helper.
The actual original CameraCheck ran58 ordered commands in its successful case,
with eight controlled child failures and one low-disk refusal. Only the subprocess
and disk-query boundaries were controlled; original source staging, command and
partial artifact writing, generated-file copies and constant comparisons ran.
This is an orchestration contract, not source compiler/camera validation.

`main-original.json` separately captures current-source hashing, executable
requirements and failure order for five original main invocations. The native
entrypoint adds its own source and runtime-controller executable to the receipt.
`cli-original.json` captures nine actual original CLI subprocesses.
`launch-original.json` captures real missing-capnp process launch with seven
retained prelaunch files and no false final result. Invocation hashes have their
own main-contract coverage and are not copied into this launch fixture.

Fixtures were captured before native implementation. Tests preserve all command
arguments and output bytes, with only temporary root substitution and the explicit
Python-runtime-command to native-runtime-command migration. Production reads
current sources and executes current native/C++ commands; it never replays these
fixtures. Complete58-command live proof remains a separate required gate.

`constants-original.json` adds two original mutation controls: changing either
formatted sensor constants or BPS constants stops before any policy comparison,
retaining the respective 35 or 38 command artifacts and no final result.

The real native58-command gate passed on2026-10-18, including all eight continuous
runtime cases. All37 preflight executable hashes remained unchanged. Five native
orchestrator tests preserve the independent captures, CLI admission, real missing
compiler failure and mandatory workflow routing. Post-deletion focused regression
and freshly rebuilt camera runtime evidence is recorded in the #296 ledger.
