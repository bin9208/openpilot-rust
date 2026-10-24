#!/usr/bin/env bash
set -euo pipefail

# Forward all openpilot service ports
while IFS=' ' read -r name port; do
  adb forward "tcp:${port}" "tcp:${port}" > /dev/null
done < <("$(dirname -- "${BASH_SOURCE[0]}")/service_ports")

# Forward SSH port, finding a free local port if 2222 is taken.
SSH_PORT=2222
while ss -tln | grep -q ":${SSH_PORT} "; do
  SSH_PORT=$((SSH_PORT + 1))
done
adb forward tcp:${SSH_PORT} tcp:22

# SSH!
ssh comma@localhost -p ${SSH_PORT} "$@"
