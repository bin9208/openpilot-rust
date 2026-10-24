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
Host recording, upload and shutdown checks pass with these sources absent;
repository-wide conversion and device acceptance remain unfinished.

Development log-retention checks use `check-deleter` with `reference`, `daemon`,
and `observer` scopes. Build it with
`cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --no-default-features --features deleter-check-native --bin check-deleter`.
Maintained CI supplies the immutable original fixture and native providers;
the daemon check creates a private temporary mount through CI's passwordless
sudo. The four Python development helpers are removed and their source-absence,
functional and caller-policy checks pass. Repository-wide conversion remains
open in #296.

Carrot Navi CI builds `check-carrot-navi-comparison` in `openpilot-dev-tools`
with `--no-default-features --features carrot-navi-compare-native --bin check-carrot-navi-comparison`.
Run it with one capture-directory argument. It performs the existing exact policy,
socket, lifecycle and discovery comparisons and writes the normalized rows and
`comparison.json`. The independent original comparator is retained as a sealed
test fixture. Original runtime/QA traffic stays in CI; whole-family retirement
and device acceptance remain pending in #296.

The Linux `check-native-logging` candidate compares the Rust logger with the
original C++ producer, including packet bytes, console output and transport
failures. Build it with `--features native-logging-check-native --bin check-native-logging`
in `openpilot-dev-tools`. Its Python comparison helpers remain until native
functionality and source-absence checks pass.

The new GitHub Actions candidate `check-native-logging retained` compares sealed
private captures with a fresh Rust producer. It takes `--context`, `--source-commit`
and `--binary-sha256` alongside `--root`, `--binary` and a new `--output` directory.
This mode does not run the original producer and accepts neither `--builder` nor
`--reference`. Its 21 new tests, execution and shutdown checks remain pending.
Reading retained data or reviewing source does not complete whole logging
qualification or Python retirement.

The Linux `check-loggerd-native` candidate compares the original and Rust logger
with complete recorded messages, media, error paths and rotation timing. Build
it in `openpilot-dev-tools` with `--features loggerd-check-native --bin check-loggerd-native`.
It requires separate original/native providers and a new output directory.
Its functional and shutdown checks are pending; existing CI callers remain.

`publish-model-catalog` in `openpilot-model-runtime` publishes already emitted
bundles from a private staging directory. It checks existing generation contents
before reuse and atomically replaces the catalog pointer. Eight focused tests
and ten original/native filesystem comparisons pass in hosted CI. Compiler,
model execution and emitter caller migration remain separate validation gates.

Maintained fast checks use the native `check-changed-files` command for Python
3.12 source syntax and Git whitespace checks. Build it in `openpilot-dev-tools`
with `--no-default-features --features changed-files-native --bin check-changed-files`.
Run from the checkout with optional `--base SHA` or `BASE_SHA`; missing, empty or
all-zero bases cover the complete issue branch against `origin/dev`. Whitespace
is checked first, then changed regular `.py` files are parsed from current worktree
bytes. Symlinks and deleted files are skipped. Cancellation stops owned Git
process groups; unsupported source capabilities fail explicitly. Source retirement
requires the accepted codec/lifecycle evidence and a separate deletion verification.
This CI migration does not establish device acceptance or repository-wide retirement.

The `developer-auth-json` candidate in `openpilot-dev-tools` builds with
`--features developer-ssh-native --bin developer-auth-json` and accepts
`google|apple|github --json`. Running it opens a browser and saves a token to the
existing auth configuration after successful sign-in. Synthetic regression tests
run in Actions without authenticating. The original `auth.py` plain/JWT modes and
existing callers remain until their conversion is verified; this candidate does
not complete the authentication tool family.
