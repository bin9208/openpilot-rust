#!/usr/bin/env bash
# #431: 동일 계측 소스로 고정 기준과 현재 실제 subscriber를 비교한다.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root"
baseline_sha=e445c9d19c3041bc79d8624589e2eb6ba7d061c6
work=$(mktemp -d "${RUNNER_TEMP:?}/messaging-allocation-431.XXXXXXXX")
proof="$RUNNER_TEMP/messaging-allocation-431/evidence"
mkdir -p "$proof"
baseline="$work/baseline"
probe=rust/tools/messaging_update_allocation_probe.rs
git diff --exit-code HEAD -- rust tools/ci/messaging-allocation-431.sh
git rev-parse HEAD > "$proof/current.SOURCE_COMMIT"
git rev-parse 'HEAD^{tree}' > "$proof/current.SOURCE_TREE"
rustc -vV > "$proof/rustc.txt"
cargo -vV > "$proof/cargo.txt"
uname -m > "$proof/architecture.txt"
sha256sum "$probe" rust/Cargo.toml rust/Cargo.lock \
  rust/crates/messaging/src/{runtime,state}.rs \
  rust/crates/messaging/tests/state.rs > "$proof/source.sha256"

git fetch --no-tags --depth=1 origin "$baseline_sha"
GIT_LFS_SKIP_SMUDGE=1 git worktree add --detach "$baseline" "$baseline_sha"
git -C "$baseline" rev-parse HEAD > "$proof/old.SOURCE_COMMIT"
test "$(cat "$proof/old.SOURCE_COMMIT")" = "$baseline_sha"

for role in old current; do
  if [[ $role == old ]]; then source_root="$baseline"; else source_root="$root"; fi
  target="$work/$role-target"
  CARGO_TARGET_DIR="$target" cargo build --manifest-path "$source_root/rust/Cargo.toml" \
    -p openpilot-messaging --lib --locked 2>&1 | tee "$proof/$role-build.log"
  shopt -s nullglob
  cereal=("$target"/debug/deps/libopenpilot_cereal-*.rlib)
  capnp=("$target"/debug/deps/libcapnp-*.rlib)
  test "${#cereal[@]}" -eq 1
  test "${#capnp[@]}" -eq 1
  sha256sum "$target/debug/libopenpilot_messaging.rlib" "${cereal[0]}" \
    "${capnp[0]}" > "$proof/$role-inputs.sha256"
  rustc --edition=2021 -C debuginfo=0 -D warnings "$probe" \
    --extern "openpilot_messaging=$target/debug/libopenpilot_messaging.rlib" \
    --extern "openpilot_cereal=${cereal[0]}" --extern "capnp=${capnp[0]}" \
    -L "dependency=$target/debug/deps" -o "$proof/$role-probe"
  sha256sum "$proof/$role-probe" > "$proof/$role-executable.sha256"
  timeout --kill-after=5s 45s "$proof/$role-probe" > "$proof/$role.log" 2> "$proof/$role.stderr"
  sha256sum -c "$proof/$role-executable.sha256"
  sha256sum -c "$proof/$role-inputs.sha256"
done

"$proof/current-probe" --compare "$proof/old.log" "$proof/current.log" | tee "$proof/comparison.log"
# 개선 없는 기준/기준 비교가 실패해야 계측 gate를 유효하게 본다.
if "$proof/current-probe" --compare "$proof/old.log" "$proof/old.log" \
    > "$proof/no-improvement.log" 2>&1; then
  printf 'unchanged allocation unexpectedly passed\n' >&2
  exit 1
fi
grep -F 'did not improve in calls and bytes' "$proof/no-improvement.log"

export CARGO_TARGET_DIR="$work/current-target"
cargo test --manifest-path rust/Cargo.toml -p openpilot-messaging --all-targets --locked \
  2>&1 | tee "$proof/tests.log"
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-messaging --all-targets --locked \
  -- -D warnings 2>&1 | tee "$proof/clippy.log"
cargo build --manifest-path rust/Cargo.toml -p openpilot-messaging --example state_probe --locked \
  2>&1 | tee "$proof/state-build.log"
python rust/tools/check_message_state.py \
  --binary "$CARGO_TARGET_DIR/debug/examples/state_probe" --output "$proof/source-comparison" \
  2>&1 | tee "$proof/source-comparison.log"
sha256sum "$CARGO_TARGET_DIR/debug/examples/state_probe" > "$proof/state-probe.sha256"
sha256sum -c "$proof/source.sha256"
git diff --exit-code HEAD -- rust tools/ci/messaging-allocation-431.sh
git -C "$baseline" diff --exit-code HEAD -- rust
printf '검증 완료: 실제 subscriber 할당·메시징 회귀·원본 상태 대조\n'
