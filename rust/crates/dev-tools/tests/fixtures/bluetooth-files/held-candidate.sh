#!/bin/sh
# Immutable negative lifecycle fixture. Inputs are private test environment only.
/bin/sleep 120 &
printf '%s %s %s\n' "$BT_FILES_TOKEN" "$$" "$!" > "$BT_FILES_MARKER"
wait
