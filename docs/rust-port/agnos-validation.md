# Native AGNOS image updater

Issue [#119](https://github.com/bin9208/openpilot-rust/issues/119), under full-runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1). Sources: unchanged `openpilot/system/hardware/tici/agnos.py` and the image paths in `openpilot/system/updated/casync/casync.py`, based on `bb421033`. Original source history and licensing remain intact. Implementation: `rust/crates/agnos`, including `openpilot-agnos`.

The runtime retains inactive-slot selection, the nonblocking updater flock, confirmation digest helpers, resumable compressed cache, compressed/uncompressed/raw hashes and size checks, sparse raw/fill/don't-care decoding, post-flash markers, verification attempts, and slot-swap success checks. A regular file is truncated by the source's `wb+` opens; the native implementation retains that behavior. Tests distinguish it from block-device semantics instead of treating a regular file as a faithful block-device emulator.

Casync retains image-index parsing, first-occurrence chunk lookup, seed/target/remote order, SHA-512/256 validation, remote retry delays, progress/statistics and final full-image verification. Directory/tar casync helpers are outside this image-only slice and remain explicit unported scope. The source's index flags check is a tautology; the native parser does not invent a new flags requirement.

Network behavior retains the 10-second connect/60-second image-read deadlines, 120-second caibx fetch policy, three remote-chunk attempts with 60-second retry delays, and five ordinary image-download attempts with 10-second delays. `--retry-network` continues transient connection/server failures beyond five attempts while certificate errors, invalid URLs and permanent HTTP failures still stop after five. Streaming image status errors are deferred until reading starts, retaining the source's target-open ordering. Native TLS errors wrapped in I/O errors are classified as non-transient certificate failures. Persistent casync cookies are sent as matched name/value pairs, avoiding the existing ureq 3.4.2 attribute-serialization bug; retry-cookie parity was checked in the focused casync rerun. Optional cache metadata with unsupported JSON types is ignored as in the source; verification returns false for unsupported size/hash types. Manifest URL-origin helpers preserve authority spelling and relative URLs.

The production CLI accepts `--verify`, `--swap`, `--retry-network` and a manifest path; `--verify` has precedence when both mode flags are given. An additional `--config` JSON path supplies owned fixture paths/executables. Defaults still address the device's original locations. Verification/swap loops preserve three flash attempts and four verification checks. Progress is clamped to 0–100 on stdout, with logging on stderr. The updater reports the source's rebooting progress text but does not itself call reboot.

Public integration seams are `runtime::target_slot`, `runtime::verify`, `runtime::flash` and `runtime::swap`, with `manifest::Paths`, `runtime::NativeCommands`, and `Observer`. The parent integration owns wiring into updated #118 and the startup UI. Production selection is unchanged here.

## Focused validation

Only owned regular files, generated fixture data, local HTTP/TLS peers, and a harmless `abctl` executable are used. No block device, actual boot-slot command, sysfs update, carrier, vehicle or C3X was accessed.

```sh
RUSTUP_TOOLCHAIN=1.94.0 cargo build --manifest-path rust/Cargo.toml -p openpilot-agnos -p openpilot-process-supervision --bins --examples --locked -j2
RUSTUP_TOOLCHAIN=1.94.0 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-agnos --all-targets --locked -j2 -- -D warnings
python rust/tools/check_agnos.py --target rust/target --evidence /tmp/agnos-evidence
```

The oracle environment needs requests, pycryptodome and the existing repository Python test dependencies; OpenSSL generates the untrusted local certificate. Native runtime execution never calls Python. The gate imports the actual source, substitutes only external owned paths/executables and controlled sleep observations, and executes the unchanged source CLI body for the CLI comparison.

Ten scenarios passed locally:

- Helpers: confirmation digest/write/remove, case-preserving origin deduplication, relative URLs and inactive-slot targeting.
- Download: cache Range resume, ignored-Range restart, multi-megabyte raw data, sparse raw/fill/don't-care output, cache removal, and large highly compressed data read in small pieces.
- Markers: full versus marker verification, source-compatible regular-file truncation, and swap output/retry behavior.
- Transient retry: five HTTP 503 failures followed by successful attempt six with `--retry-network`.
- Permanent failure: HTTP 404 stops after five attempts.
- Corruption: compressed hash rejection/removal, raw hash rejection/cache retention, incomplete HTTP body, and invalid-size verification.
- Casync: seed/target/remote reconstruction, duplicate chunk reuse, SHA-512/256 and two failed remote downloads before success.
- Invalid URL: five bounded non-transient failures.
- Actual CLI: duplicate-lock rejection, native executable identity, source/native stdout/commands/files, standalone flash despite casync fields, verification, three slot-switch attempts, exit zero and lock reacquisition.
- TLS: an untrusted certificate remains a bounded non-transient failure; no HTTP request reaches the server.

The nine library comparisons assert identical result/error classifications, progress/log/sleep events, HTTP Range requests, command arguments and file sizes/hashes. The actual CLI comparison also checks stdout and exit behavior. Request-library-specific exception wording and Python tracebacks are not treated as identical Rust diagnostics. The production timing constants are preserved; library retry sleeps are recorded through `Observer` rather than waited in fixtures. CLI swap sleeps execute normally.

After the ten-scenario run, final lint removed one needless hash-comparison reference and the persistent-cookie workaround was added. Only the affected casync scenario was rerun locally; integration Actions own the final broad run.

Evidence: `.omo/evidence/agnos-119/` in the parent workspace contains scenario `*-source.json`/`*-native.json`, stderr captures, `results.json`, `native-executable.json`, build/clippy/fmt/Ruff logs and source/binary/dependency hashes. Expected loopback connection resets can appear in the first oracle server log after clients reject HTTP responses; the scenario assertions still require complete transcripts and matching final artifacts.

## Dependencies and limits

New registry packages are only pinned `xz2 0.1.7` and its `lzma-sys 0.1.20` binding with static liblzma. Existing lockfile versions remain unchanged. Native HTTP/TLS, liblzma, Linux filesystem/flock/sync, `abctl`, `/bin/sh`, the process launcher and its existing cereal/msgq/libzmq dependency chain remain explicit external/native dependencies.

Generic ARM compilation and exact-SHA Actions validation belong to integration. Host fixtures do not establish block-device behavior, AGNOS ABI compatibility, live update/reboot safety, complete startup/log upload, device acceptance or CPU/thermal savings. This slice is not a first-device-test handoff.
