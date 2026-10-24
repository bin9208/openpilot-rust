# Independent camera runtime stress checker controls

Captured from unchanged `rust/tools/camerad_runtime_stress_check.py` (SHA256
`62b87a20edc76103bf88be0b036b650c9836aca21c4f6532ce536f848deac99a`) and the
34 policy scenarios in `check_camerad_stress.py` (SHA256
`05407fbd24e73550d8236c79d28213021493138d6436eb5b559e3bf0e10803aa`).

The original current C++/Rust fixture run passed 54 cases and 151 observations.
`cases-original.json` preserves policy inputs, expected outputs and ordering.
`process-original.json` preserves 30 executions of the original checker with
controlled child replies and clock observations: host/QEMU, launch/timeout,
malformed results, source/native mismatch, logging, errno, BOOTTIME boundaries,
and existing output directories. Original JSON serialization and assertions
remain active during capture. The saved file hashes bind partial artifacts and
failure ordering. Paths are replaced with `$ROOT`; no real device data is used.
`cli-original.json` records 12 actual original CLI invocations and side effects.
The seven additional process controls independently capture Python's errno
fallback for null, false, empty strings/lists/objects and nonempty lists/objects;
the initial 23 captures remain unchanged. `cli-paths-original.json` adds seven
actual original CLI executions covering lexical Path normalization, PATH versus
cwd executable lookup, retained parent components, empty paths and leading
slashes. Small shell peers produce controlled results; the original checker
performs its own first policy assertion and writes the captured partial receipts.

Real native validation runs both independent C++ and current Rust binaries with
PATH disabled. Reports and artifacts must match the original except BOOTTIME
values, which must each lie between independently read before/after values.
The older generic stress checker has a distinct stderr policy and remains
separate; replacing this runtime checker does not retire that entrypoint.
