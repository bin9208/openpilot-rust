#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# uv run rust/tools/carrot_server_auto_update_policy.py --output OUTPUT [--binary BINARY]
# ──────────────────
"""Original repeated-input updater conditions and pure response policies."""
from __future__ import annotations

import argparse
import ast
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'openpilot/selfdrive/carrot/server/services/auto_update.py'


def original():
  tree = ast.parse(SOURCE.read_text())
  names = {'_is_park', 'ManagerReady', 'AutoRebootCondition', '_verified_update_target', '_short_error'}
  tree.body = [node for node in tree.body if (isinstance(node, (ast.FunctionDef, ast.ClassDef)) and node.name in names)
               or (isinstance(node, ast.Assign) and all(isinstance(target, ast.Name) and target.id.isupper() for target in node.targets))]
  scope = {}
  exec(compile(tree, str(SOURCE), 'exec'), scope)
  return scope


def source(case, scope):
  match case['kind']:
    case 'manager':
      condition = scope['ManagerReady'](case.get('delay', 10.0))
      return [condition.update(step['now'], step['valid']) for step in case['steps']]
    case 'reboot':
      condition = scope['AutoRebootCondition'](case['mode'], case.get('delay', 1.0))
      return [condition.update(**step) for step in case['steps']]
    case 'target':
      result = []
      for status in case['steps']:
        try:
          result.append({'result': scope['_verified_update_target'](status)})
        except (ValueError, TypeError, OverflowError) as error:
          result.append({'exception': type(error).__name__, 'message': str(error)})
      return result
    case 'short_error':
      return [scope['_short_error'](step['output'], step['fallback']) for step in case['steps']]
    case unknown:
      raise AssertionError(unknown)


def cases():
  def manager(times):
    return [{'now': now, 'valid': valid} for now, valid in times]
  yield {'name': 'manager-long-build-restart', 'kind': 'manager', 'steps': manager(
    [(tick, False) for tick in range(600)] + [(tick, True) for tick in range(600, 611)] + [(611, False), (612, True), (630, True)])}
  yield {'name': 'manager-gap-boundary', 'kind': 'manager', 'steps': manager(
    [(0, True), (2.5, True), (5, True), (7.5, True), (10, True), (12.500000001, True), (15, True)])}
  for delay in [0, -1, 0.25, float('inf'), float('nan')]:
    yield {'name': f'manager-delay-{delay}', 'kind': 'manager', 'delay': delay,
           'steps': manager([(0, True), (0.25, True), (1, False), (1.25, True)])}
  def step(now, **kw):
    return {'now': now, 'selfdrive_valid': True, 'engaged': False, **kw}
  yield {'name': 'park-validity-and-gears', 'kind': 'reboot', 'mode': 'park', 'steps': [
    step(0, selfdrive_valid=False, car_state_valid=True, gear_shifter='park'), step(.1, engaged=True, car_state_valid=True, gear_shifter='park'),
    step(.2, car_state_valid=True, gear_shifter='drive'), step(.3, car_state_valid=True, gear_shifter='park'),
    *[step(index + 1, car_state_valid=True, gear_shifter=gear) for index, gear in enumerate(
      [None, ' PARK ', 'GearShifter.park', '\x1cpar\u212a\x1f', '\ud800.park', 4, True, ['park']])]]}
  yield {'name': 'disengaged-continuity', 'kind': 'reboot', 'mode': 'disengaged', 'steps': [
    step(1), step(1.8, engaged=True), step(2), step(2.8, selfdrive_valid=False), step(3), step(3.9), step(4)]}
  yield {'name': 'offroad-fallback', 'kind': 'reboot', 'mode': 'disengaged', 'steps': [
    step(20, selfdrive_valid=False, device_state_valid=True, device_started=False),
    step(20.9, selfdrive_valid=False, device_state_valid=True, device_started=False),
    step(21, selfdrive_valid=False, device_state_valid=True, device_started=False),
    step(21.1, selfdrive_valid=False, device_state_valid=False, device_started=False),
    step(22, selfdrive_valid=False, device_state_valid=True, device_started=True)]}
  for mode in ['off', 'invalid', 'Park']:
    yield {'name': f'reboot-mode-{mode}', 'kind': 'reboot', 'mode': mode, 'steps': [step(0, car_state_valid=True, gear_shifter='park'), step(100)]}
  for delay in [0, -1, .25, float('inf'), float('nan')]:
    yield {'name': f'reboot-delay-{delay}', 'kind': 'reboot', 'mode': 'disengaged', 'delay': delay, 'steps': [step(0), step(.25), step(1)]}
  statuses = [{'available': False, 'state': 'fetch_error', 'behind': 3, 'target_head': 'stale'},
              {'available': True, 'state': 'busy', 'behind': 3, 'target_head': 'stale'}, {}]
  for behind in [0, -3, 3, True, 2.9, None, '', ' 4 ', '٣', 10**80, 'bad', [], [1], {}, {'a': 1}, float('nan'), float('inf')]:
    for head in [' verified\n', '', None, 0, True, '\ud800\x1c', [1]]:
      statuses.append({'available': True, 'state': 'ok', 'behind': behind, 'target_head': head})
  yield {'name': 'verified-targets-and-stale-counts', 'kind': 'target', 'steps': statuses}
  yield {'name': 'short-error-text', 'kind': 'short_error', 'steps': [
    {'output': value, 'fallback': 'fallback'} for value in [None, '', ' \t\x1c ', ' one\r\n two\u0085three\u2028four ',
      False, 0, 123, ['a', None], {'z': 1, 'a': True}, '\ud800' + '한😀' * 1100, 'x' * 2001]] +
    [{'output': '', 'fallback': ' 한😀' * 1100}]}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--output', required=True, type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  scope = original()
  rows = []
  for case in cases():
    expected = json.loads(json.dumps(source(case, scope), ensure_ascii=True))
    if case['name'] == 'manager-gap-boundary':
      assert expected == [False, False, False, False, True, False, False]
    if case['name'] == 'disengaged-continuity':
      assert expected == [False, False, False, False, False, False, True]
    command = [str(args.binary.resolve())] if args.binary else None
    process = subprocess.run(command, input=json.dumps(case, ensure_ascii=True), capture_output=True, text=True, timeout=5) if command else None
    try:
      actual = json.loads(process.stdout) if process else None
    except json.JSONDecodeError as error:
      actual = {'invalid_native_json': str(error)}
    row = {'case': case, 'source': expected, 'native': actual, 'command': command,
           'exit_code': process.returncode if process else None, 'stdout': process.stdout if process else None,
           'stderr': process.stderr if process else None,
           'equal': actual == expected if process else None}
    rows.append(row)
    (args.output / 'result.json').write_text(json.dumps(rows, ensure_ascii=True, indent=2) + '\n')
    print(case['name'], 'PASS' if not process or (process.returncode == 0 and row['equal']) else 'FAIL', flush=True)
  identity = {'source': str(SOURCE), 'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
              'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest() if args.binary else None}
  (args.output / 'identity.json').write_text(json.dumps(identity, indent=2) + '\n')
  raise SystemExit(0 if not args.binary or all(row['equal'] and row['exit_code'] == 0 for row in rows) else 1)


if __name__ == '__main__':
  main()
