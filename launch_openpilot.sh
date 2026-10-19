#!/usr/bin/env bash
RUST_LAUNCH_ROOT=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd) || exit 1
if [[ $(uname -m) == aarch64 ]]; then
  # This also protects the first upgrade, before a launcher with the native
  # boot helper has been installed. Match upstream cmd_boot and voter order.
  JETLINK_BOOT_MODE=$(cat /data/params/d/JetlinkMode 2>/dev/null)
  if [[ -f /AGNOS && -n "$JETLINK_BOOT_MODE" && "$JETLINK_BOOT_MODE" != 0 ]]; then
    if ! { printf '0\n' | sudo -n tee /sys/kernel/debug/pmic-votable/USB_ICL/force_val >/dev/null &&
           printf '1\n' | sudo -n tee /sys/kernel/debug/pmic-votable/USB_ICL/force_active >/dev/null; }; then
      echo 'Jetlink early USB power setup failed; the native launcher will retry after installation.' >&2
    fi
  fi
  bash "$RUST_LAUNCH_ROOT/rust/tools/ensure_runtime.sh" --root "$RUST_LAUNCH_ROOT" || exit $?
fi
exec "$RUST_LAUNCH_ROOT/rust/bin/openpilot-launch" --root "$RUST_LAUNCH_ROOT" "$@"
