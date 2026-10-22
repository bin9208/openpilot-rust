# Issue #345 — JPEG CMake source identity across checkouts

Issue: https://github.com/bin9208/openpilot-rust/issues/345. Coordinated batch:
#497, `codex/fix-497-stale-runtime-defects`, base
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`. No commit/publication by this worker.

## Cause and narrow change

Cargo can reuse one build-script `OUT_DIR` across source checkouts. The JPEG
script always configures `OUT_DIR/jpeg`; its CMake cache retains the first
absolute `CMAKE_HOME_DIRECTORY`. A second checkout then fails CMake's source
identity check before any JPEG compilation.

Read that identity immediately before configuration. If it differs from the
current `native/vendor` path, remove only `OUT_DIR/jpeg`, then use the existing
configure/build commands. Same-checkout builds retain the cache. Missing cache
is normal; unreadable caches and genuine configure/build errors still fail.
Vendor code, compiler selection, host/ARM options, source metadata, link/include
paths, provider provenance and surrounding disk checks remain unchanged. No
blanket Cargo clean or additional cache accumulation is introduced.

`rust/crates/jpeg/tests/build_cache.py` was written before the fix. It runs the
actual compiled Cargo build script, including its cc/cxx steps, against two tiny
CMake source trees sharing one `OUT_DIR`. Both source paths contain spaces; the
first checkout is removed before switching to the second. It requires a real
static archive and matching CMake source identity, preserves a same-source cache
marker and a sibling output, and requires an intentional CMake error to propagate.
It can exercise both actual host and ARM compilers without a large provider.

## Hosted verification recipe (not executed locally)

Use GitHub-hosted Ubuntu with Rust 1.94.0, CMake, native C/C++ toolchains and
`gcc-aarch64-linux-gnu`, `g++-aarch64-linux-gnu` for the ARM lane. Execute from
repository root. Keep candidate tests when restoring original production source.
The parent must handle expected RED nonzero exit and restore the candidate in a
finally/trap step inside the disposable Actions checkout.

```bash
cp rust/crates/jpeg/build.rs "$RUNNER_TEMP/jpeg-build-candidate.rs"
git show b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:rust/crates/jpeg/build.rs > rust/crates/jpeg/build.rs
cargo +1.94.0 build --locked --manifest-path rust/Cargo.toml -p openpilot-jpeg --target-dir "$RUNNER_TEMP/jpeg-345-red"
python3 rust/crates/jpeg/tests/build_cache.py --build-script "$(find "$RUNNER_TEMP/jpeg-345-red/debug/build" -path '*/openpilot-jpeg-*/build-script-build' -type f)"
# Expected RED is the second checkout's actual CMake source mismatch.
cp "$RUNNER_TEMP/jpeg-build-candidate.rs" rust/crates/jpeg/build.rs
cargo +1.94.0 test --locked --manifest-path rust/Cargo.toml -p openpilot-jpeg --target-dir "$RUNNER_TEMP/jpeg-345-green"
python3 rust/crates/jpeg/tests/build_cache.py --build-script "$(find "$RUNNER_TEMP/jpeg-345-green/debug/build" -path '*/openpilot-jpeg-*/build-script-build' -type f)"
rustup target add --toolchain 1.94.0 aarch64-unknown-linux-gnu
export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
cargo +1.94.0 build --locked --manifest-path rust/Cargo.toml -p openpilot-jpeg --target aarch64-unknown-linux-gnu --target-dir "$RUNNER_TEMP/jpeg-345-arm"
python3 rust/crates/jpeg/tests/build_cache.py --target aarch64-unknown-linux-gnu --build-script "$(find "$RUNNER_TEMP/jpeg-345-arm/debug/build" -path '*/openpilot-jpeg-*/build-script-build' -type f)"
```

Local formatting, Python syntax and scoped diff review pass. Actual two-checkout
RED/GREEN, existing JPEG runtime regressions and ARM build acceptance await the
parent's exact-source hosted results. No CPU or device benefit is claimed.
