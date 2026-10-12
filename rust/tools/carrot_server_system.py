#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_system.py [NATIVE_EXAMPLE]
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
from typing import Final, TypedDict, TypeAlias, assert_never

ROOT: Final = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT: Final = ROOT / '.omo/evidence/225-system/standalone'
SOURCE: Final = Path(__file__).with_name('carrot_server_system_source.py')


class Case(TypedDict):
  name: str
  input: dict


Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']


class ResultRow(TypedDict, total=False):
  value: Json
  commands: list[Json]
  values: list[Json]


def setup(root: Path, config: dict) -> None:
  root.mkdir(parents=True)
  (root / 'zones').mkdir()
  (root / 'zones/UTC').write_bytes(b'zone')
  (root / 'zones/Asia').mkdir()
  (root / 'zones/Asia/Seoul').write_bytes(b'zone2')
  match config.get('local', 'matched'):
    case 'matched':
      (root / 'localtime').symlink_to(root / 'zones/UTC')
    case 'dangling':
      (root / 'localtime').symlink_to(root / 'absent')
    case 'empty':
      (root / 'localtime').write_bytes(b'')
    case 'other':
      (root / 'localtime').write_bytes(b'old')
    case 'missing':
      return
    case unexpected:
      assert_never(unexpected)


def normalize(value: ResultRow, root: Path) -> ResultRow:
  return json.loads(json.dumps(value).replace(str(root / 'zones'), '/usr/share/zoneinfo').replace(str(root / 'localtime'), '/data/etc/localtime'))


def invoke(command: list[str], config: dict, root: Path) -> ResultRow:
  completed = subprocess.run(command, input=json.dumps(config | {'root': str(root)}) + '\n', capture_output=True, text=True, timeout=8, check=False)
  receipt = {'argv': command, 'input': config, 'exit': completed.returncode, 'stdout': completed.stdout, 'stderr': completed.stderr}
  (root / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
  assert completed.returncode == 0, completed.stderr
  return normalize(json.loads(completed.stdout.splitlines()[-1]), root)


def cases() -> list[Case]:
  selected: list[Case] = []
  for offset in [-11, -10, 0, 10, 11]:
    selected.append(
      {
        'name': f'threshold-{offset}',
        'input': {'mode': 'time', 'now': 1700000000, 'body': {'epoch_ms': (1700000000 + offset) * 1000, 'timezone': 'UTC'}},
      }
    )
  for local in ['missing', 'dangling', 'empty', 'other']:
    selected.append(
      {
        'name': 'link-' + local,
        'input': {'mode': 'time', 'now': 1700000000, 'local': local, 'body': {'epoch_ms': 1700000000000, 'timezone': 'Asia/Seoul'}},
      }
    )
  for name, patch in [
    ('zone-missing', {'timezone': 'Absent'}),
    ('blank-zone', {'timezone': ' \t '}),
    ('negative-floor', {'epoch_ms': -1}),
    ('bool-epoch', {'epoch_ms': True}),
    ('huge-missing-zone', {'epoch_ms': 10**50, 'timezone': 'Absent'}),
  ]:
    selected.append({'name': name, 'input': {'mode': 'time', 'now': 1700000000, 'body': {'epoch_ms': 1700000000000, 'timezone': 'UTC'} | patch}})
  for fail in [1, 2, 3]:
    selected.append(
      {
        'name': f'exit-{fail}',
        'input': {'mode': 'time', 'now': 1700000000, 'local': 'other', 'fail': fail, 'body': {'epoch_ms': 1700000020000, 'timezone': 'Asia/Seoul'}},
      }
    )
  selected.append(
    {
      'name': 'spawn-error',
      'input': {'mode': 'time', 'now': 1700000000, 'local': 'missing', 'spawn_fail': 1, 'body': {'epoch_ms': 1700000020000, 'timezone': 'UTC'}},
    }
  )
  rows = [
    {'name': name, 'definition': definition}
    for name, definition in [
      ('CruiseSpeed', {'default': 1}),
      ('CarName', {'default': 'x'}),
      ('GitNew', {'default': 0}),
      ('DeviceNew', {'default': 0}),
      ('DeviceSerialNew', {'default': 0}),
      ('', {'default': 0}),
      ('NoDefault', {}),
      ('None', None),
      ('LiveDelay', {'default': 0}),
      ('Custom', {'default': None}),
    ]
  ]
  selected.append({'name': 'default-selection', 'input': {'mode': 'select', 'steps': rows}})
  wifi = (
    b'yes:A\\:B:WPA2:30\r\nno:other:--:0\nno:A\\:B:WPA3:90\nyes:connected:--:bad\n'
    + b'no:other:WPA2:20\nno:unicode\xc2\xa0:--: -2\nno:broken:--:x\ninvalid\nno: :--:50\n'
  )
  selected.append(
    {
      'name': 'network-cache',
      'input': {
        'mode': 'network',
        'wifi': list(wifi),
        'ip': 'ignored\nIP4.ADDRESS[1]:192.0.2.5/24\n',
        'steps': [{'operation': 'snapshot'}, {'operation': 'refresh'}, {'operation': 'snapshot', 'values': {'GsmApn': 'fresh', 'HotspotOnBoot': True}}],
      },
    }
  )
  selected.append({'name': 'network-error', 'input': {'mode': 'network', 'code': 1, 'steps': [{'operation': 'refresh'}, {'operation': 'snapshot'}]}})
  return selected


def main() -> None:
  binary = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else None
  output = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else DEFAULT_OUTPUT
  output.mkdir(exist_ok=False)
  results = []
  for case in cases():
    destination = output / case['name']
    pairs = []
    for kind in ['source', 'native'] if binary else ['source']:
      root = destination / kind
      config = case['input']
      setup(root, config)
      if config['mode'] == 'network':
        (root / 'wifi').write_bytes(bytes(config.get('wifi', [])))
        (root / 'ip').write_text(config.get('ip', ''))
        script = '#!/bin/sh\nprintf "%s\\n" "$*" >> "$(dirname "$0")/argv"\n'
        script += (
          f'exit {config["code"]}\n' if config.get('code') else 'case "$3" in ACTIVE*) cat "$(dirname "$0")/wifi";; *) cat "$(dirname "$0")/ip";; esac\n'
        )
        (root / 'nmcli').write_text(script)
        (root / 'nmcli').chmod(0o700)
      command = [sys.executable, '-P', str(SOURCE)] if kind == 'source' else [str(binary)]
      pairs.append(invoke(command, config, root))
    equal = len(pairs) == 1 or pairs[0] == pairs[1]
    results.append({'name': case['name'], 'equal': equal, 'observations': pairs})
    (output / 'result.json').write_text(json.dumps({'cases': len(results), 'paired': binary is not None, 'results': results}, indent=2) + '\n')
    assert equal, (case['name'], pairs)
  print(json.dumps({'cases': len(results), 'paired': binary is not None, 'equal': True}))


if __name__ == '__main__':
  main()
