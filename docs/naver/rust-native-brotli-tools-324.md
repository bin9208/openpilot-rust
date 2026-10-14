# Native trusted Brotli provider staging (#324)

Scope: replace `rust/tools/stage_brotli_provider.py` with the native dev-tools `stage-brotli-provider`, preserving three external SDK libraries, target/version checks, containment, hashes, ordered manifest bytes and JSON report semantics. Codec implementation/SDK binaries remain explicit external dependencies; retain the SDK MIT license/provenance in package notices.

## Original oracle and design

Before replacement, a preserved unchanged copy staged synthetic valid x86_64 and aarch64 ELF fixtures (64/65/66 bytes) and the actual trusted AGNOS SDK Brotli1.1.0 libraries (858800 bytes total). Original synthetic manifest bytes are pinned under `tests/fixtures/brotli`; original real output/report remain ignored in `.analysis/scratch/2026-10-14-native-brotli-324/`. Original source follows within-directory symlinks and emits files in encoder, decoder, common order. README is the only discovered CLI caller; no workflow currently invokes this script.

The native design uses Serialize structs for both manifest and its three named file entries, preserving insertion order regardless of serde_json `preserve_order`. Admit metadata and bounded files before allocation, use checked growth plus1MiB with the shared25GiB reserve, freeze all input bytes/hashes before creating the package, then publish an entire adjacent temporary directory using no-replace atomic rename. Existing destinations, including dangling symlinks and empty directories, cannot be clobbered; a losing concurrent publisher cleans its temporary tree.

## Host validation (2026-10-14)

Canonical SDK environment, coordinated target cache, jobs=2 and incremental disabled. Each build passed the25GiB plus1GiB growth guard (over372GiB free).

- Observed RED: actual focused native parity test reached the fail-closed stub and failed with `Brotli staging not implemented` before implementation.
- `cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --test brotli_stage`: nine pass. Covers both target goldens/report fields, source containment/internal symlinks, malformed target/version/ELF, missing/nonregular sources,1TiB sparse oversized input, exact16MiB boundary, packed-version boundaries, existing file/directory/dangling-symlink destinations, concurrent publication/temporary cleanup and native CLI with empty PATH.
- Same nine tests with `--features serde_json/preserve_order`: pass; manifest bytes remain exact original output in both map modes.
- `cargo clippy --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --all-targets -- -D warnings`: pass in default mode and with `--features serde_json/preserve_order`.
- `cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --bin stage-brotli-provider`: pass.
- Actual CLI parity with both synthetic targets and trustedAGNOS SDK: all three libraries plus manifest are byte-identical to the unchanged original; all file modes equal, output names/regular files equal, and report manifest/byte count equal (synthetic195 bytes each; actual SDK858800 bytes). Bundle report path matches its new canonical output directory.

Only after these gates passed, README's sole caller migrated and the old Python script was removed. No wrapper or Python subprocess replaces it. The unchanged original capture remains ignored for reproducibility. Full dev-tools suite was124/124 during#322 before this new isolated module; the#324 gate is focused9/9 plus all-target strictClippy. The next integrated full-suite/cloud run remains parent-owned, as does actual target delivery.

Native admission checks metadata before any file-content allocation, then bounds reads and rejects changed sizes before publishing. This does not claim protection against arbitrary concurrent replacement of source-directory ancestors. Non-UTF-8 output paths fail before package publication; ordinary Unicode and spaced paths use the native OS path APIs. The Brotli implementation and same-target SDK/library license remain external requirements. No target/runtime/device acceptance or CPU claim.
