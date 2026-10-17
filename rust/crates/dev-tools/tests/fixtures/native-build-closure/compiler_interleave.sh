#!/bin/sh
printf 'out-1\n'
printf 'err-1\n' >&2
printf 'out-2\n'
printf 'err-2\n' >&2
printf '\377\000raw\n' >&2
if [ "${COMPILER_FIXTURE_SLEEP:-0}" != 0 ]; then sleep "$COMPILER_FIXTURE_SLEEP"; fi
if [ "${COMPILER_FIXTURE_EXIT:-0}" != 0 ]; then exit "$COMPILER_FIXTURE_EXIT"; fi
destination=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = -o ]; then destination=$2; shift; fi
  shift
done
printf 'controlled compiler product\n' > "$destination"
