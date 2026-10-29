#!/usr/bin/env bash
# #630 전용 Actions 집중 검사. 로컬 실행 금지.
set -euo pipefail
: "${SOURCE_ROOT:?exact candidate checkout required}"
: "${CARGO_TARGET_DIR:?private Actions target required}"
: "${EVIDENCE:?private Actions evidence required}"
: "${SOURCE_PYTHON:?private source Python required}"
[[ ${GITHUB_ACTIONS:-} == true && ${RUNNER_NAME:-} == bin-actions2 ]] || exit 1
cd -- "$SOURCE_ROOT"
mkdir -p -- "$EVIDENCE/packages"
export MODEM_SOURCE_PYTHON="$SOURCE_PYTHON"
export PYTHONPATH="$SOURCE_ROOT:$SOURCE_ROOT/rust/tools${PYTHONPATH:+:$PYTHONPATH}"
export MODEM_PROCESS_CHILD_BINARY="$CARGO_TARGET_DIR/debug/openpilot-process-child"
export WIFI_PROCESS_CHILD_BINARY="$MODEM_PROCESS_CHILD_BINARY"
packages=(-p openpilot-web-upload -p openpilot-bootlog -p openpilot-tombstoned
  -p openpilot-soundd -p openpilot-bluetooth -p openpilot-wifi -p openpilot-modem
  -p openpilot-lpa -p openpilot-process-supervision)
cargo build --manifest-path rust/Cargo.toml --locked "${packages[@]}" --bins --examples 2>&1 | tee "$EVIDENCE/packages/build.log"
cargo test --manifest-path rust/Cargo.toml --locked "${packages[@]}" --all-targets \
  --features openpilot-modem/helper-lifetime-tests -- --nocapture 2>&1 | tee "$EVIDENCE/packages/tests.log"
cargo clippy --manifest-path rust/Cargo.toml --locked "${packages[@]}" --all-targets \
  --features openpilot-modem/helper-lifetime-tests -- -D warnings 2>&1 | tee "$EVIDENCE/packages/clippy.log"
rustc --edition=2021 -O -D warnings rust/tools/lpa_cookie_header_allocation_probe.rs -o "$EVIDENCE/packages/lpa-cookie-probe"
"$EVIDENCE/packages/lpa-cookie-probe" --require-improvement | tee "$EVIDENCE/packages/lpa-cookie-allocation.jsonl"
wifi_rlib=$(find "$CARGO_TARGET_DIR/debug/deps" -maxdepth 1 -type f -name 'libopenpilot_wifi-*.rlib' -print -quit)
[[ -n $wifi_rlib ]]
rustc --edition=2021 -O -D warnings rust/tools/wifi_snapshot_allocation_probe.rs \
  --extern "openpilot_wifi=$wifi_rlib" -L "dependency=$CARGO_TARGET_DIR/debug/deps" \
  -o "$EVIDENCE/packages/wifi-snapshot-probe"
"$EVIDENCE/packages/wifi-snapshot-probe" --require-improvement | tee "$EVIDENCE/packages/wifi-snapshot-allocation.jsonl"
sha256sum "$EVIDENCE/packages/lpa-cookie-probe" "$EVIDENCE/packages/wifi-snapshot-probe" > "$EVIDENCE/packages/probes.sha256"
git diff --exit-code
git diff --cached --exit-code
printf 'package build/tests/strict Clippy and both production allocation probes completed\n' > "$EVIDENCE/packages/SUCCESS"
