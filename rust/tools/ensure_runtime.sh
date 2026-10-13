#!/usr/bin/env bash
set -euo pipefail

if [[ $# != 2 || $1 != --root ]]; then
  echo 'usage: ensure_runtime.sh --root CHECKOUT' >&2
  exit 2
fi
runtime_root=$(CDPATH= cd -- "$2" && pwd -P)
if [[ ! -e "$runtime_root/rust/.runtime-install-journal.json" &&
      ! -L "$runtime_root/rust/.runtime-install-journal.json" &&
      ! -e "$runtime_root/rust/.runtime-install-rollback" &&
      ! -L "$runtime_root/rust/.runtime-install-rollback" &&
      -x "$runtime_root/rust/bin/openpilot-launch" ]] &&
   LD_LIBRARY_PATH="$runtime_root/rust/bin/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
     "$runtime_root/rust/bin/openpilot-launch" --verify-package --root "$runtime_root"; then
  exit 0
fi

runtime_commit=$(git -C "$runtime_root" rev-parse HEAD)
if [[ ! $runtime_commit =~ ^[0-9a-f]{40}$ ]]; then
  echo 'Cannot determine exact runtime source commit' >&2
  exit 1
fi
runtime_cache="$runtime_root/rust/.runtime-bootstrap/$runtime_commit"
mkdir -p -- "$runtime_cache"
exec 9>"$runtime_root/rust/.runtime-bootstrap/install.lock"
flock --exclusive 9
for runtime_abandoned in "$runtime_cache"/download.*; do
  if [[ -d $runtime_abandoned && ! -L $runtime_abandoned ]]; then
    rm -r -- "$runtime_abandoned"
  fi
done
runtime_helper=openpilot-runtime-install-aarch64
runtime_url="https://github.com/bin9208/openpilot-rust/releases/download/runtime-$runtime_commit"

valid_helper() {
  local checksum_name="$runtime_cache/$runtime_helper.sha256"
  local digest filename trailing
  local -a lines
  [[ -f "$checksum_name" && -x "$runtime_cache/$runtime_helper" ]] || return 1
  mapfile -t lines < "$checksum_name"
  [[ ${#lines[@]} == 1 ]] || return 1
  read -r digest filename trailing <<< "${lines[0]}"
  [[ $digest =~ ^[0-9a-f]{64}$ && $filename == "$runtime_helper" && -z $trailing ]] || return 1
  printf '%s  %s\n' "$digest" "$runtime_cache/$runtime_helper" | sha256sum --check --status
}

if ! valid_helper; then
  runtime_free_kib=$(df -Pk -- "$runtime_cache" | awk 'END {print $4}')
  if [[ ! $runtime_free_kib =~ ^[0-9]+$ ]] || (( runtime_free_kib < 2162688 )); then
    echo 'Native runtime bootstrap needs 2 GiB free plus 64 MiB temporary space' >&2
    exit 1
  fi
  runtime_download=$(mktemp -d "$runtime_cache/download.XXXXXXXX")
  runtime_child=
  cleanup_download() {
    local runtime_pending
    for runtime_pending in $(jobs -p); do
      kill "$runtime_pending" 2>/dev/null || true
      wait "$runtime_pending" 2>/dev/null || true
    done
    rm -r -- "$runtime_download"
  }
  trap cleanup_download EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  echo "Downloading native runtime installer for $runtime_commit" >&2
  for runtime_asset in "$runtime_helper.sha256" "$runtime_helper"; do
    runtime_limit=33554432
    [[ $runtime_asset != *.sha256 ]] || runtime_limit=4096
    while :; do
      curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
        --connect-timeout 15 --max-time 300 --max-filesize "$runtime_limit" \
        --write-out '%{http_code}' --output "$runtime_download/$runtime_asset" \
        "$runtime_url/$runtime_asset" > "$runtime_download/http-status" &
      runtime_child=$!
      if wait "$runtime_child"; then
        runtime_child=
        break
      else
        runtime_status=$?
      fi
      runtime_child=
      runtime_http=$(< "$runtime_download/http-status")
      case "$runtime_status:$runtime_http" in
        5:*|6:*|7:*|18:*|28:*|35:*|52:*|55:*|56:*|60:*|22:408|22:429|22:500|22:502|22:503|22:504)
          rm -f -- "$runtime_download/$runtime_asset"
          echo "Native runtime download waiting for network/time; asset=$runtime_asset curl_exit=$runtime_status http_status=$runtime_http retry_seconds=5"
          sleep 5 &
          runtime_child=$!
          wait "$runtime_child"
          runtime_child=
          ;;
        *)
          echo "Native runtime download failed; asset=$runtime_asset curl_exit=$runtime_status" >&2
          exit "$runtime_status"
          ;;
      esac
    done
  done
  chmod 755 "$runtime_download/$runtime_helper"
  runtime_previous_cache=$runtime_cache
  runtime_cache=$runtime_download
  if ! valid_helper; then
    echo 'Native runtime installer checksum verification failed' >&2
    exit 1
  fi
  runtime_cache=$runtime_previous_cache
  mv -- "$runtime_download/$runtime_helper" "$runtime_cache/$runtime_helper"
  mv -- "$runtime_download/$runtime_helper.sha256" "$runtime_cache/$runtime_helper.sha256"
  rm -r -- "$runtime_download"
  trap - EXIT INT TERM
fi
flock --unlock 9
exec 9>&-
exec "$runtime_cache/$runtime_helper" --root "$runtime_root"
