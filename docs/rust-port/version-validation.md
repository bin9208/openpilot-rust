# Native runtime build metadata and cached Git helpers

Issue [#73](https://github.com/bin9208/openpilot-rust/issues/73), under full runtime
issue [#1](https://github.com/bin9208/openpilot-rust/issues/1). This is a reusable
library increment. No manager selection or device startup path is switched, and
it is not a complete runtime candidate or evidence of CPU savings.

## Source and API contract

The MIT-licensed source at prerequisite head
`af181858afd55e92403807b7b23d3a0aa4633b52` remains unchanged:

- `openpilot/system/version.py` SHA-256
  `69b91ce6fc8fe3b164c4f6c23e9514c89a4b6759043cfb607b733c7e006144d7`.
- `openpilot/common/git.py` SHA-256
  `68a68daab8137ba307753b6f1dbc18a4a8c736ed118383800ad6bfd8690140d9`.
- `openpilot/common/utils.py` SHA-256
  `b1cc68b63cff1386ba397c0cc527dbb38fb50b154043ffcb066ed7acce65ec83`.
  The oracle executes its original `run_cmd`/`run_cmd_default` AST definitions;
  unrelated optional imports are isolated. Git and version modules are loaded
  directly from their unchanged files.

`openpilot-runtime-version` exposes build metadata, version/release text,
prebuilt/dirty policy, and cached Git helpers. Callers supply the checkout path;
there is no compile-time workstation root baked into runtime behavior.

- Existing `build.json` takes precedence, including when it is invalid. Missing
  fields default to `unknown`; `is_dirty` from a build file is always false.
  Otherwise an existing `.git` directory **or file** selects source discovery.
- The source dataclasses do not enforce field types. Metadata fields retain
  arbitrary Python JSON values in immutable `JsonValue` views, including large
  integers, nonfinite floats, ordered objects and lone Unicode surrogates.
  String-only property failures occur when accessed, not during construction.
  Canonical/UI properties use Python string/repr semantics; commit slicing also
  accepts lists and raises a key error for dictionaries under Python 3.12.
  `to_utf8()` returns `None` for nonstrings or lone surrogates without replacement
  decoding. `to_json()` preserves those values. Unicode repr classification is
  pinned to Unicode 15.0, matching the repository's required Python 3.12.
- Version extraction takes the first quoted field. Release notes stop at the
  first double newline. File and command text uses strict UTF-8 and universal
  newline conversion. Origin normalization preserves the ordered replace-once
  behavior, including matches away from URL prefixes and suffixes.
- Git success values and nonzero-exit empty defaults remain cached for the
  process; spawn, cwd and UTF-8 errors are not cached. Lexical path spelling is
  retained in cache keys. `get_head`/`get_head_date` model the source callsites
  with omitted revision arguments, and `_default` Git helpers model omitted cwd
  arguments separately from explicit `None`. Rust does not expose additional
  Python positional/keyword calling conventions or their cache-key distinctions.
- `is_dirty(path)` intentionally calls origin/branch helpers **without passing
  path**, then diffs `path` against the cached process-cwd tracking branch. This
  inherited behavior is covered in both directions. A valid origin/branch plus
  prebuilt marker bypasses the diff. Untracked files are ignored. Nonzero diff
  exits, including missing tracking refs, count as dirty. Git's shared index is
  not refreshed or locked by the dirty check.
- Missing both metadata sources emits the existing error message through the
  native Python-compatible logging producer, then returns `InvalidMetadata`.
  Missing/invalid build files and ordinary Git default returns do not introduce
  new cloud events. The source dirty check's `CalledProcessError` handler is not
  reachable through its real helpers: they consume that error, and
  `subprocess.call` returns a status. No synthetic exception is injected to claim
  that unreachable logging branch was exercised.

The shared logmessaged JSON addition is an immutable view over its existing
parser and writer. It exposes neither mutable arena indices nor a second parser.
Git remains an external native executable; logging uses libzmq and currently
brings in the existing original C++ msgq dependency through logmessaged. These
are explicit dependencies, not hidden Python execution. Project-owned C++ msgq
replacement and runtime daemon integration remain separate work.

## Reproducible verification

Run from the repository root with the pinned Rust toolchain:

```sh
CARGO_INCREMENTAL=0 cargo test --manifest-path rust/Cargo.toml -p openpilot-runtime-version -p openpilot-logmessaged --all-targets --locked
CARGO_INCREMENTAL=0 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-runtime-version -p openpilot-logmessaged --all-targets --locked -- -D warnings
cargo fmt --manifest-path rust/Cargo.toml -p openpilot-runtime-version -p openpilot-logmessaged -- --check
CARGO_INCREMENTAL=0 cargo build --manifest-path rust/Cargo.toml -p openpilot-runtime-version --example version_trace --locked
VERSION_EVIDENCE=.omo/evidence/runtime-version-73/reference uv run --no-project --python 3.12 rust/tools/check_version_reference.py rust/target/debug/examples/version_trace
```

The oracle uses temporary repositories, fixture-local Git identity/config, real
commits, refs and file changes. It disables global/system Git configuration in
its children, never updates the real checkout's config, and performs no network
Git operations. It compares values and exception classes, not platform-specific
exception prose. The line probe is test tooling, not a production daemon.

Host evidence covers 171 source comparisons: clean, modified, staged, untracked,
prebuilt, missing-origin, tracking/nontracking, detached, alternate tracking
remote, `.git` file, build JSON priority/defaults/malformed/type boundaries,
channel/property behavior, cache changes, cwd oddity, invalid UTF-8, missing Git,
missing cwd, permission errors, symlink loops and recovery. Six new Rust tests
exercise the public metadata and immutable JSON API, in addition to the existing
logmessaged regression suite.

`check_version_collector.py PROBE COLLECTOR OUTPUT` additionally uses the existing
built Python msgq binding and original cereal reader for the oracle transport.
It runs the native collector in a unique IPC/shared-memory namespace and checks
one disk record plus one valid `logMessage` and `errorLogMessage` against a fresh
execution of the original source error event. Its `PYTHONPATH` must include the
built msgq binding, repository root and `rust/tools`. This Python is test-only.

Local execution records, original-source hashes, binary hashes, JSONL comparisons
and collector captures are retained under
`.omo/evidence/runtime-version-73/`. The final evidence ledger identifies the
frozen commit and each exact invocation. CI/PR exact-head results remain the
integration owner's next gate; this increment does not claim a cloud run.

## Generic ARM boundary and remaining work

The targeted library/probe builds as generic `aarch64-unknown-linux-gnu.2.28`
using cargo-zigbuild. QEMU user-mode runs the same oracle corpus. Its manifest
separately records matching comparisons and known spawn-error divergences:
missing cwd/executable may become child exit 127 instead of the host's
`FileNotFoundError`; the source-compatible native empty-result cache then
retains that emulator-produced fallback. The host corpus verifies the actual
error/recovery contract. Do not count those QEMU cases as passing parity or
change runtime error handling to hide the emulator behavior.

No C3X, vehicle, AGNOS startup, normal upload-path comparison or performance test
was performed. Generic ARM artifacts are not installation packages. The first
user device comparison remains gated on the complete project-owned runtime
candidate, including normal startup and existing log upload.
