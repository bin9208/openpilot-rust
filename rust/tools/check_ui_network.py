"""UI-only oracle against unchanged network.py with a typed transport fixture."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import numpy as np
from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--display', required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
env = dict(os.environ, DISPLAY=args.display, OFFSCREEN='1', PYTHONPATH=str(root))
networks = [
  {'ssid': name, 'strength': strength, 'security_type': security, 'is_tethering': False}
  for name, strength, security in [
    ('Open access', 98, 'Open'),
    ('Saved network', 67, 'Wpa'),
    ('Phone’s hotspot', 45, 'Wpa2'),
    ('Unsupported', 20, 'Unsupported'),
  ]
]
snapshot = {
  'networks': networks,
  'wifi_state': {'ssid': 'Saved network', 'status': 'Connected'},
  'ipv4_address': '192.0.2.4',
  'current_network_metered': 'Yes',
  'connecting_to_ssid': None,
  'connected_ssid': 'Saved network',
  'tethering_password': 'fixture-password',
  'tethering_active': False,
  'saved_ssids': ['Saved network'],
}
first = {'events': [{'NetworksUpdated': networks}]}
scenes = [
  ('scanning', 'wifi', [{}, {}]),
  ('list', 'wifi', [first, {}, {}]),
  ('connecting', 'wifi', [first, {'operation': 'choose', 'ssid': 'Open access'}, {}, {}]),
  ('password', 'wifi', [first, {'operation': 'choose', 'ssid': 'Phone’s hotspot'}, {}, {}]),
  ('forget', 'wifi', [first, {'operation': 'forget', 'ssid': 'Saved network'}, {}, {}]),
  ('advanced', 'advanced', [first, {}, {}]),
  ('panel', 'network', [first, {}, {}]),
]


def touch(x, y, pressed=False, released=False):
  return {'touch': [{'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': pressed, 'released': released, 'down': pressed, 'time': 1.0}]}


password_frames = (
  [first, touch(200, 400, True), touch(200, 400, released=True), {}]
  + [{'chars': c} for c in 'abcdefgh']
  + [touch(1900, 950, True), touch(1900, 950, released=True), {}, {}]
)
scenes.append(('password-input', 'wifi', password_frames))
tethered = {**snapshot, 'tethering_active': True}
scenes.append(
  (
    'tether-input',
    'advanced',
    [first, touch(2050, 80, True), touch(2050, 80, released=True), {}, {'snapshot': tethered, 'events': [{'NetworksUpdated': networks}]}, {}],
  )
)
scenes.append(
  (
    'apn-input',
    'advanced',
    [first, touch(2050, 750, True), touch(2050, 750, released=True), {}]
    + [{'chars': c} for c in '  apn.fixture ']
    + [touch(1900, 950, True), touch(1900, 950, released=True), {}, {}],
  )
)
results = []
for name, kind, frames in scenes:
  scene = {'kind': kind, 'snapshot': snapshot, 'frames': frames}
  path = args.output / f'{name}.json'
  path.write_text(json.dumps(scene))
  images = []
  states = []
  for lane in ['source', 'native']:
    output = args.output / f'{lane}-{name}.png'
    command = (
      [sys.executable, str(root / 'rust/tools/ui_network_source.py'), str(path), str(output)]
      if lane == 'source'
      else [str(args.binary), str(root), str(path), str(output)]
    )
    with (args.output / f'{lane}-{name}.log').open('w') as log:
      subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    images.append(np.asarray(Image.open(output)).astype(int))
    states.append(json.loads(output.with_suffix('.json').read_text()))
  delta = abs(images[0] - images[1])
  result = {
    'scene': name,
    'frames': len(frames),
    'different_pixels': int(np.any(delta != 0, axis=-1).sum()),
    'max_channel_difference': int(delta.max()),
    'state_equal': states[0] == states[1],
  }
  results.append(result)
  print(json.dumps(result), flush=True)
  if not result['state_equal']:
    for index, (source, native) in enumerate(zip(*states, strict=True)):
      if source != native:
        print(json.dumps({'frame': index, 'source': source, 'native': native}), flush=True)
        break
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert all(item['different_pixels'] == 0 and item['state_equal'] for item in results), results
print(f'PASS: {len(results)} source/native network screens')
