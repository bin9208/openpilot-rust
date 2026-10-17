#!/bin/sh
name=${0##*/}
printf '%s\000' "$0" "$@" > "$ARGV_DIR/$name.argv"
while [ "$#" -gt 0 ]; do if [ "$1" = -o ]; then destination=$2; shift; fi; shift; done
printf 'controlled %s library\n' "$name" > "$destination"
printf 'café-%s\r\nnext\rline\n' "$name"
printf 'warn-%s\r\nnul:\000tail\r' "$name" >&2
if [ "$FIXTURE_CASE" = invalid-first-text ] && [ "$name" = clang++ ]; then printf '\377'; fi
if [ "$FIXTURE_CASE" = invalid-second-text ] && [ "$name" = cc ]; then printf '\377' >&2; fi
if [ "$FIXTURE_CASE" = first-failure ] && [ "$name" = clang++ ]; then printf 'partial library\n' > "$destination"; exit 7; fi
if [ "$FIXTURE_CASE" = second-failure ] && [ "$name" = cc ]; then printf 'partial library\n' > "$destination"; exit 9; fi
if [ "$FIXTURE_CASE" = second-signal ] && [ "$name" = cc ]; then kill -TERM $$; fi
