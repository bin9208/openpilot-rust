# Native Eigen 3.4.0 header staging preparation

The retained source is commaai/dependencies commit
`40e5d76de1b33a86c5181b63db6782d8f06da1da`, `eigen/setup.py`, through
the release `eigen/v3.4.0`. The three reviewed release wheels are:

| CLI architecture | Wheel filename | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `x86_64` | `eigen-3.4.0-py3-none-linux_x86_64.whl` | 2250658 | `0680581b726b6160646d9053cc7416f68c9a38353a26a7f4e4a58222233aa1ba` |
| `aarch64` | `eigen-3.4.0-py3-none-linux_aarch64.whl` | 2250657 | `b02ee704aa964d9d7bafb53cc0e096680e4a3b4f6fb1cd98b0a85c7298f3dc0a` |
| `macos-arm64` | `eigen-3.4.0-py3-none-macosx_11_0_arm64.whl` | 2250659 | `a733325c7290984f270a0ea1f4af0c6aca4c9cf1dfedec262767098cf8e4f355` |

Read-only inspection of the original release archives found the same 690
relative files and SHA-256 values in each wheel's
`eigen-3.4.0.data/purelib/eigen/install/` prefix (10,130,800 expanded bytes).
The Python extras and wheel metadata are outside that prefix. The ignored
working-data provenance is under
`.analysis/scratch/2026-10-21-native-eigen/`; no wheel is committed.

The native stager accepts only a user-provided wheel with the exact reviewed
filename, size and SHA-256 for the requested architecture. It validates every
ZIP member, stages only the 690 install files in `NEW_DIRECTORY/install`, and
writes `NEW_DIRECTORY/manifest.json` with sorted per-file hashes and source,
release and wheel pins. Destination creation is exclusive; failed validation
leaves no published tree. It does not run or package Python.

Before any caller switches, GitHub Actions must install the **unchanged**
Python shim from the pinned source on its fresh hosted runner. The added
estimation gate retains that existing installation and its real `INCLUDE_DIR`;
it does not replace it with an extracted reference tree. A Rust reference audit must require `EIGEN_WHEEL`,
`EIGEN_ARCHITECTURE`, `EIGEN_ORIGINAL_INCLUDE` (that fresh pip-installed
`eigen/install` directory), `EIGEN_EVIDENCE` (a new receipt directory), and
`EIGEN_NATIVE_INCLUDE` (the separately staged
native `install` directory), then compare every relative path and raw SHA-256
across both trees and the test's independent native staging. Preserve the
original install log, wheel and source hashes, native manifest, both sorted
file-hash inventories and executable identity in a visible Actions artifact.
The mandatory `eigen-tools-native` gate stages all three reviewed archives,
runs malformed archive and no-clobber tests plus strict Clippy on the same SHA,
and verifies source/tool hashes after the full comparison. This first cloud
execution is pending.
No local staging, build, functional test or original Python execution was run
for this source preparation. Existing callers continue using the original shim
until that cloud gate is accepted.
