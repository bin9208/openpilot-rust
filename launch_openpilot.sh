#!/usr/bin/env bash
RUST_LAUNCH_ROOT=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd) || exit 1
if [[ $(uname -m) == aarch64 ]]; then
  # Keep a connected host from powering the auxiliary port while installation
  # or model preparation runs. The fixed-path native helper is a no-op if off.
  if [[ -f /AGNOS && -x "$RUST_LAUNCH_ROOT/rust/bin/openpilot-launch" ]]; then
    sudo -n "$RUST_LAUNCH_ROOT/rust/bin/openpilot-launch" --jetlink-boot-power || \
      echo 'Jetlink early USB power setup unavailable; the native launcher will retry after installation.' >&2
  fi
  bash "$RUST_LAUNCH_ROOT/rust/tools/ensure_runtime.sh" --root "$RUST_LAUNCH_ROOT" || exit $?
fi
exec "$RUST_LAUNCH_ROOT/rust/bin/openpilot-launch" --root "$RUST_LAUNCH_ROOT" "$@"
