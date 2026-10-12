#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Capture real librtmp URL arguments through an owned pass-through ABI proxy."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

from carrot_server_dashcam_upload import save
from carrot_server_youtube_recipient import Recipient, TlsPeer
from carrot_server_youtube_transport import Provider, invoke


def trace(path: Path) -> list[dict[str, str]]:
  rows = []
  for line in path.read_text().splitlines():
    name, length, encoded = line.split()
    raw = bytes.fromhex(encoded)
    assert len(raw) == int(length)
    text = re.sub(r'127\.0\.0\.1:\d+', '127.0.0.1:PORT', raw.decode())
    rows.append({'name': name, 'text': text})
  return rows


def capture(provider: Provider, args: argparse.Namespace, directory: Path) -> list[dict[str, str]]:
  directory.mkdir(parents=True)
  params = directory / 'params'
  params.mkdir()
  with Recipient(directory) as receiver:
    peer = TlsPeer(args.certificate, args.key, receiver.port)
    try:
      endpoint = f'rtmps://127.0.0.1:{peer.port}' + args.suffix
      payload = {'url': endpoint, 'owned_root': str(directory), 'receipt': str(directory / 'source-result.json')}
      environment = os.environ | {
        'SSL_CERT_FILE': str(args.certificate),
        'PARAMS_ROOT': str(params),
        'OPENPILOT_PREFIX': 'owned-url-trace',
        'CARROT_DATA_DIR': str(directory),
        'LD_LIBRARY_PATH': str(args.output),
        'OWNED_RTMP_PROVIDER': str(args.provider),
        'OWNED_RTMP_TRACE': str(directory / 'trace.txt'),
      }
      result = invoke(provider, payload, directory, environment)
      assert result['ok'] and result['connected'], result
    finally:
      peer.close()
      save(directory / 'tls.json', {'eof': peer.eof, 'thread_exited': not peer.thread.is_alive(), 'error': peer.error})
  save(directory / 'recipient.json', {'argv': receiver.argv, 'pid': receiver.process.pid, 'exit': receiver.process.returncode})
  rows = trace(directory / 'trace.txt')
  save(directory / 'trace.json', rows)
  return rows


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['binary', 'provider', 'certificate', 'key', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--case', choices=['dot-segment', 'unicode-query', 'empty-query'], action='append')
  args = parser.parse_args()
  for name in ['binary', 'provider', 'certificate', 'key', 'output']:
    setattr(args, name, getattr(args, name).resolve())
  args.output.mkdir(parents=True)
  free = shutil.disk_usage(args.output).free
  required = 25 * 2**30 + 3648 * 2**20
  save(args.output / 'guard.json', {'free': free, 'required': required, 'estimated_proxy_growth': 2**20})
  assert free >= required
  source = Path(__file__).with_name('youtube_rtmp_trace.c')
  argv = ['cc', '-shared', '-fPIC', '-Wall', '-Wextra', '-Werror', str(source), '-ldl', '-o', str(args.output / 'librtmp.so.1')]
  build = subprocess.run(argv, capture_output=True, timeout=15)
  save(
    args.output / 'build.json',
    {'argv': argv, 'exit': build.returncode, 'stderr': build.stderr.decode(), 'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest()},
  )
  assert build.returncode == 0
  providers: list[Provider] = [
    {'name': 'source', 'argv': [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_youtube_transport_source.py'))]},
    {'name': 'native', 'argv': [str(args.binary)]},
  ]
  observations = []
  for name, suffix in [
    ('dot-segment', '/live2/owned-key/../tail1234'),
    ('unicode-query', '/live2/owned-한글\t1234?value=한글#fragment'),
    ('empty-query', '/live2/owned-key?'),
  ]:
    if args.case and name not in args.case:
      continue
    args.suffix = suffix
    rows = [capture(provider, args, args.output / name / provider['name']) for provider in providers]
    assert rows[0] == rows[1], rows
    observations.append({'case': name, 'arguments': rows[0], 'equal': True})
  save(args.output / 'result.json', {'actual_librtmp_forwarding': str(args.provider), 'normalization': 'only ephemeral loopback ports', 'cases': observations})
  print(json.dumps({'cases': len(observations), 'pass': True}))


if __name__ == '__main__':
  main()
