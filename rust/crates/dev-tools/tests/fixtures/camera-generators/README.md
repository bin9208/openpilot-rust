# Camera C++ reference generator fixtures

`original.json` captures27 executions of the unchanged Python request/packet
generators against owned trees. Every successful stdout length/hash and every
failure's empty stdout were captured before writing the native implementation.
`cli.json` captures14 argument parsing outcomes, including help, missing values,
unknown options, abbreviations, equals syntax and repeated source options.
Source and template SHA256 values and CPython version are in the original record.

The native command always reads the caller's current C++ source and template.
These records are independent expected results, not production pre-generated C++.
Cases preserve universal newline conversion, Python trailing Unicode whitespace,
all placeholder occurrences (including absent placeholders), required substring
errors, invalid UTF8, empty templates, and packet-method closing semicolons.
Original generator source blobs remain in Git history with original licensing.

Capture scripts and full original C++ outputs are local development evidence in
`.analysis/scratch/2026-10-18-camera-generators/`; neither device captures nor
private input is included. These checks do not establish hardware operation.
