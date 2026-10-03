#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp", "pyzmq"]
# ///
# How to run: use the commands and original-binding environment in
# docs/rust-port/soundd-validation.md; never initialize a real audio device.
"""Original soundd bodies against native samples/state; no sounddevice import."""

import argparse
import ast
import functools
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import wave
import uuid

import numpy as np
from openpilot.cereal import car
from original_params_binding import load

ROOT = Path(__file__).resolve().parents[2]


def definitions(path, scope):
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom))]
  exec(compile(tree, str(path), 'exec'), scope)


def source(config, binding, root):
  os.environ['OPENPILOT_PREFIX'] = 'sound-source-' + uuid.uuid4().hex[:16]
  module, cloudlog = load(binding, 'ipc://' + str(root / 'log'), root / 'logs')
  params = module.Params(str(root / 'params'))
  params.put('SoundVolumeAdjustEngage', round(config['engage_volume'] * 100))
  params.put('SoundLanguageSetting', config['language'])
  now = [0.0]
  scope = {
    '__name__': 'soundd_oracle',
    'np': np,
    'math': math,
    'os': os,
    'wave': wave,
    'time': SimpleNamespace(monotonic=lambda: now[0], sleep=lambda _: None),
    'Params': lambda: params,
    'BASEDIR': config.get('basedir', str(root)),
    'car': car,
    'HARDWARE': SimpleNamespace(get_device_type=lambda: 'tizi' if config['tizi'] else 'pc'),
    'micd': SimpleNamespace(SAMPLE_RATE=16000, FFT_SAMPLES=1600),
    'cloudlog': cloudlog.cloudlog,
    'functools': functools,
  }
  tree = ast.parse((ROOT / 'openpilot/common/utils.py').read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'retry']
  exec(compile(tree, 'original-retry', 'exec'), scope)
  definitions(ROOT / 'openpilot/common/filter_simple.py', scope)
  definitions(ROOT / 'openpilot/selfdrive/car/openpilot_toggle.py', scope)
  definitions(ROOT / 'openpilot/selfdrive/ui/soundd.py', scope)
  sound = scope['Soundd']()
  toggle = scope['CruiseMainOpenpilotToggle'](car.CarState.ButtonEvent.Type.mainCruise)
  rows = []

  class Messages(dict):
    pass

  for step in config['steps']:
    value = step['input']
    now[0] = value['now']
    if step['language'] is not None:
      params.put('SoundLanguageSetting', step['language'])
      sound.update_language()
    sm = Messages(
      selfdriveState=SimpleNamespace(enabled=value['enabled'], alertSound=SimpleNamespace(raw=value['alert'])),
      carrotMan=SimpleNamespace(leftSec=value['countdown']),
    )
    sm.updated = {'selfdriveState': value['updated_selfdrive'], 'carrotMan': value['updated_carrot']}
    sm.recv_time = {'selfdriveState': value['selfdrive_received']}
    if value['pressure'] is not None and sound.current_alert == 0:
      sound.spl_filter_weighted.update(value['pressure'])
      sound.current_volume = sound.calculate_volume(float(sound.spl_filter_weighted.x)) * sound.soundVolumeAdjust
    buttons = [SimpleNamespace(type=car.CarState.ButtonEvent.Type.mainCruise, pressed=pressed) for pressed in value['main_buttons']]
    if toggle.update(buttons, value['enabled']):
      sound.update_alert(car.CarControl.HUDControl.AudibleAlert.prompt)
    else:
      sound.get_audible_alert(sm)
    samples = sound.get_sound_data(step['frames'])
    sound.soundVolumeAdjust = step['adjust']
    rows.append(
      {
        'state': {
          'alert': sound.current_alert,
          'frame': sound.current_sound_frame,
          'volume': sound.current_volume,
          'adjust': sound.soundVolumeAdjust,
          'countdown': sound.carrot_count_down,
          'timeout': sound.selfdrive_timeout_alert,
          'filter': sound.spl_filter_weighted.x,
        },
        'samples': samples.tolist(),
      }
    )
  return rows


def steps():
  rows = []

  def add(alert=0, frames=11, **changes):
    value = {
      'now': len(rows) * 1.1,
      'updated_selfdrive': True,
      'updated_carrot': False,
      'selfdrive_received': 0.0,
      'enabled': False,
      'alert': alert,
      'countdown': -1,
      'pressure': None,
      'main_buttons': [],
    }
    value.update(changes)
    rows.append({'input': value, 'frames': frames, 'adjust': 0.73, 'language': None})

  add(pressure=24.0)
  rows[-1]["adjust"] = -0.73
  add(pressure=30.0)
  for alert in list(range(1, 38)) + [65535]:
    add(alert, frames=2)
    add(alert, frames=17)
    add(0, frames=3)
  for count in [12, 11, 11, 10, 3, 1, 0, -1]:
    add(countdown=count, updated_selfdrive=False, updated_carrot=True)
  add(0, pressure=54.0)
  add(0, pressure=30.0)
  for missing in [5.0, 5.000001, 14.999999, 15.0, 15.000001]:
    add(0, updated_selfdrive=False, enabled=True, selfdrive_received=len(rows) * 1.1 - missing)
  add(0, frames=2, main_buttons=[True])
  add(0, frames=2)
  add(0, frames=2)
  add(0, main_buttons=[False])
  for lang in [' ko_KR ', 'main-zh-Hans', 'en', 'main-ko', 'zh-TW', 'zz']:
    add(1, frames=3)
    rows[-1]['language'] = lang
  return rows


def assets(root, rate, channels):
  base = root / 'openpilot/selfdrive/assets'
  for directory, values in [('sounds_eng', [1200, -2400, 3600, -4800]), ('sounds', [800, -900, 1000, -1100]), ('sounds_chs', [-500, 600, -700, 800])]:
    folder = base / directory
    folder.mkdir(parents=True)
    for name in ['prompt.wav', 'engage.wav', 'engage_tizi.wav', 'disengage_tizi.wav']:
      with wave.open(str(folder / name), 'wb') as wav:
        wav.setparams((channels, 2, rate, 0, 'NONE', 'not compressed'))
        samples = np.array(values if channels == 1 else [[v, v // 3] for v in values], dtype='<i2')
        wav.writeframes(samples.tobytes())
  return base


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  provenance = json.loads(args.binding.with_name('provenance.json').read_text())
  assert hashlib.sha256(args.binding.read_bytes()).hexdigest() == provenance['module_sha256']
  for path, expected in provenance['sources'].items():
    assert hashlib.sha256((ROOT / path).read_bytes()).hexdigest() == expected, path
  comparisons = 0
  for index, (rate, channels, tizi) in enumerate([(48000, 1, False), (48000, 2, True), (12000, 1, False), (44100, 2, True), (0, 1, False), (0, 1, True)]):
    with tempfile.TemporaryDirectory(prefix='sound-source-') as temporary:
      root = Path(temporary)
      config = {
        'assets': str(assets(root, rate, channels) if rate else ROOT / 'openpilot/selfdrive/assets'),
        'engage_volume': 0.37,
        'language': 'en',
        'tizi': tizi,
        'steps': steps(),
      }
      if not rate:
        config['basedir'] = str(ROOT)
      expected = source(config, args.binding, root)
      result = subprocess.run([str(args.binary)], input=json.dumps(config), text=True, capture_output=True, check=True)
      observed = [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
      (args.output / f'{index}-input.json').write_text(json.dumps(config))
      (args.output / f'{index}-source.json').write_text(json.dumps(expected))
      (args.output / f'{index}-native.json').write_text(json.dumps(observed))
      (args.output / f'{index}-stderr.log').write_text(result.stderr or 'no stderr\n')
      assert len(observed) == len(expected)
      for step, (left, right) in enumerate(zip(expected, observed, strict=True)):
        for key in ['alert', 'frame', 'adjust', 'countdown', 'timeout']:
          assert left['state'][key] == right['state'][key], (index, step, key, left['state'], right['state'])
        for key in ['volume', 'filter']:
          assert math.isclose(left['state'][key], right['state'][key], rel_tol=1e-12, abs_tol=1e-12), (index, step, key)
        np.testing.assert_array_equal(np.array(left['samples'], dtype=np.float32).view(np.uint32), np.array(right['samples'], dtype=np.float32).view(np.uint32))
        comparisons += len(left['samples'])
  summary = {
    'passed': True,
    'scenarios': 6,
    'steps_per_scenario': len(steps()),
    'sample_comparisons': comparisons,
    'binding_sources_verified': len(provenance['sources']),
    'audio_initialized': False,
  }
  (args.output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
  print(json.dumps(summary))


if __name__ == '__main__':
  main()
