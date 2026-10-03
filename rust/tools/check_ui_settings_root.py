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


def base(big):
  return {
    'kind': 'settings-root',
    'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0},
    'language': 'en',
    'rect': {'x': 0, 'y': 0, 'width': 2160 if big else 536, 'height': 1080 if big else 240},
    'frames': 40,
    'prime': 0,
    'params': {'DongleId': 'fixture', 'UpdaterState': 'idle'},
    'capture_effects': True,
    'steps': [],
    'models': {'compiled': True, 'compile_pending': False},
    'egpu': {'devices': []},
    'wifi': {
      'networks': [],
      'wifi_state': {'ssid': None, 'status': 'Disconnected'},
      'ipv4_address': '',
      'current_network_metered': 'Unknown',
      'connecting_to_ssid': None,
      'connected_ssid': None,
      'tethering_password': 'owned-password',
      'tethering_active': False,
      'saved_ssids': [],
    },
  }


def click(frame, x, y):
  return [
    {'frame': frame + i, 'events': [{'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': i == 0, 'released': i == 1, 'down': i == 0, 'time': (frame + i) / 20}]}
    for i in range(2)
  ]


cases = []
for index, panel in enumerate(['Device', 'Network', 'Toggles', 'Software', 'Firehose', 'Developer', 'Egpu']):
  scene = base(True)
  scene['steps'] = click(10, 250, 355 + index * 110)
  cases.append((f'big-{panel}', scene, panel, None))
scene = base(True)
scene['steps'] = click(10, 250, 160)
cases.append(('big-close', scene, 'Device', {'page': 'Home'}))
scene = base(True)
scene['frames'] = 100
scene['steps'] = click(10, 250, 465) + click(20, 1900, 95) + click(35, 2000, 760) + click(55, 250, 355) + click(70, 250, 465)
cases.append(('big-network-params-reshow', scene, 'Network', None))
for index, panel in enumerate(['Toggles', 'Network', 'Device', 'Pair', 'Firehose', 'Egpu', 'Developer']):
  scene = base(False)
  scene['steps'] = [{'frame': 10, 'scroll': index * 422}] + click(25, 220, 120)
  if panel == 'Pair':
    scene['prime'] = -2
  cases.append((f'mici-{panel}', scene, None, {'page': 'Pairing' if panel == 'Pair' else f'Settings({panel})'}))
for mode in ['absent', 'present', 'branch']:
  scene = base(False)
  scene['models']['compiled'] = False
  if mode == 'present':
    scene['params']['UsbGpuPresent'] = '1'
  if mode == 'branch':
    scene['params']['GitBranch'] = 'carrot-egpu'
  scene['steps'] = [{'frame': 10, 'scroll': 2200}]
  cases.append((f'mici-egpu-{mode}', scene, None, None))

results = []
for language in ['en', 'ko']:
  for label, template, panel, effect in cases:
    name = f'{label}-{language}'
    if args.filter and args.filter not in name:
      continue
    scene = copy.deepcopy(template)
    scene['language'] = language
    scene['capture_frames'] = list(range(scene['frames']))
    path = args.output / f'{name}.json'
    path.write_text(json.dumps(scene))
    traces = []
    with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-settings148-', dir='/dev/shm') as namespace:
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
        trace = json.loads(output.with_suffix('.json').read_text())
        assert trace[-1]['settings'] == panel, (name, lane, trace[-1]['settings'], panel)
        if effect:
          assert effect in trace[-1]['effects'], (name, lane, trace[-1]['effects'], effect)
        if 'params-reshow' in name:
          assert trace[-1]['params']['GsmRoaming'] == '1', (name, lane, trace[-1]['params'])
        traces.append(trace)
    for frame, (source, native) in enumerate(zip(*traces, strict=True)):
      assert source == native, (name, frame, source, native)
    differences = []
    for frame in scene['capture_frames']:
      images = [np.asarray(Image.open(args.output / f'{lane}-{name}-frame-{frame:04}.png')).astype(int) for lane in ['source', 'native']]
      delta = abs(images[0] - images[1])
      if delta.any():
        differences.append({'frame': frame, 'pixels': int(np.any(delta != 0, axis=-1).sum()), 'max_channel': int(delta.max())})
    row = {'scene': name, 'frames': len(traces[0]), 'different_frames': differences}
    results.append(row)
    print(json.dumps(row), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert results and all(not row['different_frames'] for row in results), results
print(f'PASS: {len(results)} settings-root source/native cases; exact frames, panel changes, actions and Params')
