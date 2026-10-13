#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Repeated real clients free handles while retaining the process-owned provider."""

from __future__ import annotations

import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from carrot_server_dashcam_upload import save
from carrot_server_youtube_decode import decoded
from carrot_server_youtube_recipient import Recipient


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['binary', 'flv', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True)
  results = []
  providers = [
    ('source', [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_youtube_transport_source.py'))]),
    ('native', [str(args.binary.resolve())]),
  ]
  for name, argv in providers:
    directory = output / name
    params = directory / 'params'
    params.mkdir(parents=True)
    receivers = []
    with ExitStack() as stack:
      for index in range(2):
        root = directory / str(index)
        root.mkdir()
        receivers.append(stack.enter_context(Recipient(root)))
      value = {'url': receivers[0].url, 'repeat_urls': [receiver.url for receiver in receivers], 'input': str(args.flv.resolve()), 'owned_root': str(directory)}
      environment = os.environ | {'PARAMS_ROOT': str(params), 'OPENPILOT_PREFIX': 'owned-provider-lifetime', 'CARROT_DATA_DIR': str(directory)}
      start = time.monotonic()
      process = subprocess.run(argv, input=(json.dumps(value) + '\n').encode(), capture_output=True, env=environment, timeout=10)
      save(
        directory / 'invocation.json',
        {
          'argv': argv,
          'input': value,
          'exit': process.returncode,
          'seconds': time.monotonic() - start,
          'stdout': process.stdout.decode(),
          'stderr': process.stderr.decode(),
        },
      )
      assert process.returncode == 0 and 'Sanitizer' not in process.stderr.decode()
      snapshots = json.loads(process.stdout)
      assert len(snapshots) == 2 and all(row['library_mapped_after_client_drop'] and row['bytes_written'] == args.flv.stat().st_size for row in snapshots)
    media = [decoded(receiver.path.read_bytes()) for receiver in receivers]
    assert all(len(value['video']) == 8 and len(value['audio']) == 17 for value in media)
    save(directory / 'result.json', {'cycles': snapshots, 'decoded': media, 'recipient_exits': [receiver.process.returncode for receiver in receivers]})
    results.append((snapshots, media))
  assert results[0] == results[1]
  save(
    output / 'result.json',
    {
      'cycles_per_process': 2,
      'per_client_handles_freed': True,
      'provider_mapped_after_drop': True,
      'source_native_equal': True,
      'sanitizer_scope': 'Native Rust instrumented; installed provider uninstrumented',
    },
  )


if __name__ == '__main__':
  main()
