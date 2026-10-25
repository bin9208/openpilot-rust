# Independent native-build closure oracles

Captured on 2026-10-15 from unchanged project Python at baseline
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`, before implementing the Rust tools.

- `services.json` records all 88 original service tuples in insertion order,
  including frequency ranges. The two `.h` files contain exact `build_header()`
  and script `print(build_header())` bytes, preserving their different endings.
- `json11-original.json` records the original helper's returned prefix,
  dependency fields, curl command and extracted-file hashes for both supported
  architectures. The two small fixture wheels contain a header and static-library
  marker, use fixed ZIP timestamps, and match `json11-lock.toml` hashes.
- `runtime-original.json` records sanitized and ordinary runtime-source compiler
  commands, logs, manifests and service headers using a controlled compiler peer.
  Generated source files contain `controlled generated NAME\n`, the fixture
  contains `controlled libusb fixture\n`, and compiler products contain
  `controlled compiler product\n`. Root/output/dependency paths alone are replaced
  with `$ROOT`, `$OUTPUT` and `$DEPS`.

The original implementations perform tuple generation, header formatting,
wheel selection/extraction and source fingerprinting. No network, real compiler
or device was used during capture. Expectations are not generated from the Rust
implementation. Actual compiler comparisons are recorded separately.

`logging-original.json` independently captures the complete original logging
builder for both architectures. It includes locked staging, exact fixture-clock,
version and hardware overlays, compiler argv, combined build log and complete
source/dependency provenance. Only fixture paths are normalized. Compiler
failures in the original leave overlays but write neither build.log nor success
provenance; native regression tests preserve that contract.

`logging-oracles-original.json` captures eight unchanged bootlog/loggerd builder
cases before port implementation: both bootlog/identifier modes with default
staging or explicit dependency prefixes, and all four loggerd optional-prefix
combinations. It retains compiler argument order, the Cap'n Proto environment,
all original evidence files, exact services script-header bytes and copied schema
hashes. The native comparison excludes only the original `python3 services.py`
command, whose output must match the native header byte-for-byte.
