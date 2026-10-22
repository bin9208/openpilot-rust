# Experimental Rust port

This independent repository ports the openpilot runtime and development tools to
Rust. Repository-wide Python retirement is still in progress; current acceptance
and outstanding work are recorded in the [#296 evidence ledger](../../naver/rust-repository-python-retirement-296-evidence.md).

On a Linux development host, with Rust 1.94.0 installed, run from `rust/`:

```sh
cargo run --release --locked --bin cpu-sample -- 1000
```

The optional interval is 1–60000 milliseconds (default 1000). Output is TSV.
100% means one logical core; values above 100% are valid for multi-threaded
processes. The processor column is the last observed main-thread core, not the
process's affinity or the placement of every thread. Unreadable/exited processes
and invalid counter deltas are omitted. Short intervals have coarse resolution.

This is a one-shot diagnostic, not a background service. It does not publish
vehicle commands. Host tests and generic aarch64 builds do not validate AGNOS
compatibility, CPU savings, temperature improvements, or safe vehicle operation.
Do not deploy the generic CI build to a vehicle; target validation remains open.

The teleoprtc release script also uses Rust for version computations. From the repository root,
prepare it with `cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --features teleop-version-native --bin teleop-version`.
Set `TELEOP_VERSION_BIN` when using a binary installed elsewhere.
`project.version` must be a string; existing branch checks, version sorting, commits and tags remain unchanged.

`ci-receipt-metadata` reads CI Params module paths and JSON11 receipt prefixes.
Build it with `cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --features ci-receipt-metadata-native --bin ci-receipt-metadata`.
It supports `module ENV FILE`, `first-binding ENV DIRECTORY`, and `prefix FILE`.
Receipts are limited to 4MiB; JSON11 `prefix` must be a string. CI builds the
native binary for each host first and exports its path as `CI_RECEIPT_METADATA`.

The experimental `offline-cached-code` tool packages and checks complete original
CPU program images on x86-64 Linux. Build it with
`cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-model-runtime --features cached-program-isa --bin offline-cached-code`.
Its explicit request path and SHA256 bind original and current source/issuer
evidence; it does not compile model source or execute the images. Output paths
must be new files. This candidate still requires numerical acceptance.

Cached bundles require explicit `--cached-admission PATH --cached-admission-sha256 SHA256`
arguments in `model-run --trusted-bundle BUNDLE SEQUENCE.json` or
`openpilot-dmonitoringmodeld --trusted-catalog CATALOG`. Obtain that admission
from independently trusted original/current evidence, separately from the bundle.
Ordinary loading rejects cached bundles; this option does not enable vehicle
installation or change the normal runtime selection.

Development upload checks use `check-uploader-reference`, `check-uploader-logging`,
`check-uploader-siblings`, `check-manager-log-upload`, and `check-runtime-log-upload`.
The ten original Python controllers are removed; maintained CI builds the
required Rust features and executables before use. The final source-absence
acceptance for this transition is tracked in #296.
