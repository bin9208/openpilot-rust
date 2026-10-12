#!/usr/bin/env bash
RUST_LAUNCH_ROOT=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd) || exit 1
exec "$RUST_LAUNCH_ROOT/rust/bin/openpilot-launch" --root "$RUST_LAUNCH_ROOT" "$@"
