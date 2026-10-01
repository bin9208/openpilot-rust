#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["jeepney==0.9.0"]
# ///
# Run with PYTHONPATH=. uv run rust/tools/check_wifi_policy.py --binary TARGET/wifi_trace --output EVIDENCE
import argparse
from dataclasses import asdict
import hashlib
import importlib
import itertools
import json
from pathlib import Path
import subprocess
import sys
from types import ModuleType, SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]


def load_source():
  module = ModuleType('openpilot.common.swaglog')
  module.cloudlog = SimpleNamespace(warning=lambda *args: None, exception=lambda *args: None, debug=lambda *args: None)
  sys.modules[module.__name__] = module
  return importlib.import_module('openpilot.system.ui.lib.wifi_manager')


def cases():
  rows = []
  states = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 255]
  statuses = ['Disconnected', 'Connecting', 'Connected']
  for index, (state, status, reason, ssid) in enumerate(itertools.product(states, statuses, [0, 7, 8, 38, 39, 60], [None, 'A', ''])):
    connection = [None, '/A', '/missing'][index % 3]
    during = [['B'], [None], []][index % 3]
    rows.append({'dongle': 'fixture', 'connections': [['A', '/A'], ['B', '/B']], 'actions': [
      {'op': 'set_state', 'ssid': ssid, 'status': status},
      {'op': 'signal', 'current': state, 'previous': 50 if index % 2 else 30, 'reason': reason, 'connection': connection, 'during': during},
      {'op': 'initial', 'state': state, 'connection': connection, 'during': during},
    ]})
  rows.append({'dongle': None, 'connections': [['A', '/A'], ['B', '/B']], 'actions': [
    {'op': 'connect', 'ssid': 'B'}, {'op': 'remove', 'path': '/A'},
    {'op': 'signal', 'current': 30, 'previous': 110, 'reason': 38, 'connection': None, 'during': []},
    {'op': 'new', 'ssid': 'A', 'path': '/newA'}, {'op': 'new', 'ssid': 'B', 'path': '/newB'},
    {'op': 'networks', 'networks': [{'ssid': name, 'strength': strength, 'security_type': 'Open', 'is_tethering': False}
                                  for name, strength in [('b', 99), ('B', 20), ('A', 30), ('a', 30), ('가', 80), ('É', 80)]]},
  ]})
  for status, active, elapsed in itertools.product(statuses, [True, False], [4.9999, 5., 5.0001, 60.]):
    rows.append({'dongle': '가나다라마', 'connections': [], 'actions': [
      {'op': 'set_state', 'ssid': 'A', 'status': status}, {'op': 'scan', 'active': active, 'last': 100., 'now': 100. + elapsed},
    ]})
  return rows


def network(value):
  result = asdict(value)
  result['security_type'] = value.security_type.name.capitalize()
  return result


def source(module, case):
  wm = module.WifiManager.__new__(module.WifiManager)
  wm._exit = True
  wm._connections = dict(case['connections'])
  wm._wifi_state = module.WifiState()
  wm._user_epoch = 0
  wm._networks = []
  wm._ipv4_address = wm._tethering_password = ''
  wm._current_network_metered = module.MeteredType.UNKNOWN
  wm._tethering_ssid = 'weedle' + ('-' + case['dongle'][:4] if case['dongle'] else '')
  wm._wifi_device = '/device'
  wm._callback_queue = []
  events, saves = [], []
  wm._need_auth = [lambda ssid: events.append({'NeedAuth': ssid})]
  wm._activated = [lambda: events.append('Activated')]
  wm._forgotten = wm._networks_updated = wm._disconnected = []
  counters = {'lookups': 0, 'updates': 0}
  action = {}

  def lookup(*args):
    counters['lookups'] += 1
    for ssid in action['during']:
      wm._set_connecting(ssid)
    return action['connection'], {}

  def update():
    counters['updates'] += 1

  def save(message):
    saves.append(str(message.header.fields[1]))
    return SimpleNamespace(header=SimpleNamespace(message_type=module.MessageType.method_return))

  wm._get_active_wifi_connection = lookup
  wm._update_active_connection_info = update
  wm._conn_monitor = SimpleNamespace(send_and_get_reply=save)
  wm._router_main = SimpleNamespace(send_and_get_reply=lambda _: SimpleNamespace(body=[('u', action['state'])]))
  rows = []
  for action in case['actions']:
    scan_due = None
    match action['op']:
      case 'set_state':
        wm._wifi_state = module.WifiState(action['ssid'], module.ConnectStatus[action['status'].upper()])
      case 'connect':
        wm._set_connecting(action['ssid'])
      case 'signal':
        wm._handle_state_change(action['current'], action['previous'], action['reason'])
      case 'initial':
        wm._init_wifi_state()
      case 'remove':
        wm._connection_removed(action['path'])
      case 'new':
        wm._get_connection_settings = lambda _: {'802-11-wireless': {'ssid': ('ay', action['ssid'].encode())}}
        wm._new_connection(action['path'])
      case 'networks':
        wm._networks = [module.Network(row['ssid'], row['strength'], module.SecurityType[row['security_type'].upper()], row['is_tethering']) for row in action['networks']]
      case 'scan':
        requested = []
        wm._exit, wm._active, wm._last_network_scan = False, action['active'], action['last']
        wm._request_scan = lambda: requested.append(True)
        module.time = SimpleNamespace(monotonic=lambda: action['now'], sleep=lambda _: setattr(wm, '_exit', True))
        wm._network_scanner()
        scan_due = bool(requested)
      case _:
        raise AssertionError(action)
    wm.process_callbacks()
    rows.append({'snapshot': {
      'networks': [network(row) for row in wm.networks],
      'wifi_state': {'ssid': wm.wifi_state.ssid, 'status': wm.wifi_state.status.name.capitalize()},
      'ipv4_address': wm.ipv4_address, 'current_network_metered': wm.current_network_metered.name.capitalize(),
      'connecting_to_ssid': wm.connecting_to_ssid, 'connected_ssid': wm.connected_ssid,
      'tethering_password': wm.tethering_password, 'tethering_active': wm.is_tethering_active(), 'saved_ssids': list(wm._connections),
    }, 'events': events.copy(), 'epoch': wm._user_epoch, 'lookups': counters['lookups'], 'saves': saves.copy(), 'active_updates': counters['updates'], 'scan_due': scan_due})
    events.clear()
  return rows


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  requests = cases()
  module = load_source()
  expected = [source(module, case) for case in requests]
  process = subprocess.run([args.binary.resolve()], input=json.dumps(requests), text=True, capture_output=True, check=True, timeout=30)
  actual = json.loads(process.stdout)
  for name, data in [('input', requests), ('source', expected), ('native', actual)]:
    (args.output / f'{name}.json').write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n')
  for index, (left, right) in enumerate(zip(expected, actual, strict=True)):
    assert left == right, (index, requests[index], left, right)
  report = {'cases': len(requests), 'frames': sum(len(row) for row in expected), 'exact': True,
            'source_sha256': hashlib.sha256((ROOT / 'openpilot/system/ui/lib/wifi_manager.py').read_bytes()).hexdigest()}
  (args.output / 'comparison.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
