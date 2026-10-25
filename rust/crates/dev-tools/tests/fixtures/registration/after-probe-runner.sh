#!/bin/sh
# Immutable boundary fixture: run the REAL probe normally; hold its runner only
# after the probe has received ACK and exited, before the controller publishes.
set -eu
"$@"
: "${REGISTRATION_TEST_READY:?missing test ready path}"
: "${REGISTRATION_TEST_RELEASE:?missing test release path}"
printf 'probe-returned\n' > "$REGISTRATION_TEST_READY"
i=0
while [ ! -f "$REGISTRATION_TEST_RELEASE" ]; do
  i=$((i+1))
  if [ "$i" -ge 1000 ]; then exit 98; fi
  sleep 0.01
done
