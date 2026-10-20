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

The independent original observation is
`original-linux-x86_64-files.json` (690 relative path/SHA-256 pairs; file
SHA-256 `8a4307258256f1357d60f013ddecc719d8919dc6ff0163e8346aca6d2da396a2`).
It was copied byte-for-byte from `eigen/x86_64-comparison/original-files.json`
in GitHub Actions artifact `12300415646`, run `39500380365`, candidate
`4991f8f569b50adf78e498396e246e8b68aadc68`. The downloaded artifact ZIP
SHA-256 was `877b1fd589b4bef46905dbad1e7d26876ec65402af698d863db28d99fa62ecaf`.
The original install log SHA-256 was
`fc95c039d5289379a62a057739a04f9eb914ab733d05799b4005879f7a46edb8`;
it records a fresh **Linux x86_64** `pip install` of the unchanged shim from
`commaai/dependencies@40e5d76de1b33a86c5181b63db6782d8f06da1da`.
The artifact's three `original-files.json` entries were byte-identical to
this map. Each comparison record reported the same original Linux install
path and complete equality with that architecture's separately staged native
headers. This does **not** claim original Python installs on ARM or macOS.
The Eigen comparison and strict Clippy passed in the estimation job; a later,
unrelated paramsd gate compile error made that overall job fail.

The Rust reference test pins the original-map bytes in code. It requires
`EIGEN_WHEEL`, `EIGEN_ARCHITECTURE`, `EIGEN_NATIVE_INCLUDE` and `EIGEN_EVIDENCE`;
it independently stages the supplied pinned wheel and compares every relative
path and file SHA-256 with both the sealed original map and the separately
staged native tree. Wrong wheel/architecture, symlink input, malformed ZIP and
output clobber controls remain required. The comparison receipt records the
original observation and artifact identities without labelling a native tree
as original. Only tests and fixture data change here; existing Eigen callers
remain unchanged until the parent integrates and validates their native header
acquisition in GitHub Actions. No local staging, build or functional test ran.
