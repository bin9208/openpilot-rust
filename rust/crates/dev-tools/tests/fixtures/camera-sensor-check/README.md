# Original sensor comparator evidence

All expected values were independently captured from unchanged
`rust/tools/check_camerad_sensors.py` (SHA256
`401526615ca939d19cb79b384bb589887e340e9ef2b2c5241ce3f9e888426dcc`)
using CPython 3.12.15 before writing the native comparator.

`live-original.json` records the actual current-source C++ reference against the
current Rust sensor_policy example: all 535,020 cases passed (3 config, 9 address,
475,008 exposure, 60,000 score), with seed 187 and input SHA256
`067eeb0afb1d9f4654a49957d007977dfb298816a4163ee9e76f71f377faf568`.
The initial C++ configurations and every source batch's SHA256, line count and
endpoints are captured. The native implementation must generate requests from
live configurations and CPython-compatible seeded random operations; production
must never consume this fixture or a canned request blob.

`requests-original.json` captures 13 changed configuration/invalid admission
cases. `comparison-original.json` preserves 37 original structural comparison
outcomes, including binary32 rounding/sign/NaN/overflow and exact Python error
representation. `process-original.json` captures 17 process/cardinality/JSON and
existing-output cases with original child requests and exact artifact/stdout
bytes. `runner-original.json` captures 13 shlex boundary cases.
`random-original.json` independently records random(), getrandbits(), randrange()
and .17g formatting, including subnormal/maximal binary64 values.
`cli-original.json` captures 17 CLI/runner/spawn/encoding/hash/write failure
boundaries, including the stage at which directories and receipts appear.
`paths-original.json` records four actual original CLI successes for raw Unix
runner bytes, trailing separators and symlinks. `depth-original.json` records
direct comparator recursion limits, while `main-depth-original.json` and
`parser-depth-original.json` capture 38 actual CLI boundaries. Main comparisons
succeed through 997 nested arrays, fail at 998, and mismatch serialization writes
through 994 but fails before creating a receipt at 995; parsing independently
admits through 9,997 arrays. Native tests assert these distinct failure phases.

Capture drivers and original live binary provenance are retained in ignored
`.analysis/scratch/2026-10-18-camera-sensor-check/`. The native implementation is source-ready after the intended missing-module/bin
RED. Compilation, eight regression tests, strict Clippy and the full live
native proof remain pending. No retirement or device result is claimed here.

Parent verification: all eight tests and scoped strict Clippy passed. The actual
native535020-case run with PATH=/nonexistent produced a byte-identical original
report, including every generated input digest. The Python CLI was removed;
its unchanged `equal`/`f32` helpers remain for three pending comparison tools.
This is a partial source retirement and does not claim camera hardware testing.
