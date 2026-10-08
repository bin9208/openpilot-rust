"""Original/native Software layout pixels, updater requests and Params transitions."""

import argparse
import copy
import datetime
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
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]


def click(frame, x, y):
  return [
    {'frame': frame + i, 'events': [{'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': i == 0, 'released': i == 1, 'down': i == 0, 'time': (frame + i) / 20}]}
    for i in range(2)
  ]


def base():
  return {
    'kind': 'software',
    'config': {'big': True, 'large_viewport': True, 'pc': True, 'scale': 1.0},
    'language': 'en',
    'rect': {'x': 0, 'y': 0, 'width': 2160, 'height': 1080},
    'frames': 40,
    'prime': -2,
    'params': {},
    'capture_effects': True,
    'steps': [],
  }


cases = []
a = base()
cases.append(('empty', a))
a = base()
a['params'] = {
  'UpdaterCurrentDescription': 'dev / 2026.10.01',
  'UpdaterCurrentReleaseNotes': '<h1>Release</h1><p>Native UI &amp; original behavior.</p>',
  'UpdaterNewDescription': 'next / 2026.10.02',
  'UpdateAvailable': '1',
  'UpdaterNewReleaseNotes': '<p>Install notes</p>',
}
a['steps'] = click(10, 100, 85) + click(20, 100, 610)
cases.append(('release-notes', a))
a = base()
a['params'] = {'UpdaterState': 'idle'}
a['frames'] = 240
a['steps'] = click(10, 2040, 255) + click(30, 2040, 255) + click(208, 2040, 255) + click(212, 2040, 255)
cases.append(('check-idle-timeout', a))
a = base()
a['params'] = {'UpdaterFetchAvailable': '1'}
a['steps'] = click(10, 2040, 255) + click(20, 2040, 255) + [{'frame': 25, 'params': {'UpdaterState': 'downloading...'}}]
cases.append(('download-busy', a))
a = base()
a['params'] = {'UpdaterState': 'checking...'}
a['steps'] = click(10, 2040, 255) + [{'frame': 15, 'params': {'UpdaterState': 'idle', 'UpdateFailedCount': '9999999999999999999999999'}}] + click(20, 2040, 255)
cases.append(('failed-retry', a))
a = base()
a['params'] = {'UpdaterState': 'custom-state'}
cases.append(('unknown-state', a))
a = base()
a['params'] = {'UpdaterState': 'finalizing update...', 'UpdateAvailable': '1'}
a['steps'] = [{'frame': 5, 'started': True}] + click(10, 2040, 425)
cases.append(('onroad-update', a))
a = base()
a['params'] = {'UpdateAvailable': '1'}
a['steps'] = click(10, 2040, 425)
cases.append(('install', a))
a = base()
a['steps'] = click(10, 2040, 595) + [{'frame': 15, 'confirm': False}] + click(20, 2040, 595) + [{'frame': 25, 'engaged': True, 'confirm': True}]
cases.append(('uninstall-cancel-confirm', a))
a = base()
a['params'] = {
  'GitBranch': 'feature',
  'UpdaterAvailableBranches': 'x,master,feature,nightly,devel,,nightly-dev,devel-staging,x',
  'UpdaterTargetBranch': 'feature',
}
a['steps'] = (
  click(10, 2040, 425)
  + [{'frame': 15, 'selection': 'master', 'confirm': False}]
  + click(20, 2040, 425)
  + [{'frame': 25, 'selection': 'nightly', 'confirm': True}]
)
cases.append(('branch-order-selection', a))
a = base()
a['params'] = {'IsTestedBranch': '1'}
cases.append(('tested-branch-hidden', a))
now = datetime.datetime(2026, 10, 1, 12, 34, 56).astimezone()
for seconds in [-100, 0, 59, 60, 119, 120, 3599, 3600, 86399, 86400, 604799, 604800]:
  a = base()
  a['frames'] = 10
  a['params'] = {'LastUpdateTime': (now - datetime.timedelta(seconds=seconds)).isoformat()}
  cases.append((f'ago-{seconds}', a))
a = base()
a['params'] = {'LastUpdateTime': '2026-10-01T12:34:56'}
a['time_valid'] = False
cases.append(('invalid-clock-date', a))
a = base()
a['params'] = {'LastUpdateTime': 'not-a-date', 'UpdateFailedCount': 'not-an-int'}
cases.append(('invalid-params', a))
a = base()
a["params"] = {"LastUpdateTime": "0001-01-01T00:00:00"}
a["time_valid"] = False
cases.append(("early-year-date", a))
results = []
for language in ['en', 'ko']:
  for label, template in cases:
    scene = copy.deepcopy(template)
    scene['language'] = language
    name = f'{label}-{language}'
    path = args.output / f'{name}.json'
    path.write_text(json.dumps(scene))
    images = []
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
        images.append(np.asarray(Image.open(output)).astype(int))
        states.append(json.loads(output.with_suffix('.json').read_text()))
    for index, (source, native) in enumerate(zip(*states, strict=True)):
      assert source == native, (name, index, source, native)
    delta = abs(images[0] - images[1])
    row = {'scene': name, 'frames': len(states[0]), 'different_pixels': int(np.any(delta != 0, axis=-1).sum()), 'max_channel_difference': int(delta.max())}
    results.append(row)
    print(json.dumps(row), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert all(row['different_pixels'] == 0 for row in results), results
print(f'PASS: {len(results)} original/native Software scenarios with exact Params/effects and pixels')
