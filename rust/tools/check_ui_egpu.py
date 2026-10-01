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


def device(speed=5000, product='custom ed4e39b7-CLEAN'):
  return {
    'sysfs_name': 'owned-usb',
    'vendor_id': 0xADD1,
    'product_id': 1,
    'speed_mbps': speed,
    'manufacturer': 'fixture',
    'product': product,
    'busnum': 1,
    'devnum': 1,
    'link_error_count': 0,
  }


def base(big):
  return {
    'kind': 'egpu',
    'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0},
    'language': 'en',
    'rect': {'x': 0, 'y': 0, 'width': 2160 if big else 536, 'height': 1080 if big else 240},
    'frames': 20,
    'prime': 0,
    'params': {},
    'capture_effects': True,
    'steps': [],
    'models': {'compiled': True, 'compile_pending': False},
    'egpu': {'devices': [device()]},
  }


def click(frame, x, y):
  return [
    {'frame': frame + i, 'events': [{'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': i == 0, 'released': i == 1, 'down': i == 0, 'time': (frame + i) / 20}]}
    for i in range(2)
  ]


cases = []
for big in [True, False]:
  layout = 'big' if big else 'mici'
  for label in ['absent', 'multiple', 'slow', 'firmware', 'failed', 'loading', 'pending', 'active', 'uncompiled', 'ready']:
    scene = base(big)
    match label:
      case 'absent':
        scene['egpu']['devices'] = []
      case 'multiple':
        scene['egpu']['devices'] *= 2
      case 'slow':
        scene['egpu']['devices'] = [device(480)]
      case 'firmware':
        scene['egpu']['devices'] = [device(product='wrong firmware')]
      case 'failed':
        scene['params'] = {'UsbGpuStartupFailed': '1', 'UsbGpuActive': '1'}
      case 'loading':
        scene['params'] = {'UsbGpuLoading': '1', 'UsbGpuActive': '1'}
      case 'pending':
        scene['models']['compile_pending'] = True
      case 'active':
        scene['params'] = {'UsbGpuActive': '1'}
      case 'uncompiled':
        scene['models']['compiled'] = False
      case 'ready':
        pass
    cases.append((f'{layout}-{label}', scene, {'calls': 0, 'removals': 0}))
  for speed in [999, 1000, 5500]:
    scene = base(big)
    scene['egpu']['devices'] = [device(speed)]
    cases.append((f'{layout}-link-{speed}', scene, {'calls': 0, 'removals': 0}))
  for label, error in [('success', None), ('failure', '12V / PCIe not ready'), ('onroad', None), ('pending', None)]:
    scene = base(big)
    scene['frames'] = 65
    scene['egpu']['check_error'] = error
    scene['egpu']['complete_at'] = None if label == 'pending' else 40
    if not big:
      scene['steps'].append({'frame': 10, 'scroll': 360})
    scene['steps'] += click(20, 2000 if big else 220, 595 if big else 120)
    scene['steps'] += click(30, 2000 if big else 220, 595 if big else 120)
    if label == 'onroad':
      scene['steps'].insert(0, {'frame': 0, 'started': True})
    cases.append((f'{layout}-check-{label}', scene, {'calls': 0 if label == 'onroad' else 1, 'removals': 0}))
for confirm in [True, False]:
  scene = base(False)
  scene['models']['compiled'] = False
  scene['frames'] = 60
  scene['steps'] = [{'frame': 10, 'scroll': 800}] + click(25, 220, 120) + [{'frame': 40, 'confirm': confirm}]
  cases.append((f'mici-compile-{confirm}', scene, {'calls': 0, 'removals': int(confirm)}))

results = []
for language in ['en', 'ko']:
  for label, template, expected in cases:
    name = f'{label}-{language}'
    if args.filter and args.filter not in name:
      continue
    scene = copy.deepcopy(template)
    scene['language'] = language
    scene['capture_frames'] = list(range(scene['frames']))
    path = args.output / f'{name}.json'
    path.write_text(json.dumps(scene))
    states = []
    with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-egpu148-', dir='/dev/shm') as namespace:
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
        actual = json.loads(output.with_suffix('.egpu.json').read_text())
        assert actual == expected, (name, lane, actual, expected)
    for index, (source, native) in enumerate(zip(*states, strict=True)):
      assert source == native, (name, index, source, native)
    frames = []
    for frame in scene['capture_frames']:
      images = [np.asarray(Image.open(args.output / f'{lane}-{name}-frame-{frame:04}.png')).astype(int) for lane in ['source', 'native']]
      delta = abs(images[0] - images[1])
      if delta.any():
        frames.append({'frame': frame, 'different_pixels': int(np.any(delta != 0, axis=-1).sum()), 'max_channel_difference': int(delta.max())})
    row = {'scene': name, 'frames': len(states[0]), 'different_frames': frames, 'effects': expected}
    results.append(row)
    print(json.dumps(row), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert results and all(not row['different_frames'] for row in results), results
print(f'PASS: {len(results)} source/native eGPU scenarios; exact frames, effects and worker-call counts')
