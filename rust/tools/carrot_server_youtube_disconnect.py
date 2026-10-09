#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Real connected-recipient EOF followed by original/native librtmp write cleanup."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import time

from carrot_server_dashcam_upload import save
from carrot_server_dashcam_sync_probe import Json
from carrot_server_youtube_recipient import Recipient, TlsPeer
from carrot_server_youtube_transport import Provider


def capture(provider: Provider, args: argparse.Namespace) -> dict[str, Json]:
  directory = args.output / provider['name']
  directory.mkdir()
  params = directory / 'params'
  params.mkdir()
  with Recipient(directory) as receiver:
    peer = TlsPeer(args.certificate, args.key, receiver.port)
    process = None
    try:
      value = {
        'url': f'rtmps://127.0.0.1:{peer.port}/live2/owned-stream-key',
        'input': str(args.flv),
        'chunk': args.flv.stat().st_size,
        'pause_after_connect': True,
        'owned_root': str(directory),
        'receipt': str(directory / 'source-result.json'),
        'monitor_blocked': args.mode != 'disconnect',
        'force_wake': args.mode == 'wake',
      }
      environment = os.environ | {
        'PARAMS_ROOT': str(params),
        'OPENPILOT_PREFIX': 'owned-disconnect',
        'CARROT_DATA_DIR': str(directory),
        'SSL_CERT_FILE': str(args.certificate),
      }
      with (directory / 'process.log').open('wb') as log:
        started = time.monotonic()
        process = subprocess.Popen(provider['argv'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, env=environment)
        assert process.stdin and process.stdout
        process.stdin.write((json.dumps(value) + '\n').encode())
        process.stdin.flush()
        assert select.select([process.stdout], [], [], 10)[0], 'publish did not complete'
        assert json.loads(process.stdout.readline()) == {'ready': True}
        if args.mode == 'disconnect':
          peer.close()
        else:
          peer.pause.set()
          assert peer.paused.wait(1), 'owned recipient did not stop reading'
        payload, _errors = process.communicate(b'continue\n', timeout=8)
        result = json.loads(payload)
        save(
          directory / 'invocation.json',
          {'argv': provider['argv'], 'input': value, 'exit': process.returncode, 'seconds': time.monotonic() - started, 'stdout': payload.decode()},
        )
        assert process.returncode == 0 and result['ok'] is False and result['bytes_written'] == 0
        assert result['pending_bytes'] == args.flv.stat().st_size
        if args.mode != 'disconnect':
          observation = result['monitor']
          assert observation['health_during_write'] is None and observation['worker_joined']
          duration = observation['write_seconds']
          assert 1.9 <= duration < 3 if args.mode == 'wake' else 5.9 <= duration < 9
          peer.discard.set()
          peer.pause.clear()
          deadline = time.monotonic() + 1
          while not peer.eof and time.monotonic() < deadline:
            time.sleep(0.01)
          peer.thread.join(timeout=1)
          save(
            directory / 'remote-close.json',
            {
              'eof': peer.eof,
              'error': peer.error,
              'peer_exited': not peer.thread.is_alive(),
              'discard_buffered_data_after_worker_exit': True,
            },
          )
          assert not peer.thread.is_alive() and (peer.eof or peer.error)
        save(directory / 'result.json', result)
    finally:
      peer.close()
      if process:
        if process.poll() is None:
          process.kill()
        process.wait(timeout=3)
    save(directory / 'cleanup.json', {'peer_thread_exited': not peer.thread.is_alive(), 'child_reaped': process is not None and process.returncode is not None})
  save(directory / 'recipient.json', {'argv': receiver.argv, 'pid': receiver.process.pid, 'exit': receiver.process.returncode})
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['binary', 'flv', 'certificate', 'key', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--mode', choices=['disconnect', 'stall', 'wake'], default='disconnect')
  parser.add_argument('--native-only', action='store_true')
  args = parser.parse_args()
  for name in ['binary', 'flv', 'certificate', 'key', 'output']:
    setattr(args, name, getattr(args, name).resolve())
  args.output.mkdir(parents=True)
  providers: list[Provider] = [
    {'name': 'source', 'argv': [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_youtube_transport_source.py'))]},
    {'name': 'native', 'argv': [str(args.binary)]},
  ]
  if args.native_only:
    providers = providers[1:]
  results = [capture(provider, args) for provider in providers]
  save(
    args.output / 'result.json',
    {'mode': args.mode, 'actual_connected_recipient_closed': args.mode == 'disconnect', 'both_writes_failed': True, 'results': results},
  )


if __name__ == '__main__':
  main()
