#!/bin/sh
# Immutable negative lifecycle fixture. Inputs are private test environment only.
/bin/sleep 120 &
printf '%s %s %s\n' "$BT_JOURNAL_TOKEN" "$$" "$!" > "$BT_JOURNAL_MARKER"
wait
