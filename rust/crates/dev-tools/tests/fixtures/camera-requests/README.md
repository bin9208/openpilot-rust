# Original request checker captures

Expectations come from unchanged `rust/tools/check_camerad_requests.py`, whose
SHA256 is recorded in `generation-original.json`. Generation covers all512
scenarios and43batches,72493steps, per-case native JSON digests, separate source
and native batch input digests and the exact pretty-printed `cases.json` hash.
The native implementation must generate these requests dynamically.

`process-original.json` contains18 original executions with only scenario
selection and subprocess results injected: success, nonzero/signal exits,
spawn/timeout failures in either lane, cardinality/JSON failures, differences,
malformed runner, missing final source hash, existing failure file, and report/
failure path collisions. Input order, stdout and output files are captured from
the original, not calculated by the replacement.

Capture scripts remain in ignored `2026-10-18-camera-requests`. Initial native
test failed because camera_request_check did not exist. All five tests and the post-deletion live 512-scenario/72,493-step comparison pass;
the report is byte-identical to the original with PATH disabled. The Python CLI
has been removed after caller migration. Stream-limit review reproduced and
fixed an added 16 MiB stderr restriction using the same original/native probe.
