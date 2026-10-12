#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Actual original/native librtmp and trusted TLS comparisons on owned recipients only."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid
from typing import TypedDict

from carrot_server_dashcam_upload import save
from carrot_server_dashcam_sync_probe import Json
from carrot_server_youtube_decode import decoded
from carrot_server_youtube_recipient import Recipient, TlsPeer


class Provider(TypedDict):
  name: str
  argv: list[str]


def certificate(output: Path, name: str, address: str) -> tuple[Path, Path]:
  cert, key = output / f'{name}.crt', output / f'{name}.key'
  argv = [
    'openssl',
    'req',
    '-x509',
    '-newkey',
    'rsa:2048',
    '-nodes',
    '-keyout',
    str(key),
    '-out',
    str(cert),
    '-days',
    '1',
    '-subj',
    '/CN=owned-youtube-fixture',
    '-addext',
    f'subjectAltName=IP:{address}',
    '-addext',
    'basicConstraints=critical,CA:FALSE',
  ]
  result = subprocess.run(argv, capture_output=True, timeout=10)
  save(output / f'{name}-invocation.json', {'argv': argv, 'exit': result.returncode, 'stderr': result.stderr.decode()})
  assert result.returncode == 0
  return cert, key


def invoke(provider: Provider, payload: dict[str, Json], output: Path, environment: dict[str, str]) -> dict[str, Json]:
  save(output / 'input.json', payload)
  started = time.monotonic()
  result = subprocess.run(provider['argv'], input=(json.dumps(payload) + '\n').encode(), capture_output=True, env=environment, timeout=16)
  save(
    output / 'invocation.json',
    {
      'argv': provider['argv'],
      'input': payload,
      'exit': result.returncode,
      'seconds': time.monotonic() - started,
      'stdout': result.stdout.decode(),
      'stderr': result.stderr.decode(),
    },
  )
  assert result.returncode == 0
  captured = json.loads(result.stdout)
  save(output / 'result.json', captured)
  return captured


def normal(providers: list[Provider], args: argparse.Namespace, output: Path, certs: tuple[Path, Path], environment: dict[str, str], *, secure: bool) -> None:
  snapshots = []
  frames = []
  for provider in providers:
    directory = output / provider['name']
    directory.mkdir(parents=True)
    with Recipient(directory) as receiver:
      tls = TlsPeer(*certs, receiver.port) if secure else None
      try:
        url = f'rtmps://127.0.0.1:{tls.port}/live2/owned-stream-key' if tls else receiver.url
        payload = {
          'url': url,
          'input': str(args.flv.resolve()),
          'chunk': 17,
          'owned_root': str(output.parent),
          'receipt': str(directory / 'source-result.json'),
        }
        result = invoke(provider, payload, directory, environment)
        assert result['ok'], result
        assert result['pending_bytes'] == 0
        snapshots.append({name: value for name, value in result.items() if name != 'seconds'})
      finally:
        if tls:
          tls.close()
          save(
            directory / 'tls-peer.json', {'eof': tls.eof, 'error': tls.error, 'thread_exited': not tls.thread.is_alive(), 'seconds': tls.finished - tls.started}
          )
    save(
      directory / 'recipient.json',
      {'argv': receiver.argv, 'pid': receiver.process.pid, 'exit': receiver.process.returncode, 'bytes': receiver.path.stat().st_size},
    )
    frames.append(decoded(receiver.path.read_bytes()))
  assert snapshots[0] == snapshots[1]
  assert frames[0] == frames[1]
  save(output / 'comparison.json', {'transport_counters_equal': True, 'decoded_recipient_equal': True, 'results': snapshots, 'decoded': frames})


def failure(providers: list[Provider], output: Path, certs: tuple[Path, Path], environment: dict[str, str], *, trickle: bool) -> None:
  results = []
  for provider in providers:
    directory = output / provider['name']
    directory.mkdir(parents=True)
    peer = TlsPeer(*certs, None, trickle=trickle)
    try:
      payload = {
        'url': f'rtmps://127.0.0.1:{peer.port}/live2/owned-stream-key',
        'owned_root': str(output.parent),
        'receipt': str(directory / 'source-result.json'),
      }
      result = invoke(provider, payload, directory, environment)
      assert result['ok'] is False and result['connected'] is False
      assert str(result['error']).startswith('YouTube RTMPS connection failed:')
      results.append(result)
    finally:
      peer.close()
      save(
        directory / 'tls-peer.json',
        {
          'eof': peer.eof,
          'error': peer.error,
          'thread_exited': not peer.thread.is_alive(),
          'seconds': peer.finished - peer.started,
          'trickled_bytes': peer.trickled_bytes,
        },
      )
    if trickle:
      assert peer.eof and peer.trickled_bytes >= 3
      assert 7.7 <= peer.finished - peer.started <= 9.5
  save(
    output / 'comparison.json', {'both_failed': True, 'provider_error_details_differ': True, 'total_handshake_deadline_observed': trickle, 'results': results}
  )


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--flv', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True)
  params = output / 'owned-params'
  prefix = 'youtube-transport-' + uuid.uuid4().hex
  (params / prefix).mkdir(parents=True)
  environment = os.environ | {'PARAMS_ROOT': str(params), 'OPENPILOT_PREFIX': prefix, 'CARROT_DATA_DIR': str(output / 'owned-data')}
  certs = certificate(output, 'trusted', '127.0.0.1')
  wrong = certificate(output, 'wrong-host', '127.0.0.2')
  environment['SSL_CERT_FILE'] = str(certs[0])
  providers: list[Provider] = [
    {'name': 'source', 'argv': [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_youtube_transport_source.py'))]},
    {'name': 'native', 'argv': [str(args.binary.resolve())]},
  ]
  save(
    output / 'invocation.json',
    {
      'argv': [sys.executable, '-P', *sys.argv],
      'PYTHONPATH': environment.get('PYTHONPATH', ''),
      'SSL_CERT_FILE': environment['SSL_CERT_FILE'],
      'owned_params_root': str(params),
      'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
      'recipients': 'owned 127.0.0.1 only',
    },
  )
  normal(providers, args, output / 'plain', certs, environment, secure=False)
  normal(providers, args, output / 'trusted-tls', certs, environment, secure=True)
  rejected = environment | {'SSL_CERT_FILE': str(wrong[0])}
  failure(providers, output / 'untrusted', certs, rejected, trickle=False)
  wrong_env = environment | {'SSL_CERT_FILE': str(wrong[0])}
  failure(providers, output / 'hostname', wrong, wrong_env, trickle=False)
  failure(providers, output / 'trickle', certs, environment, trickle=True)
  save(output / 'result.json', {'actual_rtmp_pairs': 2, 'tls_failure_pairs': 3, 'pass': True})


if __name__ == '__main__':
  main()
