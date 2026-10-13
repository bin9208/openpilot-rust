#!/usr/bin/env bash
RUST_LAUNCH_ROOT=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd) || exit 1
if [[ $(uname -m) == aarch64 ]]; then
  bash "$RUST_LAUNCH_ROOT/rust/tools/ensure_runtime.sh" --root "$RUST_LAUNCH_ROOT" || exit $?
fi
exec "$RUST_LAUNCH_ROOT/rust/bin/openpilot-launch" --root "$RUST_LAUNCH_ROOT" "$@"
