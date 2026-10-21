#!/usr/bin/env bash
# Insert after the existing UI job's renderer/native builds and Xvfb startup.
# Cloud-only diagnostic: leave the main 20-case assertion unchanged and required.
set -euo pipefail
root="${GITHUB_WORKSPACE:?}"
proof="${RUNNER_TEMP:?}/ui-strict-fps-diagnostic"
mkdir -p "$proof"
test "$(git rev-parse HEAD)" = "${EXPECTED_SOURCE_SHA:?}"
test "$(sha256sum "$root/openpilot/system/ui/lib/application.py" | cut -d' ' -f1)" = d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00
git fetch --no-tags --depth=1 origin 76a83c8aede5626b868846cf8fe2c32acdf73ae1
git show 76a83c8aede5626b868846cf8fe2c32acdf73ae1:rust/tools/ui_qa/application_source.py > "$proof/application_source.original.py"
python -P "${DIAGNOSTIC_ASSETS:?}/instrument_original.py" "$proof/application_source.original.py" "$proof/application_source.diagnostic.py"
sha256sum "$proof"/application_source.*.py "$root/openpilot/system/ui/lib/application.py" > "$proof/sources.sha256"
{ uname -a; nproc; cat /proc/loadavg; cat /sys/fs/cgroup/cpu.max 2>/dev/null || true; taskset -pc "$$"; } > "$proof/host.txt"
env | grep -E '^(DISPLAY|GALLIUM_DRIVER|LIBGL_ALWAYS_SOFTWARE|LP_NUM_THREADS|STARTUP_UI_RAYLIB_LIBRARY)=' | sort > "$proof/render-env.txt"

# Same display, renderer and fixture environment as the mandatory native test.
export BIG=0 SCALE=1 PROFILE_RENDER=2 OFFSCREEN=1 STRICT_MODE=1
export PYTHONPATH="$root" PYTHONDONTWRITEBYTECODE=1
export UI_APPLICATION_FPS_DIAGNOSTIC_OUTPUT="$proof/original-fps.json"

# The required 20-case source test in the checkout is never changed. Only an
# isolated detached checkout receives a byte-pinned, test-only strict filter.
diagnostic_root="$RUNNER_TEMP/ui-strict-checkout"
git worktree add --detach "$diagnostic_root" "$EXPECTED_SOURCE_SHA"
test "$(git -C "$diagnostic_root" rev-parse HEAD)" = "$EXPECTED_SOURCE_SHA"
test -z "$(git -C "$diagnostic_root" status --porcelain)"
test_file="$diagnostic_root/rust/crates/dev-tools/tests/ui_application_lifecycle_reference.rs"
sha256sum "$test_file" > "$proof/native-test-before.sha256"
python -P "$DIAGNOSTIC_ASSETS/filter_test.py" "$test_file"
sha256sum "$test_file" > "$proof/native-test-after.sha256"
git -C "$diagnostic_root" diff -- rust/crates/dev-tools/tests/ui_application_lifecycle_reference.rs > "$proof/test-only.diff"
test "$(git -C "$diagnostic_root" diff --name-only | wc -l)" -eq 1
test "$(git -C "$diagnostic_root" diff --name-only)" = rust/crates/dev-tools/tests/ui_application_lifecycle_reference.rs
set +e
python -P "$proof/application_source.diagnostic.py" profile "$proof/original.png" > "$proof/original.stdout" 2> "$proof/original.stderr"
original_status=$?
UI_APPLICATION_DIAGNOSTIC_OUTPUT="$proof" UI_APPLICATION_FPS_DIAGNOSTIC=1 CARGO_TARGET_DIR="$RUNNER_TEMP/ui-strict-target" \
  cargo test --manifest-path "$diagnostic_root/rust/Cargo.toml" -p openpilot-dev-tools \
  --features ui-application-lifecycle-native --test ui_application_lifecycle_reference \
  --locked -- --exact twenty_original_environment_cases_preserve_admission_and_files --nocapture \
  > "$proof/native-test.stdout" 2> "$proof/native-test.stderr"
native_status=$?
set -e
printf 'original=%s native_test=%s\n' "$original_status" "$native_status" > "$proof/status.txt"
sha256sum "$proof"/*.png > "$proof/images.sha256" 2>/dev/null || true
find "$proof" -maxdepth 1 -type f -printf '%f %s bytes\n' | sort > "$proof/contents.txt"
cat "$proof/status.txt"

# A completed diagnostic requires actual source/native FPS observations; a
# compile error or test-only graft failure cannot masquerade as data.
test -s "$proof/original-fps.json"
test -s "$proof/native.exit"
grep -F 'UI_APPLICATION_FPS_DIAGNOSTIC frame=' "$proof/native.stderr"
! grep -F 'could not compile' "$proof/native-test.stderr"
git rev-parse HEAD > "$proof/source-commit.txt"
printf '%s\n' "$DIAGNOSTIC_WORKFLOW_SHA" > "$proof/workflow-commit.txt"
