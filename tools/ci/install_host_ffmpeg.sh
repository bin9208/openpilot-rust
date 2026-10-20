#!/usr/bin/env bash
# Host providers only. AGNOS and generic ARM source builds keep their own pins.
set -euo pipefail
mode="${1:-install}"
case "$mode" in install|--simulate|--verify) ;; *) echo "usage: $0 [--simulate|--verify]" >&2; exit 2 ;; esac
. /etc/os-release
case "$ID:${VERSION_ID:-}" in
  ubuntu:24.04)
    package_version='7:6.1.1-*'
    versions=(60.31.102 60.16.100 58.29.100 7.5.100)
    ;;
  ubuntu:26.04)
    package_version='7:8.0.1-*'
    versions=(62.11.100 62.3.100 60.8.100 9.1.100)
    ;;
  *) echo "Unsupported host FFmpeg provider: $ID ${VERSION_ID:-unknown}" >&2; exit 1 ;;
esac
libraries=(libavcodec libavformat libavutil libswscale)
packages=("ffmpeg=$package_version")
for library in "${libraries[@]}"; do
  packages+=("${library}-dev=$package_version")
done
if [ "$mode" != --verify ]; then
  if [ "$mode" = --simulate ]; then
    sudo -n apt-get --simulate install "${packages[@]}"
    exit
  fi
  available=$(df -Pk "${RUNNER_TEMP:-.}" | awk 'NR==2 {print $4}')
  if [ "$available" -lt $((35 * 1024 * 1024)) ]; then
    echo "Recover at least 35 GiB free before installing host FFmpeg." >&2
    exit 1
  fi
  sudo -n apt-get install -y "${packages[@]}"
fi
for index in "${!libraries[@]}"; do
  actual=$(pkg-config --modversion "${libraries[$index]}")
  if [ "$actual" != "${versions[$index]}" ]; then
    echo "${libraries[$index]} ABI: expected ${versions[$index]}, got $actual" >&2
    exit 1
  fi
  printf '%s=%s\n' "${libraries[$index]}" "$actual"
done
dpkg-query -W ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev
