#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Controlled real-provider positive consumption with complete FLV prefix/carry."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from carrot_server_dashcam_upload import save
from carrot_server_youtube_decode import decoded
from carrot_server_youtube_recipient import Recipient
from carrot_server_youtube_transport import Provider, invoke


def complete_prefix(data: bytes) -> int:
  assert data[:3] == b'FLV'
  offset = 13
  prefix = 0
  while offset < len(data):
    size = int.from_bytes(data[offset + 1 : offset + 4], 'big')
    offset += size + 15
    assert offset <= len(data)
    if 16384 <= offset < len(data):
      prefix = offset
  assert prefix > 0
  return prefix


def capture(provider: Provider, args: argparse.Namespace) -> tuple[dict, dict]:
  directory = args.output / provider['name']
  directory.mkdir()
  params = directory / 'params'
  params.mkdir()
  with Recipient(directory) as receiver:
    payload = {
      'url': receiver.url,
      'input': str(args.flv),
      'chunk': args.flv.stat().st_size,
      'owned_root': str(directory),
      'receipt': str(directory / 'source-result.json'),
      'omit_carry_fixture': args.omit_only,
    }
    environment = os.environ | {
      'PARAMS_ROOT': str(params),
      'OPENPILOT_PREFIX': 'owned-carry',
      'CARROT_DATA_DIR': str(directory),
      'LD_LIBRARY_PATH': str(args.output),
      'OWNED_RTMP_PROVIDER': str(args.provider),
      'OWNED_RTMP_FIRST_PREFIX': str(args.prefix),
      'OWNED_RTMP_WRITE_TRACE': str(directory / 'writes.txt'),
    }
    result = invoke(provider, payload, directory, environment)
    assert result['ok'] and result['pending_bytes'] == 0
    if args.omit_only:
      assert result['bytes_written'] == args.prefix
    else:
      assert result['partial_writes'] == 1 and result['bytes_written'] == args.flv.stat().st_size
  rows = [list(map(int, line.split())) for line in (directory / 'writes.txt').read_text().splitlines()]
  requested, forwarded, consumed = rows[0]
  assert 0 < consumed <= forwarded < requested and forwarded == args.prefix
  assert all(0 < actual <= sent <= requested for requested, sent, actual in rows)
  media = decoded(receiver.path.read_bytes())
  save(directory / 'observation.json', {'calls': rows, 'decoded': media, 'recipient_exit': receiver.process.returncode})
  return {key: value for key, value in result.items() if key != 'seconds'}, media


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['binary', 'provider', 'flv', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--omit-only', action='store_true')
  parser.add_argument('--native-only', action='store_true')
  parser.add_argument('--sanitize-proxy', action='store_true')
  args = parser.parse_args()
  for name in ['binary', 'provider', 'flv', 'output']:
    setattr(args, name, getattr(args, name).resolve())
  args.output.mkdir(parents=True)
  args.prefix = complete_prefix(args.flv.read_bytes())
  required = 25 * 2**30 + 3648 * 2**20
  free = shutil.disk_usage(args.output).free
  save(args.output / 'guard.json', {'free': free, 'required': required})
  assert free >= required
  argv = [
    'cc',
    '-shared',
    '-fPIC',
    '-Wall',
    '-Wextra',
    '-Werror',
    str(Path(__file__).with_name('youtube_rtmp_trace.c')),
    '-ldl',
    '-o',
    str(args.output / 'librtmp.so.1'),
  ]
  if args.sanitize_proxy:
    argv[0] = 'clang'
    argv[1:1] = ['-fsanitize=address']
  build = subprocess.run(argv, capture_output=True, timeout=15)
  save(args.output / 'build.json', {'argv': argv, 'exit': build.returncode, 'stderr': build.stderr.decode()})
  assert build.returncode == 0
  providers: list[Provider] = [
    {'name': 'source', 'argv': [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_youtube_transport_source.py'))]},
    {'name': 'native', 'argv': [str(args.binary)]},
  ]
  if args.omit_only:
    result = capture(providers[0], args)
    expected = decoded(args.flv.read_bytes())
    assert len(result[1]['audio']) < len(expected['audio'])
    save(
      args.output / 'result.json',
      {
        'fixture_omits_carry': True,
        'missing_audio_detected': True,
        'expected_audio_frames': len(expected['audio']),
        'actual_audio_frames': len(result[1]['audio']),
      },
    )
    return
  if args.native_only:
    result = capture(providers[1], args)
    assert len(result[1]['video']) == 8 and len(result[1]['audio']) == 17
    save(
      args.output / 'result.json',
      {
        'native_only': True,
        'controlled_prefix': args.prefix,
        'owned_proxy_instrumented': args.sanitize_proxy,
        'system_provider_instrumented': False,
        'decoded_video': 8,
        'decoded_audio': 17,
      },
    )
    return
  results = [capture(provider, args) for provider in providers]
  assert results[0] == results[1]
  assert len(results[0][1]['video']) == 8
  save(
    args.output / 'result.json',
    {'controlled_prefix': args.prefix, 'naturally_occurring_partial_writes': False, 'counters_equal': True, 'decoded_equal': True, 'frames': 8},
  )
  print(json.dumps({'pass': True}))


if __name__ == '__main__':
  main()
