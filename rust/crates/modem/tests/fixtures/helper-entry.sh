#!/bin/sh
# Immutable exec target: parallel fixture setup writes only data, never this file.
set -eu
root=${0%/*}
IFS= read -r nonce < "$root/nonce"
exec /usr/bin/env MODEM_HELPER_NONCE="$nonce" /bin/sh "$root/payload" "$@"
