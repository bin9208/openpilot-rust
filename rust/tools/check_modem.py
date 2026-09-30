#!/usr/bin/env python3
"""Focused actual-source differential and real continuous native daemon/PTY gate."""

import argparse
import fcntl
import json
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

from modem_fixture import Fixture


def wait_for(predicate, description):
  deadline = time.monotonic() + 15
  while time.monotonic() < deadline:
    value = predicate()
    if value:
      return value
    time.sleep(0.02)
  raise AssertionError('timeout: ' + description)


def snapshot(path):
  try:
    return json.loads(path.read_text())
  except FileNotFoundError:
    return {}


def trace(executable, root, operations, changes, flags, held):
  fixture = Fixture(root, changes)
  lock = None
  try:
    for flag in flags:
      (root / flag).touch()
    if held:
      lock = open(root / 'lock', 'w')
      fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    request = {'config': fixture.config, 'operations': operations}
    result = subprocess.run(executable, input=json.dumps(request), text=True, capture_output=True, timeout=20)
    if result.returncode:
      raise AssertionError(result.stderr)
    rows = json.loads(result.stdout)
    for row in rows:
      row['snapshot'].pop('seconds_since_boot')
    calls = fixture.calls()
    for call in calls:
      if call[0] == 'pppd':
        call[1] = '<owned-data-port>'
    return {'rows': rows, 'commands': fixture.commands, 'calls': calls}
  finally:
    if lock:
      lock.close()
    fixture.close()


def differential(args, evidence):
  def step(state):
    return {'op': 'step', 'state': state}

  initialize = step('INITIALIZING')
  cases = [
    (
      'normal',
      [initialize, step('SEARCHING'), step('CONNECTING'), {'op': 'sleep', 'ms': 150}, step('CONNECTED'), {'op': 'poll'}, step('DISCONNECTING')],
      {},
      [],
      False,
    ),
    ('at_errors', [{'op': 'at', 'command': cmd} for cmd in ('AT+BAD', 'AT+PLAINERROR', 'AT+BLANK', 'AT+TIMEOUT')], {}, [], False),
    ('lpa_lock', [{'op': 'at', 'command': 'AT+CGMI'}, initialize], {}, [], True),
    ('echo_retry', [initialize], {'AT+CGMI': ['AT+CGMI', 'Quectel']}, [], False),
    ('identity_retry', [initialize], {'AT+CGSN': ['invalid']}, [], False),
    ('absent_sim', [initialize, step('SEARCHING')], {'AT+QCCID': ['ERROR'], 'AT+CIMI': ['ERROR']}, [], False),
    ('roaming_blocked', [initialize, step('SEARCHING')], {'AT+QCCID': ['+QCCID: 1234567890'], 'AT+CREG?': ['+CREG: 2,5']}, [], False),
    (
      'poll_errors',
      [{'op': 'poll'}],
      {
        'AT+CSQ': ['+CSQ: 99,99'],
        'AT+QTEMP': ['+QTEMP: bad'],
        'AT+QNWINFO': ['+QNWINFO: "LTE","45008","LTE BAND 3",bad'],
        'AT+COPS?': ['+COPS: 0,0,"Fixture",bad'],
      },
      ['no-ip'],
      False,
    ),
    (
      'retry_limit',
      [initialize, step('CONNECTING')] + sum(([{'op': 'wait_exit'}, step('CONNECTED')] for _ in range(3)), []) + [step('DISCONNECTING')],
      {},
      ['fail-ppp'],
      False,
    ),
    (
      'route_failure_retry',
      [
        initialize,
        step('CONNECTING'),
        {'op': 'sleep', 'ms': 150},
        {'op': 'poll'},
        {'op': 'wait_exit'},
        step('CONNECTED'),
        {'op': 'sleep', 'ms': 500},
        {'op': 'kill'},
      ],
      {},
      ['fail-route'],
      False,
    ),
    ('route_failure', [{'op': 'routes', 'ip': '10.0.0.2', 'peer': '10.0.0.1'}], {}, ['fail-route'], False),
    ('invalid_route', [{'op': 'routes', 'ip': 'bad', 'peer': '10.0.0.1'}], {}, [], False),
    ('dns_failure', [{'op': 'dns', 'servers': ['1.1.1.1']}], {}, ['fail-dns'], False),
  ]
  reports = []
  for name, operations, changes, flags, held in cases:
    with tempfile.TemporaryDirectory(prefix='modem-oracle-') as directory:
      root = Path(directory)
      outputs = []
      for label, command in [('source', [sys.executable, str(Path(__file__).with_name('modem_source.py'))]), ('native', [str(args.trace)])]:
        folder = root / label
        folder.mkdir()
        outputs.append(trace(command, folder, operations, changes, flags, held))
      (evidence / (name + '.json')).write_text(json.dumps({'source': outputs[0], 'native': outputs[1]}, indent=2))
      assert outputs[0] == outputs[1], name + ' source/native mismatch; inspect artifact'
      reports.append({'scenario': name, 'pass': True, 'artifact': name + '.json'})
  return reports


def lifecycle(args, evidence):
  with tempfile.TemporaryDirectory(prefix='modem-lifecycle-') as directory:
    root = Path(directory)
    fixture = Fixture(root)
    config = root / 'config.json'
    config.write_text(json.dumps(fixture.config))
    state = root / 'state'
    records = []
    process = None
    try:
      with (evidence / 'daemon.stderr').open('w') as stderr:
        process = subprocess.Popen([str(args.binary), '--config', str(config)], stderr=stderr, stdout=subprocess.DEVNULL)

        def connected():
          value = snapshot(state)
          return value if value.get('connected') and value.get('state') == 'CONNECTED' else None

        records.append(wait_for(connected, 'initial PPP connection'))
        native_exe = str(Path(f'/proc/{process.pid}/exe').resolve())
        assert Path(native_exe) == args.binary.resolve()
        assert records[-1]['signal_strength'] == 23 and records[-1]['rx_bytes'] == 456
        assert state.stat().st_mode & 0o777 == 0o644
        assert records[-1]['seconds_since_boot'] > 0
        (root / 'params/GsmApn').write_text('new.apn')
        wait_for(lambda: 'AT+CGDCONT=1,"IP","new.apn"' in fixture.commands, 'APN reconnect')
        records.append(wait_for(connected, 'APN reconnected'))
        fixture.responses['AT+QCCID'] = ['+QCCID: 8985235999999999999F']
        wait_for(lambda: snapshot(state).get('iccid') == '8985235999999999999', 'ICCID reconnect')
        records.append(wait_for(connected, 'SIM reconnected'))
        (root / 'no-ip').touch()
        wait_for(lambda: snapshot(state).get('connected') is False, 'interface disappearance')
        records.append(snapshot(state))
        fixture.at.unlink()
        wait_for(lambda: snapshot(state).get('state') == 'INITIALIZING', 'port disappearance')
        records.append(snapshot(state))
        process.send_signal(signal.SIGTERM)
        assert process.wait(timeout=10) == 0
        assert not state.exists(), 'state remains after clean shutdown'
        pid = int((root / 'ppp.pid').read_text())
        assert not Path(f'/proc/{pid}').exists(), 'owned PPP child remains'
        calls = fixture.calls()
        assert ['systemctl', 'mask', '--runtime', 'ModemManager'] in calls
        assert ['systemctl', 'start', 'ModemManager'] in calls
        assert ['resolvectl', 'dns', 'ppp0', '1.1.1.1', '8.8.8.8'] in calls
        (evidence / 'lifecycle.json').write_text(
          json.dumps(
            {
              'snapshots': records,
              'at_commands': fixture.commands,
              'calls': calls,
              'native_executable': native_exe,
              'exit_code': process.returncode,
              'state_removed': True,
              'child_reaped': True,
            },
            indent=2,
          )
        )
    finally:
      if process is not None and process.poll() is None:
        process.kill()
        process.wait()
      fixture.close()
  return {'scenario': 'continuous_native_pty_lifecycle', 'pass': True, 'artifact': 'lifecycle.json'}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  report = differential(args, args.evidence)
  report.append(lifecycle(args, args.evidence))
  (args.evidence / 'results.json').write_text(json.dumps(report, indent=2))
  print(json.dumps({'passed': len(report), 'evidence': str(args.evidence)}))


if __name__ == '__main__':
  main()
