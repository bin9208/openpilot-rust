"""Actual settings widgets: injected touch frames, modal outcomes, Params and exact pixels."""

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
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]


def touch(frame, x, y, phase):
  return {
    'frame': frame,
    'events': [
      {'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': phase == 'press', 'released': phase == 'release', 'down': phase != 'release', 'time': frame / 20}
    ],
  }


def click(frame, x, y):
  return [touch(frame, x, y, 'press'), touch(frame + 1, x, y, 'release')]


def base(kind='toggles', big=True):
  return {
    'kind': kind,
    'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0},
    'language': 'en',
    'rect': {'x': 0, 'y': 0, 'width': 2160 if big else 536, 'height': 1080 if big else 240},
    'frames': 80,
    'prime': -2,
    'params': {},
    'capture_effects': True,
    'steps': [],
  }


def car(alpha=False, long=True):
  return {'alpha_longitudinal_available': alpha, 'openpilot_longitudinal_control': long, 'max_lateral_accel': 2.2}


cases = []
for big in [True, False]:
  a = base('developer', big)
  cases.append(('developer-big-empty' if big else 'developer-mici-empty', a))
  a = base('developer', big)
  a['params'] = {'IsReleaseBranch': '1', 'AlphaLongitudinalEnabled': '1', 'LongitudinalManeuverMode': '1', 'ShowDebugInfo': '1'}
  a['car'] = car(alpha=True)
  cases.append(('developer-big-release' if big else 'developer-mici-release', a))
  a = base('developer', big)
  a['car'] = car(alpha=True)
  a['params'] = {'AlphaLongitudinalEnabled': '1', 'LongitudinalManeuverMode': '1'}
  a['steps'] = [{'frame': 10, 'started': True}, {'frame': 30, 'started': False, 'params': {'AdbEnabled': '1', 'SshEnabled': '1'}}]
  cases.append(('developer-big-transitions' if big else 'developer-mici-transitions', a))
for y, name in [(598, 'joystick'), (769, 'longitudinal')]:
  a = base('developer')
  a['frames'] = 12
  a['car'] = car()
  a['params'] = {'JoystickDebugMode': '1', 'LongitudinalManeuverMode': '1'}
  a['steps'] = click(10, 2040, y)
  cases.append((f'big-mutual-same-frame-{name}', a))
for x, name in [(100, 'joystick'), (400, 'longitudinal')]:
  a = base('developer', False)
  a['frames'] = 37
  a['car'] = car(alpha=True)
  a['params'] = {'AlphaLongitudinalEnabled': '1', 'JoystickDebugMode': '1', 'LongitudinalManeuverMode': '1'}
  a['steps'] = [{'frame': 15, 'scroll': 1000}] + click(35, x, 120)
  cases.append((f'mici-mutual-same-frame-{name}', a))
a = base('developer')
a['car'] = car(alpha=True)
a['steps'] = click(10, 2040, 940) + [{'frame': 15, 'confirm': False}] + click(20, 2040, 940) + [{'frame': 25, 'confirm': True}] + click(35, 2040, 940)
cases.append(('big-alpha-confirm-cancel-disable', a))
a = base('developer', False)
a['car'] = car(alpha=True)
a['steps'] = [{'frame': 15, 'scroll': 1666}] + click(35, 200, 120) + click(45, 200, 120)
cases.append(('mici-alpha-enable-disable', a))
a = base('developer')
a['steps'] = click(10, 2040, 85) + click(20, 2040, 256) + click(30, 2040, 940) + [{'frame': 40, 'started': True}] + click(45, 2040, 85)
cases.append(('big-basic-debug-onroad-adb', a))
a = base('developer', False)
a['steps'] = click(25, 100, 120) + click(35, 300, 120) + [{'frame': 40, 'scroll': 1686}] + click(55, 200, 120)
cases.append(('mici-basic-debug', a))
a = base('developer', False)
a['params'] = {'GithubUsername': 'old-user', 'GithubSshKeys': 'ssh-ed25519 old-key'}
a['steps'] = [{'frame': 15, 'scroll': 400}] + click(35, 200, 120) + [{'frame': 40, 'input_text': ''}]
cases.append(('mici-ssh-clear', a))
a = base('developer', False)
a['time_valid'] = False
a['steps'] = [{'frame': 15, 'scroll': 400}] + click(35, 200, 120)
cases.append(('mici-ssh-clock', a))
a = base('developer')
a['params'] = {'GithubUsername': 'old-user', 'GithubSshKeys': 'ssh-ed25519 old-key'}
a['steps'] = click(10, 2040, 427)
cases.append(('big-ssh-remove', a))
a = base('developer')
a['steps'] = click(10, 2040, 427) + [{'frame': 15, 'confirm': False}]
cases.append(('big-ssh-input-cancel', a))
for big in [True, False]:
  for username in ['fixture-user', 'empty-user', 'missing-user', 'pending-user']:
    a = base('developer', big)
    a['steps'] = click(10, 2040, 427) if big else [{'frame': 15, 'scroll': 400}] + click(35, 200, 120)
    a['steps'] += [{'frame': 40, 'input_text': (' ' + username + ' ') if big else username, 'flush_ssh': username != 'pending-user'}]
    cases.append((f'ssh-fetch-{"big" if big else "mici"}-{username}', a))
from ui_application_qa.ssh_server import Server

with Server() as server:
  results = []
  for language in ['en', 'ko']:
    for label, template in cases:
      scene = copy.deepcopy(template)
      scene['language'] = language
      name = f'{label}-{language}'
      if name.startswith('ssh-fetch-'):
        scene['ssh_host'] = server.host
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
  print(f'PASS: {len(results)} original/native settings scenarios with exact Params/effects/personality traces and final pixels')
  (args.output / 'http-requests.json').write_text(json.dumps(server.requests, indent=2))
