#!/usr/bin/env python3
"""Focused AGNOS source/native and actual CLI gate using owned files and local peers."""

import argparse
import fcntl
import json
import os
from pathlib import Path
import ssl
import subprocess
import sys
import tempfile

from agnos_fixture import Fixture, Server


def run_trace(executable, fixture, server):
  server.reset()
  result = subprocess.run(
    executable,
    input=json.dumps(fixture.request),
    text=True,
    capture_output=True,
    timeout=40,
    env=os.environ | {'PYTHONPATH': str(Path(__file__).resolve().parents[2])},
  )
  assert result.returncode == 0, result.stderr
  trace = json.loads(result.stdout) | fixture.snapshot() | {'requests': list(server.requests)}
  normalized = json.dumps(trace).replace(server.address, '<server>').replace(str(fixture.root), '<fixture>')
  return json.loads(normalized), result.stderr


def differential(args, root, server, case, tls=False):
  results = []
  for name, executable in [
    ('source', [sys.executable, str(Path(__file__).with_name('agnos_source.py'))]),
    ('native', [str(args.target / 'debug/examples/agnos_trace')]),
  ]:
    folder = root / (case + '-' + name)
    folder.mkdir()
    fixture = Fixture(folder, server, args.target / 'debug/openpilot-process-child', 'fatal' if tls or case == 'invalidurl' else case)
    if tls or case == 'invalidurl':
      manifest = json.loads(fixture.manifest.read_text())
      manifest[0]['url'] = ('https://localhost:' + str(server.server.server_port) + '/boot.xz') if tls else 'not-a-url'
      fixture.manifest.write_text(json.dumps(manifest))
    trace, stderr = run_trace(executable, fixture, server)
    (args.evidence / f'{case}-{name}.json').write_text(json.dumps(trace, indent=2) + '\n')
    (args.evidence / f'{case}-{name}.stderr').write_text(stderr or '(no stderr)\n')
    results.append(trace)
  assert results[0] == results[1], f'differential mismatch: {case}'
  if case in ('fatal', 'tls', 'invalidurl'):
    assert results[1]['rows'] == [{'error': 'Download failed after 5 attempts. Check the connection or update server, then Retry.'}]
    assert sum(row[:2] == ['progress', 'Retrying download'] for row in results[1]['events']) == 4
  if case == 'retry':
    assert sum(row[:2] == ['progress', 'Waiting for internet'] for row in results[1]['events']) == 5
    assert len(results[1]['requests']) == 6 and results[1]['rows'] == [{'result': None}]
  if case == 'marker':
    assert results[1]['rows'][3:5] == [{'result': True}, {'result': False}], 'regular wb+ must retain source truncation semantics'
  if case == 'casync':
    assert ['log', 'error', 'casync done {"seed": 2048, "target": 1024, "remote": 1024}'] in results[1]['events']
    assert results[1]['events'].count(['sleep', 60]) == 2
    assert [request['cookie'] for request in results[1]['requests'] if request['path'].endswith('.cacnk')] == [None, 'chunk=fixture', 'chunk=fixture']
  return {'scenario': case, 'pass': True, 'artifact': f'{case}-native.json'}


def cli(args, root, server):
  records = []
  native_exe = None
  for name in ('source', 'native'):
    folder = root / ('cli-' + name)
    folder.mkdir()
    fixture = Fixture(folder, server, args.target / 'debug/openpilot-process-child', 'cli')
    config = folder / 'config.json'
    config.write_text(json.dumps(fixture.config))
    executable = (
      [str(args.target / 'debug/openpilot-agnos'), '--config', str(config)]
      if name == 'native'
      else [sys.executable, str(Path(__file__).with_name('agnos_source.py')), '--cli', str(config)]
    )
    environment = os.environ | {'PYTHONPATH': str(Path(__file__).resolve().parents[2])}
    server.reset()
    with open(fixture.config['paths']['lock'], 'w') as lock:
      fcntl.flock(lock, fcntl.LOCK_EX)
      held = subprocess.run([*executable, '--verify', str(fixture.manifest)], capture_output=True, text=True, env=environment, timeout=5)
      assert held.returncode != 0 and not fixture.snapshot()['calls']
      assert 'Another AGNOS updater is already running' in held.stderr
      fcntl.flock(lock, fcntl.LOCK_UN)
    process = subprocess.Popen(
      [*executable, '--swap', '--retry-network', str(fixture.manifest)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=environment
    )
    try:
      if name == 'native':
        native_exe = str(Path(f'/proc/{process.pid}/exe').resolve())
        assert native_exe == str(args.target / 'debug/openpilot-agnos')
      stdout, stderr = process.communicate(timeout=15)
      assert process.returncode == 0, stderr
    finally:
      if process.poll() is None:
        process.kill()
        process.wait()
    with open(fixture.config['paths']['lock'], 'w') as lock:
      fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    trace = fixture.snapshot() | {
      'requests': list(server.requests),
      'stdout': stdout,
      'exit': process.returncode,
      'lock_reacquired': True,
      'duplicate_rejected': True,
    }
    trace = json.loads(json.dumps(trace).replace(server.address, '<server>'))
    (args.evidence / f'cli-{name}.json').write_text(json.dumps(trace, indent=2) + '\n')
    (args.evidence / f'cli-{name}.stderr').write_text(stderr)
    records.append(trace)
  assert records[0] == records[1], 'actual CLI differential mismatch'
  assert records[1]['stdout'].endswith('Update complete; rebooting: 100\n')
  assert records[1]['calls'].count(['--set_active', '1']) == 3
  (args.evidence / 'native-executable.json').write_text(json.dumps({'exe': native_exe, 'native': True}) + '\n')
  return {'scenario': 'actual_cli_swap_and_lock', 'pass': True, 'artifact': 'cli-native.json'}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--target', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.target = args.target.resolve()
  args.evidence.mkdir(parents=True, exist_ok=True)
  reports = []
  with tempfile.TemporaryDirectory(prefix='agnos-fixture-') as directory:
    root = Path(directory)
    server = Server()
    try:
      for case in ('helpers', 'download', 'marker', 'retry', 'fatal', 'corruption', 'casync', 'invalidurl'):
        reports.append(differential(args, root, server, case))
      reports.append(cli(args, root, server))
    finally:
      server.close()
    cert, key = root / 'cert.pem', root / 'key.pem'
    subprocess.run(
      [
        'openssl',
        'req',
        '-x509',
        '-newkey',
        'rsa:2048',
        '-nodes',
        '-keyout',
        key,
        '-out',
        cert,
        '-days',
        '1',
        '-subj',
        '/CN=localhost',
        '-addext',
        'subjectAltName=DNS:localhost',
      ],
      check=True,
      capture_output=True,
    )
    tls = Server()
    try:
      context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
      context.load_cert_chain(cert, key)
      tls.server.socket = context.wrap_socket(tls.server.socket, server_side=True)
      reports.append(differential(args, root, tls, 'tls', tls=True))
    finally:
      tls.close()
  (args.evidence / 'results.json').write_text(json.dumps(reports, indent=2) + '\n')
  print(json.dumps({'passed': len(reports), 'evidence': str(args.evidence)}))


if __name__ == '__main__':
  main()
