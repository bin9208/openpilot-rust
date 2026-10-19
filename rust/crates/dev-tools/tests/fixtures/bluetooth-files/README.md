# Bluetooth atomic-file checker source contract

`source-pins.json` records bytes read from the retained original checker,
`model.atomic_json`, and native example source. It is a source identity roster,
not a captured result fixture.

No original or candidate program was executed locally while authoring this
conversion. The reference corpus is an independent translation of the original
fixed schedule and JSON string encoding rules, without importing the native
Bluetooth serializer. It is **not yet validated**.

The mandatory CI test runs the unchanged `check_bluetooth_files.py` against the
fresh `file_fixture`, reads **only its `original/` files** as expected data, and
compares all 15 full file byte sequences, original JSONL input bytes, modes and
Unicode success/failure outcomes with the new corpus. It then invokes the real
native controller/candidate and compares all original summary fields. It writes
compact actual original hashes and provenance to `BLUETOOTH_FILES_EVIDENCE`.
Mocked negative controls copy the original files produced by this unchanged
source run; they do not generate golden answers through the native candidate.

The retained schedule includes null, both booleans, zero, negative zero, two
float exponent boundaries, infinities, NaN, a nested object with a 201-digit
integer and Unicode/NUL/DEL, three lone-surrogate failures, and every 1,112,064
Unicode scalar value. Candidate output remains subject to the original
30-second subprocess timeout. Exact file bytes and permission bits, 15-file
directory cardinality and removal of temporary files remain assertions.

Local checks are formatting/diff/syntax only. Successful compilation, six test
entry points, cancellation/timeout cleanup, and exact source parity are pending
GitHub Actions. The Python checker must remain live until those cloud results
are reviewed. A later retirement patch must seal the actual original CI
observations and transition only that checker's pin to absence-only historical
admission; changing/dangling files and missing runtime inputs must still fail.
No device or performance claim is made.
