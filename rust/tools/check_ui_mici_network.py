"""Compact network source/native pixels on every frame and owned transport actions."""

import argparse
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import numpy as np
from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--display', required=True)
parser.add_argument('--filter', default='')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]


def network(ssid, strength=50, security='Open', tether=False):
  return {'ssid': ssid, 'strength': strength, 'security_type': security, 'is_tethering': tether}


def snapshot(networks=None, ssid=None, status='Disconnected', saved=None, tether=False):
  return {
    'networks': networks or [],
    'wifi_state': {'ssid': ssid, 'status': status},
    'ipv4_address': '192.0.2.2' if status == 'Connected' else '',
    'current_network_metered': 'Unknown',
    'connecting_to_ssid': ssid if status == 'Connecting' else None,
    'connected_ssid': ssid if status == 'Connected' else None,
    'tethering_password': 'old-password',
    'tethering_active': tether,
    'saved_ssids': saved or [],
  }


def base(kind='wifi-mici', wifi=None):
  return {
    'kind': kind,
    'config': {'big': False, 'large_viewport': False, 'pc': True, 'scale': 1.0},
    'language': 'en',
    'rect': {'x': 0, 'y': 0, 'width': 536, 'height': 240},
    'frames': 80,
    'prime': 0,
    'params': {},
    'capture_effects': True,
    'steps': [],
    'wifi': wifi or snapshot(),
  }


def click(frame, x=200, y=120):
  return [
    {'frame': frame + i, 'events': [{'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': i == 0, 'released': i == 1, 'down': i == 0, 'time': (frame + i) / 20}]}
    for i in range(2)
  ]


def scan(frame, state):
  return {'frame': frame, 'wifi': state, 'wifi_events': [{'NetworksUpdated': state['networks']}]}


open_net = network('Cafe', 25)
secure = network('Home’s Wi-Fi', 75, 'Wpa2')
cases = []
cases.append(('wifi-empty', base()))
for strength in [25, 50, 75]:
  a = base(wifi=snapshot([network('강도와 signal', strength, 'Wpa3')]))
  cases.append((f'wifi-strength-{strength}', a))
a = base(wifi=snapshot([network('Old WEP', 90, 'Unsupported')]))
a['steps'] = click(20)
cases.append(('wifi-unsupported', a))
a = base(wifi=snapshot([open_net, secure]))
a['steps'] = click(20)
cases.append(('wifi-open-connect', a))
a = base(wifi=snapshot([open_net, secure], saved=[secure['ssid']]))
a['steps'] = [{'frame': 15, 'scroll': 400}] + click(25)
cases.append(('wifi-saved-move', a))
a = base(wifi=snapshot([open_net, secure]))
a['steps'] = (
  [{'frame': 15, 'scroll': 400}]
  + click(25)
  + [{'frame': 35, 'input_text': 'fixture-password'}, {'frame': 45, 'wifi': snapshot([open_net, secure]), 'wifi_events': [{'NeedAuth': secure['ssid']}]}]
)
cases.append(('wifi-auth-wrong-password', a))
a = base(wifi=snapshot([secure], saved=[secure['ssid']]))
a['steps'] = (
  click(20, 360, 170)
  + [{'frame': 25, 'confirm': True}]
  + click(30, 360, 170)
  + [{'frame': 45, 'wifi': snapshot([secure]), 'wifi_events': [{'Forgotten': secure['ssid']}]}]
)
cases.append(('wifi-forget-once', a))
a = base(wifi=snapshot([secure], ssid=secure['ssid'], status='Connected', saved=[secure['ssid']]))
a['steps'] = click(20, 360, 170) + [{'frame': 25, 'confirm': False}]
cases.append(('wifi-connected-forget-cancel', a))
a = base(wifi=snapshot([open_net, secure]))
new = network('New network', 100, 'Wpa')
a['steps'] = [
  {'frame': 15, 'scroll': 400},
  scan(25, snapshot([open_net, new], ssid=secure['ssid'], status='Connected')),
  {'frame': 35, 'wifi': snapshot([open_net, new])},
  {'frame': 55, 'show_again': True},
]
cases.append(('wifi-missing-reshow', a))
a = base(wifi=snapshot([network('comma', 100, 'Wpa2', True)], ssid='comma', status='Connecting', tether=True))
a['steps'] = [scan(30, snapshot([network('comma', 100, 'Wpa2', True)], ssid='comma', status='Connected', tether=True))]
cases.append(('wifi-tether-status', a))
a = base(wifi=snapshot([network('A very long network name that scrolls beyond the card width', 60, 'Wpa2')]))
a['frames'] = 110
cases.append(('wifi-long-name', a))
a = base('network-mici')
a['prime'] = -2
cases.append(('menu-unpaired', a))
a = base('network-mici', snapshot([secure], ssid=secure['ssid'], status='Connected'))
a['steps'] = click(20)
cases.append(('menu-open-wifi', a))
a = base('network-mici', snapshot([secure], ssid=secure['ssid'], status='Connected'))
a['steps'] = [scan(0, a['wifi']), {'frame': 15, 'scroll': 400}] + click(25) + click(30) + [scan(45, dict(a['wifi'], current_network_metered='Yes'))] + click(50)
cases.append(('menu-metered', a))
a = base('network-mici', snapshot([secure], ssid=secure['ssid'], status='Connected'))
a['steps'] = [scan(0, a['wifi']), {'frame': 15, 'scroll': 800}] + click(25) + click(30) + [scan(45, snapshot(tether=True))] + click(50)
cases.append(('menu-tether', a))
for value in ['', 'new-password']:
  a = base('network-mici')
  a['steps'] = [{'frame': 15, 'scroll': 1200}] + click(25) + [{'frame': 35, 'input_text': value}] + click(45)
  cases.append(('menu-password-' + ('empty' if not value else 'change'), a))
for value in ['  fixture.apn  ', ' \t\x1f']:
  a = base('network-mici')
  a['params'] = {'GsmApn': 'old.apn'}
  a['steps'] = [{'frame': 15, 'scroll': 2000}] + click(25) + [{'frame': 35, 'input_text': value}]
  cases.append(('menu-apn-' + ('set' if value.strip() else 'clear'), a))
for offset, name in [(1600, 'roaming'), (2400, 'cellular-metered')]:
  a = base('network-mici')
  a['steps'] = [{'frame': 15, 'scroll': offset}] + click(25) + [{'frame': 45, 'prime': 1}]
  cases.append(('menu-' + name, a))

results = []
for language in ['en', 'ko']:
  for label, template in cases:
    name = f'{label}-{language}'
    if args.filter and args.filter not in name:
      continue
    scene = copy.deepcopy(template)
    scene['language'] = language
    scene['capture_frames'] = list(range(scene['frames']))
    path = args.output / f'{name}.json'
    path.write_text(json.dumps(scene))
    states = []
    with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-ui148-', dir='/dev/shm') as namespace:
      env = dict(os.environ, DISPLAY=args.display, PYTHONPATH=str(root), OFFSCREEN='1', OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'))
      for lane in ['source', 'native']:
        output = args.output / f'{lane}-{name}.png'
        command = (
          [sys.executable, str(root / 'rust/tools/ui_application_qa/product_source.py'), str(path), str(output)]
          if lane == 'source'
          else [str(args.binary), str(root), str(path), str(output)]
        )
        with (args.output / f'{lane}-{name}.log').open('w') as log:
          subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
        states.append(json.loads(output.with_suffix('.json').read_text()))
    for index, (source, native) in enumerate(zip(*states, strict=True)):
      assert source == native, (name, index, source, native)
    frames = []
    for frame in scene['capture_frames']:
      images = [np.asarray(Image.open(args.output / f'{lane}-{name}-frame-{frame:04}.png')).astype(int) for lane in ['source', 'native']]
      delta = abs(images[0] - images[1])
      if delta.any():
        frames.append({'frame': frame, 'different_pixels': int(np.any(delta != 0, axis=-1).sum()), 'max_channel_difference': int(delta.max())})
    row = {'scene': name, 'frames': len(states[0]), 'different_frames': frames}
    results.append(row)
    print(json.dumps(row), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert results and all(not row['different_frames'] for row in results), results
print(f'PASS: {len(results)} original/native compact network scenarios, every captured frame and owned transport/Params/effect trace identical')
