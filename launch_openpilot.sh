#!/usr/bin/env bash
RUST_LAUNCH_ROOT=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd) || exit 1
if [[ -n ${OPENPILOT_NATIVE_MACHINE_CONFIG:-} ]]; then
  exec /usr/bin/env -u LD_LIBRARY_PATH -u LD_PRELOAD /usr/bin/python3 -I -B -S \
    "$RUST_LAUNCH_ROOT/tools/native/source_entry.py" --launch --source "$RUST_LAUNCH_ROOT" \
    --machine-config "$OPENPILOT_NATIVE_MACHINE_CONFIG" "$@"
fi
if [[ $(uname -m) == aarch64 ]]; then
  bash "$RUST_LAUNCH_ROOT/rust/tools/ensure_runtime.sh" --root "$RUST_LAUNCH_ROOT" || exit $?
fi
exec "$RUST_LAUNCH_ROOT/rust/bin/openpilot-launch" --root "$RUST_LAUNCH_ROOT" "$@"
