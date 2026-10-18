#!/bin/sh
# Immutable test launcher: observe inherited stdin, then run the real daemon.
IFS= read -r token || :
printf '%s\n' "$token" >> "$TIMED_STDIN_PROBE"
exec "$TIMED_NATIVE_ENTRY" "$@"
