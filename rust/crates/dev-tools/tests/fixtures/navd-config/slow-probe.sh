#!/bin/sh
# Process-boundary control, no navd response/policy or success data.
if [ -n "${NAVD_CONFIG_FIXTURE_MARKER:-}" ]; then
  printf '%s\n' "$$" > "$NAVD_CONFIG_FIXTURE_MARKER"
fi
exec /bin/sleep 60
