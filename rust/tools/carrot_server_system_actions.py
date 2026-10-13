#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_system_actions.py NATIVE_EXAMPLE NEW_OUTPUT
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import shlex
from typing import Final, TypedDict

ROOT: Final = Path(__file__).resolve().parents[2]
BINDING: Final = (
  ROOT / '.analysis/scratch/2026-09-30-rust-startup-integration/combined-inputs-3/startup-params-binding' / 'params_pyx.cpython-312-x86_64-linux-gnu.so'
)
SOURCE: Final = Path(__file__).with_name('carrot_server_system_actions_source.py')


class Case(TypedDict):
  name: str
  input: dict


def cases() -> list[Case]:
  result: list[Case] = []
  for action in ['reboot', 'poweroff', 'recalibrate']:
    for available, engaged in [(True, False), (True, True), (False, False), (False, True)]:
      result.append(
        {
          'name': f'{action}-{available}-{engaged}',
          'input': {
            'mode': 'action',
            'action': action,
            'params': available,
            'engaged': engaged,
            'initial': {'CalibrationParams': '616263', 'LiveTorqueParameters': '7b7d', 'LiveParameters': '7b7d', 'LiveDelay': '32'},
          },
        }
      )
  result.append({'name': 'fallback-spawn-error', 'input': {'mode': 'action', 'action': 'reboot', 'params': False, 'spawn_fail': 1}})
  for status in ['uncalibrated', 'calibrated', 'invalid']:
    result.append(
      {
        'name': 'calibration-' + status,
        'input': {
          'mode': 'calibration',
          'calibration': {'status': status, 'rpy': [0.0, 0.12345, -0.23456]},
        },
      }
    )
  result.append({'name': 'calibration-short', 'input': {'mode': 'calibration', 'calibration': {'status': 'calibrated', 'rpy': [0.0, 0.1]}}})
  result.append({'name': 'calibration-corrupt', 'input': {'mode': 'calibration', 'initial': {'CalibrationParams': 'ff'}}})
  result.append({'name': 'calibration-empty', 'input': {'mode': 'calibration'}})
  result.append({'name': 'calibration-unavailable', 'input': {'mode': 'calibration', 'params': False}})
  data = {
    'params': [
      {'name': 'IsMetric', 'default': 1},
      {'name': 'CarName', 'default': 'excluded'},
      {'name': 'CruiseGapLevels', 'min': 2, 'max': 4, 'default': 4, 'options': {'names': ['2', '3', '4']}},
      {'name': 'FutureSetting', 'min': 0, 'max': 60, 'default': 20},
      {'name': 'OtherSetting', 'min': 0, 'max': 60, 'default': 'bad'},
    ]
  }
  result.append(
    {'name': 'defaults-apply', 'input': {'mode': 'defaults', 'data': data, 'initial': {'IsMetric': '30', 'CarName': '6f6c64', 'CruiseGapLevels': '32'}}}
  )
  result.append({'name': 'defaults-unavailable', 'input': {'mode': 'defaults', 'params': False, 'data': data}})
  for namespace in ['removed', 'blocked']:
    result.append({'name': 'constructor-' + namespace, 'input': {'mode': 'action', 'action': 'reboot', 'namespace': namespace}})
  return result


def main() -> None:
  binary = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else None
  output = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else ROOT / '.omo/evidence/225-system/actions-source'
  environment = json.loads((ROOT / '.omo/evidence/carrot-server-225-resume/live-runtime/application-ruff-v4-invocation.json').read_text())
  env = os.environ | {'PYTHONPATH': environment['PYTHONPATH'], 'OPENPILOT_PREFIX': 'd'}
  output.mkdir(exist_ok=False)
  results = []
  for case in cases():
    if len(sys.argv) > 3 and sys.argv[3] == 'constructor' and not case['name'].startswith('constructor-'):
      continue
    if len(sys.argv) > 3 and sys.argv[3] == 'remaining' and case['input']['mode'] == 'action':
      continue
    pairs = []
    for kind in ['source', 'native'] if binary else ['source']:
      root = output / case['name'] / kind
      root.mkdir(parents=True)
      config = case['input'] | {'root': str(root), 'binding': str(BINDING)}
      if kind == 'native' and config['mode'] == 'calibration' and case['input'].get('calibration'):
        config['initial'] = config.get('initial', {}) | {'CalibrationParams': pairs[0]['stored']['CalibrationParams']}
      (root / 'bin').mkdir()
      if not config.get('spawn_fail'):
        helper = root / 'bin/sudo'
        helper.write_text('#!/bin/sh\nprintf \'{"pid":%s}\\n\' "$$" > ' + shlex.quote(str(root / 'spawn.json')) + '\n')
        helper.chmod(0o700)
      case_env = env | {'PATH': str(root / 'bin')}
      command = [environment['argv'][0], '-P', str(SOURCE)] if kind == 'source' else [str(binary)]
      process = subprocess.run(command, env=case_env, input=json.dumps(config) + '\n', text=True, capture_output=True, timeout=8, check=False)
      receipt = {'argv': command, 'input': config, 'exit': process.returncode, 'stdout': process.stdout, 'stderr': process.stderr}
      (root / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
      assert process.returncode == 0, process.stderr
      observed = json.loads(process.stdout.splitlines()[-1])
      pairs.append(json.loads(json.dumps(observed).replace(str(root), '<owned>')))
    equal = len(pairs) == 1 or pairs[0] == pairs[1]
    results.append({'name': case['name'], 'equal': equal, 'observations': pairs})
    (output / 'result.json').write_text(json.dumps({'cases': len(results), 'paired': binary is not None, 'results': results}, indent=2) + '\n')
    assert equal, (case['name'], pairs)
  print(json.dumps({'cases': len(results), 'paired': binary is not None, 'equal': True}))


if __name__ == '__main__':
  main()
