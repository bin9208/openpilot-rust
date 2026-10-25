# Independent camera lifecycle generator captures

`original.json` records138 executions of the unchanged CPython3.12.15 generators
against isolated current-source/template trees. All12 baseline modes succeed:
AE, misc, kernel, sensor source/fixture, ISP source/fixture, camera source/fixture,
state, stress, and runtime fixture. Expected output bytes are represented by
length and SHA256. Failures retain their exit class and empty stdout.

Mutations exercise universal newlines, Python trailing whitespace, absent and
duplicate markers, current-source body changes, missing extraction signatures,
invalid UTF8, missing files, fixture replacement cardinality, split cardinality,
and state composition replacement checks. `extract.json` independently captures
nine examples of the original shared brace counter, including braces in C++
strings/comments and missing/unclosed bodies. The implementation must preserve
that intentionally simple algorithm. `cli.json` records67 original CLI outcomes;
successful generation also records exact stdout length/SHA for native-only tests.

Source/template/generator hashes are in the capture record. Captures use the
actual original modules with `PYTHONPATH=<root>/rust/tools`, matching the camera
CI import closure. State invokes original AE; camera/runtime invoke original ISP
fixture generation. Native replacements compose the corresponding Rust
algorithms directly and never execute Python. Retired generator scripts are
provenance, not runtime input. All C++ source and template bytes remain live
inputs; generated output fixtures are never production policy.

Local capture scripts, original generated C++, and logs remain in ignored
`.analysis/scratch/2026-10-18-camera-lifecycle-generators/`. These are synthetic
source-transformer tests, not device or camera hardware validation. BPS/sensor
constant generation and comparison/cleanup checkers are separate task slices.

The implementation followed the intended missing-module/bin RED in
`2026-10-18-camera-data/lifecycle-red.log`. Native tests passed all138 source-tree
cases,9 extractor cases, and67 CLI cases, including exact successful stdout.
All12 current-source generated C++ outputs matched the independent originals
byte-for-byte, and native-only generation traces contained no Python execution.

The parent compiled the actual generated C++ and reported full host comparison
passes:334 kernel scenarios,62 sensor lifecycle cases,30 ISP lifecycle cases,
32 camera lifecycle cases,8 cleanup cases,1,080 state frames,54 stress cases,
and8 continuous runtime scenarios. These use the retained independent source
and syscall/transport fixtures; they do not establish camera hardware or device
acceptance. Initial local SDK/sanitizer setup failures were investigated before
the completed host reruns; the retained receipts distinguish those attempts.

The migrated caller invokes the native generators directly and retains required
source/template inputs, compilation, comparisons, and failure evidence. Python
comparison and cleanup checkers remain outside this generator retirement slice.
The parent post-deletion gate passed65 generator/CI tests and scoped strict
Clippy. All12 generated modes again matched original bytes with PATH=/nonexistent
and native-only execve traces after removal. Sensor path review separately added
10 original cases; the four sensor tests and scoped strict Clippy also passed.
