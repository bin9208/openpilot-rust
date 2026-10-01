# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = []
# ///
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from typing import TypeAlias

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from openpilot.selfdrive.carrot.bluetooth.model import ACTIONS, validate_config

Json: TypeAlias = bool | int | float | str | None | list['Json'] | dict[str, 'Json']
MAC = '66:C0:0C:7B:6E:71'


def device_case(device: Json) -> Json:
  return {'devices': {MAC: device}}


def cases() -> list[Json]:
  result: list[Json] = [None, False, 0, '', [], {}, {'devices': None}, {'devices': []}, {'devices': {}},
                        {'version': -55, 'ignored': {'invalid': True}}, device_case(None), device_case(False),
                        device_case({}), device_case({'profile': 'generic'}), device_case({'profile': 'yiser-j6'})]
  for profile in ('generic', 'yiser-j6', 'unknown', None, 1, [], {}):
    result.append(device_case({'profile': profile}))
  for enabled in (True, False, 1, 0, None, '', [], {}, 'true'):
    result.append(device_case({'profile': 'generic', 'enabled': enabled}))
  for name in (None, True, False, 100, -0.0, float('inf'), float('-inf'), float('nan'), 10**200,
               1e-7, 1e20, '한😀' * 100, '\ud800x\udfff', ['a', None, True, 1e-7], {'z': "'한", 'a': False}):
    result.append(device_case({'profile': 'generic', 'name': name}))
  for address in ('66:c0:0c:7b:6e:71', 'ﬀ:aa:bb:cc:dd:ee', 'AA:BB:CC:DD:EE:FF', 'A:B:C:D:E:F',
                  '66:C0:0C:7B:6E:71\n', ' 66:C0:0C:7B:6E:71', '\ud800', '01:02:03:04:05:0G'):
    result.append({'devices': {address: {'profile': 'generic'}}})
  result.append({'devices': {MAC.lower(): {'profile': 'generic', 'name': 'first'},
                            '00:00:00:00:00:00': {'profile': 'generic'},
                            MAC: {'profile': 'yiser-j6', 'name': 'last'}}})
  for count in (16, 17):
    result.append({'devices': {f'00:00:00:00:00:{index:02x}': {'profile': 'generic'} for index in range(count)}})
  for mapping in (None, [], 'up', True, {}, {'up': 'accelCruise'}):
    result.append(device_case({'profile': 'generic', 'mapping': mapping}))
  tokens = ('up', 'down', 'left', 'right', 'center', '1', '2', 'key:0', 'key:0001', 'key:767', 'key:768',
            'key:9999', 'key:00000', 'key:-1', 'key:', 'key:１', 'swipe:x+', 'swipe:x-', 'swipe:y+', 'swipe:y-',
            'swipe:z+', 'tap:0:0', 'tap:99999:00000', 'tap:100000:0', 'tap:-1:0', 'tap:1:2:3', 'up\n', '\ud800')
  for token in tokens:
    for suffix in ('', '@double', '@long', '@other', '@long@double'):
      result.append(device_case({'profile': 'generic', 'mapping': {token + suffix: 'accelCruise'}}))
  for action in (*ACTIONS, None, False, 1, [], {}, 'shell', 'AccelCruise'):
    for token in ('up', 'key:999'):
      result.append(device_case({'profile': 'generic', 'mapping': {token: action}}))
  for count in (63, 64, 65):
    result.append(device_case({'profile': 'generic', 'mapping': {f'key:{key}': 'none' for key in range(count)}}))
  mapping = {f'key:{key}{suffix}': 'none' for key in range(64) for suffix in ('', '@long', '@double')}
  result.append(device_case({'profile': 'generic', 'mapping': mapping}))
  result.append(device_case({'profile': 'generic', 'mapping': {**mapping, 'up': 'none'}}))
  result.append(device_case({'profile': 'generic', 'mapping': {**{f'key:{key}': 'none' for key in range(65)}, '\ud800': 'none'}}))
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  inputs = cases()
  payload = ''.join(json.dumps(value) + '\n' for value in inputs)
  (args.output / 'input.jsonl').write_text(payload)
  expected = []
  for value in inputs:
    try:
      expected.append(validate_config(value))
    except ValueError as error:
      expected.append({'error': str(error)})
  (args.output / 'original.jsonl').write_text(''.join(json.dumps(value) + '\n' for value in expected))
  result = subprocess.run([str(args.binary.resolve())], input=payload, text=True, capture_output=True, timeout=30)
  (args.output / 'native.jsonl').write_text(result.stdout)
  (args.output / 'native.stderr').write_text(result.stderr)
  result.check_returncode()
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  assert len(actual) == len(expected)
  for index, (source, native) in enumerate(zip(expected, actual, strict=True)):
    assert source == native, (index, inputs[index], source, native)
    if 'devices' in source:
      assert list(source['devices']) == list(native['devices']), index
      for address in source['devices']:
        assert list(source['devices'][address]['mapping']) == list(native['devices'][address]['mapping']), (index, address)
  summary = {'cases': len(inputs), 'accepted': sum('error' not in value for value in expected),
             'source_sha256': hashlib.sha256((ROOT / 'openpilot/selfdrive/carrot/bluetooth/model.py').read_bytes()).hexdigest(),
             'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'result.json').write_text(json.dumps(summary, indent=2))
  print(json.dumps(summary))


if __name__ == '__main__':
  main()
