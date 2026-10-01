#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["jeepney==0.9.0", "pyzmq==27.2.0", "numpy==2.5.3"]
# ///
import argparse
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import sys
import tempfile
import time
import uuid

from wifi_fixture.service import Service

FIXTURE = Path(__file__).with_name('wifi_fixture')
MUTATIONS = {'AddConnection', 'AddAndActivateConnection2', 'ActivateConnection', 'Save', 'Update', 'Delete', 'DeactivateConnection'}


def scenario(args, side, hotspot):
  output = args.output / side / ('existing' if hotspot else 'create')
  output.mkdir(parents=True)
  with tempfile.TemporaryDirectory(prefix='wifi-qa-') as temporary:
    root = Path(temporary)
    prefix = 'wifi-' + uuid.uuid4().hex[:16]
    (root / 'bin').mkdir()
    shutil.copyfile(FIXTURE / 'sudo.py', root / 'bin/sudo')
    (root / 'bin/sudo').chmod(0o700)
    params = root / 'params' / prefix
    params.mkdir(parents=True)
    (params / 'DongleId').write_text('test-dongle')
    peer_root = root / 'peer'
    peer_root.mkdir()
    bus = subprocess.Popen(['dbus-daemon', '--session', '--nofork', '--print-address=1'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    assert select.select([bus.stdout], [], [], 5)[0]
    address = bus.stdout.readline().strip()
    assert address.startswith('unix:')
    service = Service(address, hotspot)
    process = None
    records, events, gate_observations = [], [], []
    gate_active = False
    try:
      service.start()
      environment = os.environ | {'DBUS_SYSTEM_BUS_ADDRESS': address, 'PARAMS_ROOT': str(root / 'params'), 'OPENPILOT_PREFIX': prefix,
                                  'PATH': str(root / 'bin') + os.pathsep + os.environ['PATH'], 'WIFI_COMMAND_RECORD': str(root / 'sysctl.json')}
      command = [str(args.binary)] if side == 'native' else [sys.executable, str(FIXTURE / 'source_peer.py'), str(args.binding), str(peer_root)]
      with (output / 'stderr.log').open('w') as stderr:
        process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, env=environment, text=True)
        executable = str(Path(f'/proc/{process.pid}/exe').resolve())
        if side == 'native':
          assert executable == str(args.binary), executable
        start = {'address': address, 'launcher': str(args.launcher)}
        if args.defer_final_forgotten and side == 'source':
          start['defer_final_forgotten'] = True
        process.stdin.write(json.dumps(start) + '\n')
        process.stdin.flush()

        def request(payload):
          process.stdin.write(json.dumps(payload) + '\n')
          process.stdin.flush()
          assert select.select([process.stdout], [], [], 5)[0], (service.error, (output / 'stderr.log').read_text())
          line = process.stdout.readline()
          assert line, (process.poll(), (output / 'stderr.log').read_text())
          row = json.loads(line)
          events.extend(row.get('events', []))
          if 'callback_gate' in row:
            gate_observations.append({'request': payload, 'gate': row['callback_gate'], 'saved_ssids': row['snapshot']['saved_ssids'],
                                      'forgotten_count': events.count({'Forgotten': 'B'})})
          return row

        def wait_for(predicate, timeout=8):
          deadline = time.monotonic() + timeout
          while True:
            value = request({'op': 'snapshot'})['snapshot']
            if predicate(value):
              return value
            if gate_active and side == 'source' and 'B' not in value['saved_ssids']:
              request({'op': 'release_final_forget'})
            assert time.monotonic() < deadline, (side, value, service.calls[-8:], (output / 'stderr.log').read_text())
            time.sleep(.02)

        def send(name, value):
          request({'op': 'command', 'value': {name: value}})

        def saves():
          return sum(call['method'] == 'Save' for call in service.calls)

        if hotspot:
          records.append(wait_for(lambda s: len(s['networks']) == 3 and 'weedle-test' in s['saved_ssids'] and s['tethering_password'] == 'fixture-password'))
          send('Activate', 'A')
          records.append(wait_for(lambda s: s['connected_ssid'] == 'A' and saves() >= 1 and s['ipv4_address'] == '10.42.0.7'))
          send('Connect', {'ssid': 'B', 'password': 'fixture-secret', 'hidden': True})
          records.append(wait_for(lambda s: s['connected_ssid'] == 'B' and saves() >= 2))
          send('SetCurrentNetworkMetered', 'Yes')
          wait_for(lambda _: any(call['method'] == 'Update' for call in service.calls))
          send('SetActive', True)
          records.append(wait_for(lambda s: s['current_network_metered'] == 'Yes'))
          service.control.put('wrong-password')
          records.append(wait_for(lambda s: s['wifi_state']['ssid'] is None and {'NeedAuth': 'B'} in events))
          send('SetTetheringActive', True)
          records.append(wait_for(lambda s: s['connected_ssid'] == 'weedle-test' and saves() >= 3 and (root / 'sysctl.json').exists()))
          assert json.loads((root / 'sysctl.json').read_text()) == ['sysctl', 'net.ipv4.ip_forward=0']
          send('SetTetheringPassword', 'fixture-new-password')
          records.append(wait_for(lambda s: s['tethering_password'] == 'fixture-new-password' and s['connected_ssid'] == 'weedle-test' and saves() >= 4))
          send('SetTetheringActive', False)
          records.append(wait_for(lambda s: s['wifi_state']['ssid'] is None))
          forgotten_before = events.count({'Forgotten': 'B'})
          if args.defer_final_forgotten and side == 'source':
            request({'op': 'arm_final_forget'})
            gate_active = True
          send('Forget', 'B')
          records.append(wait_for(lambda s: 'B' not in s['saved_ssids'] and events.count({'Forgotten': 'B'}) > forgotten_before))
        else:
          wait_for(lambda _: any(call['method'] == 'AddConnection' for call in service.calls))
        started = time.monotonic()
        assert request({'op': 'stop'}) == {'stopped': True}
        assert process.wait(timeout=5) == 0, (output / 'stderr.log').read_text()
        elapsed = time.monotonic() - started
      normalized_events = [event for event in events if not isinstance(event, dict) or 'NetworksUpdated' not in event]
      result = {'snapshots': records, 'events': normalized_events,
                'mutations': [call for call in service.calls if call['method'] in MUTATIONS]}
      (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
      execution = {'argv': command, 'executable': executable, 'shutdown_seconds': elapsed, 'scans': service.scans}
      (output / 'execution.json').write_text(json.dumps(execution, indent=2))
      print('PASS', side, 'existing' if hotspot else 'create', flush=True)
      return result
    finally:
      (output / 'callback-gate.json').write_text(json.dumps(gate_observations, indent=2) + '\n')
      (output / 'calls.json').write_text(json.dumps(service.calls, indent=2) + '\n')
      if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=3)
      try:
        service.close()
      finally:
        bus.terminate()
        bus.wait(timeout=3)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--launcher', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--defer-final-forgotten', action='store_true')
  args = parser.parse_args()
  args.binary, args.launcher, args.binding, args.output = [path.resolve() for path in (args.binary, args.launcher, args.binding, args.output)]
  for hotspot in (True, False):
    expected = scenario(args, 'source', hotspot)
    actual = scenario(args, 'native', hotspot)
    assert expected == actual, ('source/native lifecycle mismatch', args.output)
  result = {'exact': True, 'private_bus_scenarios': 2, 'normal_snapshots': 9, 'owned_sysctl_only': True}
  (args.output / 'comparison.json').write_text(json.dumps(result, indent=2))
  print('PASS source/native private D-Bus methods, settings, signals, snapshots and cleanup')


if __name__ == '__main__':
  main()
