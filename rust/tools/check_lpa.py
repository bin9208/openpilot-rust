#!/usr/bin/env python3
"""Focused unchanged-source differential plus native PTY/TLS/lock lifecycle evidence."""

import argparse
import fcntl
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from lpa_fixture import BPP, Fixture, b64, certificate, tlv


def scenarios(address):
  return {
    'profiles': [
      {'op': 'list'},
      {'op': 'active'},
      {'op': 'nickname', 'iccid': '123456789012345', 'nickname': '새 프로필'},
      {'op': 'nickname', 'iccid': '123456789012345', 'nickname': '가' * 22},
      {'op': 'switch', 'iccid': '123456789012345'},
      {'op': 'delete', 'iccid': '8985235123456789012'},
      {'op': 'delete', 'iccid': 'missing'},
      {'op': 'delete', 'iccid': '123456789012345'},
      {'op': 'is_euicc'},
      {'op': 'query', 'command': 'AT+BAD'},
      {'op': 'query', 'command': 'AT+TIMEOUT'},
    ],
    'download': [
      {'op': 'codec', 'data': BPP.hex(), 'digits': '12345-6789', 'activation': 'LPA:1$fixture.example$MATCH'},
      {'op': 'download', 'activation': f'LPA:1${address}$MATCH', 'nickname': 'Installed'},
      {'op': 'notifications'},
      {
        'op': 'prepare',
        'signed': b64(tlv(0x30, tlv(0x80, b'transaction') + tlv(1, b'\1'))),
        'signature': b64(tlv(0x5F37, b'sig')),
        'certificate': b64(tlv(0x30, b'cert')),
        'cc': 'confirmation',
      },
      {
        'op': 'prepare',
        'signed': b64(tlv(0x30, tlv(0x80, b'transaction') + tlv(1, b'\1'))),
        'signature': b64(tlv(0x5F37, b'sig')),
        'certificate': b64(tlv(0x30, b'cert')),
      },
    ],
    'cancel': [{'op': 'download', 'activation': f'LPA:1${address}$MATCH'}],
    'open_retry': [{'op': 'list'}],
    'apdu_retry': [
      {'op': 'list'},
      {'op': 'apdu', 'data': '80E2910003BF2000'},
      {'op': 'query', 'command': 'AT+RECONNECT'},
      {'op': 'apdu', 'data': '80E2910003BF2000'},
    ],
    'install_error': [{'op': 'download', 'activation': f'LPA:1${address}$MATCH'}],
  }


def trace(executable, root, cert, key, launcher, scenario):
  fixture = Fixture(root, cert, key, launcher, scenario)
  try:
    request = {'config': fixture.config, 'ca': str(cert), 'operations': scenarios(fixture.address)[scenario]}
    result = subprocess.run(
      executable,
      input=json.dumps(request),
      text=True,
      capture_output=True,
      timeout=20,
      env=os.environ | {'PYTHONPATH': str(Path(__file__).resolve().parents[2])},
    )
    assert result.returncode == 0, result.stderr
    assert not fixture.errors, fixture.errors
    records = {
      'rows': json.loads(result.stdout),
      'commands': fixture.commands,
      'requests': fixture.requests,
      'resets': (root / 'resets').read_text().splitlines() if (root / 'resets').exists() else [],
    }
    normalized = json.dumps(records).replace(fixture.address, '<loopback>')
    return json.loads(normalized), result.stderr
  finally:
    fixture.close()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--target', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.target = args.target.resolve()
  args.evidence.mkdir(parents=True, exist_ok=True)
  launcher = args.target / 'debug/openpilot-process-child'
  native = [str(args.target / 'debug/examples/lpa_trace')]
  source = [sys.executable, str(Path(__file__).with_name('lpa_source.py'))]
  report = []
  with tempfile.TemporaryDirectory(prefix='lpa-fixture-') as directory:
    root = Path(directory)
    cert, key = certificate(root)
    for scenario in scenarios('unused'):
      outputs = []
      for name, executable in [('source', source), ('native', native)]:
        folder = root / (scenario + '-' + name)
        folder.mkdir()
        records, stderr = trace(executable, folder, cert, key, launcher, scenario)
        (args.evidence / f'{scenario}-{name}.json').write_text(json.dumps(records, indent=2) + '\n')
        (args.evidence / f'{scenario}-{name}.stderr').write_text(stderr or '(no stderr)\n')
        outputs.append(records)
      assert outputs[0] == outputs[1], f'differential mismatch: {scenario}'
      if scenario == 'download':
        assert outputs[1]['rows'][1] == {'result': None}
        assert [request['endpoint'] for request in outputs[1]['requests']] == [
          'initiateAuthentication',
          'authenticateClient',
          'getBoundProfilePackage',
          'handleNotification',
        ]
        assert [request['cookie'] for request in outputs[1]['requests']] == [None, 'session=fixture', 'session=fixture', None]
        assert outputs[1]['rows'][-1] == {'error': 'Confirmation code required but not provided'}
      if scenario == 'cancel':
        assert [request['endpoint'] for request in outputs[1]['requests']] == ['initiateAuthentication', 'authenticateClient', 'cancelSession']
        assert 'already installed on another device' in outputs[1]['rows'][0]['error']
      if scenario == 'install_error':
        assert outputs[1]['rows'] == [{'error': 'This eSIM profile is already installed on this device.'}]
        assert outputs[1]['requests'][-1]['endpoint'] == 'cancelSession'
      if scenario in ('profiles', 'open_retry'):
        assert outputs[1]['resets'] == ['reset']
      report.append({'scenario': scenario, 'pass': True, 'artifact': f'{scenario}-native.json'})
    folder = root / 'lifecycle'
    folder.mkdir()
    fixture = Fixture(folder, cert, key, launcher)
    try:
      config = folder / 'config.json'
      config.write_text(json.dumps(fixture.config))
      binary = args.target / 'debug/openpilot-lpa'
      with open(fixture.config['lock'], 'w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        process = subprocess.Popen([binary, '--config', config], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
          process.stdin.write('{"op":"list"}')
          process.stdin.close()
          process.stdin = None
          time.sleep(0.2)
          assert process.poll() is None and not fixture.commands
          executable = str(Path(f'/proc/{process.pid}/exe').resolve())
          assert executable == str(binary)
          fcntl.flock(lock, fcntl.LOCK_UN)
          stdout, stderr = process.communicate(timeout=5)
          assert process.returncode == 0, stderr
          assert len(json.loads(stdout)) == 2
          assert fixture.commands[-1] == 'AT+CCHC=1'
          fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
          fcntl.flock(lock, fcntl.LOCK_UN)
        finally:
          if process.poll() is None:
            process.kill()
            process.wait()
      lifecycle = {
        'executable': executable,
        'exit': process.returncode,
        'stdout': json.loads(stdout),
        'commands': fixture.commands,
        'blocked_until_lock_release': True,
        'lock_reacquired': True,
      }
      (args.evidence / 'lifecycle.json').write_text(json.dumps(lifecycle, indent=2) + '\n')
      report.append({'scenario': 'native_binary_lock_and_channel_lifecycle', 'pass': True, 'artifact': 'lifecycle.json'})
      rejects = []
      alternate = root / 'alternate'
      alternate.mkdir()
      other_cert, _ = certificate(alternate)
      for name, executable in [('source', source), ('native', native)]:
        for case, ca, address in [('hostname', cert, fixture.address.replace('localhost', '127.0.0.1')), ('trust', other_cert, fixture.address)]:
          before = len(fixture.requests)
          request = {'config': fixture.config, 'ca': str(ca), 'operations': [{'op': 'http', 'address': address}]}
          result = subprocess.run(
            executable,
            input=json.dumps(request),
            capture_output=True,
            text=True,
            timeout=5,
            env=os.environ | {'PYTHONPATH': str(Path(__file__).resolve().parents[2])},
          )
          assert result.returncode == 0, result.stderr
          rows = json.loads(result.stdout)
          assert 'error' in rows[0] and len(fixture.requests) == before
          rejects.append({'runtime': name, 'case': case, 'error': rows[0]['error'], 'requests_reached_server': 0})
      (args.evidence / 'tls-rejection.json').write_text(json.dumps(rejects, indent=2) + '\n')
      report.append({'scenario': 'tls_hostname_and_root_rejection', 'pass': True, 'artifact': 'tls-rejection.json'})
      fixture.mode = 'http_timeout'
      deadlines = []
      for name, executable in [('source', source), ('native', native)]:
        request = {'config': fixture.config, 'ca': str(cert), 'operations': [{'op': 'http', 'address': fixture.address}]}
        started = time.monotonic()
        result = subprocess.run(
          executable,
          input=json.dumps(request),
          capture_output=True,
          text=True,
          timeout=5,
          env=os.environ | {'PYTHONPATH': str(Path(__file__).resolve().parents[2])},
        )
        elapsed = time.monotonic() - started
        assert result.returncode == 0, result.stderr
        row = json.loads(result.stdout)[0]
        assert 'error' in row and any(word in row['error'].lower() for word in ('timeout', 'timed out'))
        assert 1.8 <= elapsed < 3.0, elapsed
        deadlines.append({'runtime': name, 'elapsed_seconds': elapsed, 'error': row['error'], 'configured_timeout_seconds': 2})
      (args.evidence / 'http-deadline.json').write_text(json.dumps(deadlines, indent=2) + '\n')
      report.append({'scenario': 'https_read_deadline', 'pass': True, 'artifact': 'http-deadline.json'})
    finally:
      fixture.close()
  (args.evidence / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'passed': len(report), 'evidence': str(args.evidence)}))


if __name__ == '__main__':
  main()
