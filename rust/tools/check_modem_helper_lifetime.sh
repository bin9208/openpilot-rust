#!/usr/bin/env bash
# GitHub Actions 전용 #398 gate. 로컬 실행 금지: 빌드와 실제 자식 프로세스를 실행한다.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
: "${RUNNER_TEMP:?GitHub Actions RUNNER_TEMP required}"
: "${MODEM398_BASELINE_ROOT:?prepare immutable 1364e48c baseline outside candidate first}"
: "${MODEM_SOURCE_PYTHON:?Python 3.12 with unchanged-source prerequisites required}"
: "${MODEM_HELPER_EVIDENCE:?private Actions artifact directory required}"
baseline=$(cd -- "$MODEM398_BASELINE_ROOT" && pwd)
[[ "$baseline" != "$root" ]] || { echo 'baseline must differ from candidate' >&2; exit 1; }
[[ -d "$baseline/rust/crates/modem" ]] || { echo 'baseline modem crate missing' >&2; exit 1; }
# CI preparation must use the reviewed historical Rust source, not a rewritten policy.
for file in Cargo.toml src/lib.rs src/ppp.rs src/runtime.rs src/main.rs src/observe.rs; do
  git -C "$root" show "1364e48c96e888fa5d178c5eb2132d135cf30765:rust/crates/modem/$file" | cmp - "$baseline/rust/crates/modem/$file"
done
mkdir -p -- "$MODEM_HELPER_EVIDENCE"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/rust/target}"
# An existing Actions-owned operation growth admission must precede this script.
# Keep the historical 15 live tests, not the later captured JSON replacement.
mkdir -p -- "$baseline/rust/crates/modem/tests/fixtures"
cp -- "$root/rust/crates/modem/tests/helper_lifetime.rs" "$baseline/rust/crates/modem/tests/"
cp -- "$root/rust/crates/modem/tests/fixtures/helper-entry.sh" "$baseline/rust/crates/modem/tests/fixtures/"
export MODEM_PROCESS_CHILD_BINARY="$CARGO_TARGET_DIR/debug/openpilot-process-child"
cargo build --manifest-path "$baseline/rust/Cargo.toml" -p openpilot-modem -p openpilot-process-supervision --bins --locked
if cargo test --manifest-path "$baseline/rust/Cargo.toml" -p openpilot-modem --test helper_lifetime --locked -- --exact interface_deadline_includes_stdout_eof_after_leader_exit --nocapture 2>&1 | tee "$MODEM_HELPER_EVIDENCE/baseline.log"; then
  echo 'Expected the old modem EOF assertion to fail' >&2
  exit 1
fi
# A compile/import/fixture failure is not the required old-source RED.
grep -F 'test result: FAILED' "$MODEM_HELPER_EVIDENCE/baseline.log"
grep -F "must time out, not accept leader's valid inet line" "$MODEM_HELPER_EVIDENCE/baseline.log"
cargo build --manifest-path "$root/rust/Cargo.toml" -p openpilot-modem -p openpilot-process-supervision --bins --examples --locked
cargo test --manifest-path "$root/rust/Cargo.toml" -p openpilot-process-supervision --test session_redirected_630 --locked 2>&1 | tee "$MODEM_HELPER_EVIDENCE/session-630.log"
grep -F 'test result: ok. 3 passed; 0 failed; 0 ignored;' "$MODEM_HELPER_EVIDENCE/session-630.log"
cargo test --manifest-path "$root/rust/Cargo.toml" -p openpilot-modem --features helper-lifetime-tests --test helper_lifetime --locked 2>&1 | tee "$MODEM_HELPER_EVIDENCE/fixed.log"
grep -F 'test result: ok. 15 passed; 0 failed; 0 ignored;' "$MODEM_HELPER_EVIDENCE/fixed.log"
cargo clippy --manifest-path "$root/rust/Cargo.toml" -p openpilot-modem --features openpilot-modem/helper-lifetime-tests -p openpilot-process-supervision --all-targets --locked -- -D warnings
"$MODEM_SOURCE_PYTHON" "$root/rust/tools/check_modem_process_boundary.py" --target "$CARGO_TARGET_DIR" --evidence "$MODEM_HELPER_EVIDENCE/process-boundary"
"$MODEM_SOURCE_PYTHON" "$root/rust/tools/check_modem.py" --binary "$CARGO_TARGET_DIR/debug/openpilot-modem" --trace "$CARGO_TARGET_DIR/debug/examples/modem_trace" --evidence "$MODEM_HELPER_EVIDENCE/source-comparison"
