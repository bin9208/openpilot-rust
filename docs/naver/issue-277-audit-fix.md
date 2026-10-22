# Issue #277 — exclude ZIP directories from native torque staging

Issue: https://github.com/bin9208/openpilot-rust/issues/277. Coordinated batch:
#497, `codex/fix-497-stale-runtime-defects`, base
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`. No commit/publication by this worker.

## Cause and narrow change

`torque_stage::stage` explicitly exempted `numpy.libs/` directories from its
ZIP directory skip. Flattening converted that member to the regular file
`numpy.libs`, including its empty-file hash in the native manifest. Nested
library directories could likewise become pseudo-files or output collisions.
The native replacement had intentionally retained this old behavior.

Skip every directory, including Unix directory-mode entries, after the existing
safe-name, uniqueness and special-member checks. File selection, archive/member
size limits, exact locked filename/version/SHA256/size checks, ILP64 selection,
manifest format/order, license extraction, provenance and atomic publication
remain unchanged. The intentional source-compatibility exception is only the
absence of directory-derived pseudo-files and their manifest rows. This
supersedes the directory-marker compatibility paragraph in
`rust-native-torque-staging-296.md`; its historical observation remains intact.
No numerical output tolerance or provider identity is waived.

Tests were changed before production. The focused runtime test stages an actual
synthetic ZIP through `stage`, covering library, nested library, license and
unselected directory entries, both real file payloads, hashes, licenses and
provenance. Existing negative-admission and empty-PATH CLI tests remain. An
explicitly requested ignored test stages the pinned real ARM wheel, compares
every native/license byte with its ZIP member, verifies exactly two native
manifest rows and their hashes, and checks wheel provenance. ARM wheel SHA256:
`76c2c1e6bfa5c84adc6434dfbf013aa92096a7985221762c8f11fedfd20fff58`.

## Hosted verification recipe (not executed locally)

Run from repository root on a GitHub-hosted Linux runner with Rust 1.94.0 and
sufficient space for the existing disk reserve. Preserve the candidate source,
restore only the production file from the base, retain the candidate tests,
and expect the selected regression to fail at a runtime assertion. Restore the
candidate before GREEN; keep this mutation inside the disposable Actions checkout.

```bash
cp rust/crates/dev-tools/src/torque_stage.rs "$RUNNER_TEMP/torque-stage-candidate.rs"
git show b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:rust/crates/dev-tools/src/torque_stage.rs > rust/crates/dev-tools/src/torque_stage.rs
cargo +1.94.0 test --locked --manifest-path rust/Cargo.toml -p openpilot-dev-tools --test torque_stage directories_are_excluded_and_native_bytes_hashes_licenses_and_provenance_are_preserved -- --exact
# Parent records expected nonzero RED, then restores candidate even on failure.
cp "$RUNNER_TEMP/torque-stage-candidate.rs" rust/crates/dev-tools/src/torque_stage.rs
cargo +1.94.0 test --locked --manifest-path rust/Cargo.toml -p openpilot-dev-tools --test torque_stage
export NUMPY_ARM_WHEEL="$RUNNER_TEMP/numpy-2.5.3-cp312-cp312-manylinux_2_27_aarch64.manylinux_2_28_aarch64.whl"
curl --fail --location --retry 5 --retry-all-errors --output "$NUMPY_ARM_WHEEL" https://files.pythonhosted.org/packages/f1/32/84146fc020ad3c25f805f70ab60da46fe3c540a21369754a7e4369754b6f/numpy-2.5.3-cp312-cp312-manylinux_2_27_aarch64.manylinux_2_28_aarch64.whl
cargo +1.94.0 test --locked --manifest-path rust/Cargo.toml -p openpilot-dev-tools --test torque_stage pinned_arm_wheel_retains_exactly_two_native_libraries_and_all_license_bytes -- --ignored --exact
```

Local work was limited to source inspection, Rust formatting, Python syntax
parsing of companion tests, and scoped diff review. RED, GREEN and real-wheel
acceptance await the parent's exact-source hosted results. No device result or
release acceptance is claimed.
