#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp", "pyzmq"]
# ///
# ─── How to run ───
# PYTHONPATH=.:<original-msgq> python rust/tools/check_selfdrived_controller.py --binary <controller> --binding <params_pyx.so> --output <evidence>
# ──────────────────
"""Compare source SelfdriveD methods, mutable state, Params effects and actual wire messages."""
from __future__ import annotations

import argparse
import ast
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import capnp
from openpilot.cereal import car, log
from original_params_binding import load as load_binding
from selfdrived_source import Fixture, SOURCE
from selfdrived_snapshot import source_snapshot, source_health, native_snapshot, normalize, compress_buffers


def execute(fixture, row, params_root, prefix):
  fixture.effects.clear()
  fixture.messages.clear()
  error = None
  try:
    match row['operation']:
      case 'parameter':
        path = params_root / prefix / row['key']
        if path.is_dir():
          path.rmdir()
        elif path.exists():
          path.unlink()
        if row['directory']:
          path.mkdir()
        elif row['bytes'] is not None:
          path.write_bytes(bytes(row['bytes']))
      case 'init':
        if row['mode']['simulation']:
          os.environ['SIMULATION'] = '1' if row.get('health_simulation', False) else '0'
        else:
          os.environ.pop('SIMULATION', None)
        with car.CarParams.from_bytes(bytes(row['cp'])) as cp:
          fixture.initialize(cp.as_builder(), row['mode'], row['language'])
      case 'streams':
        fixture.streams = row['values'].copy()
      case 'step' | 'events' | 'sample' | 'advance':
        fixture.now = row['now']
        fixture.current = None
        if row['current'] is not None:
          with car.CarState.from_bytes(bytes(row['current'])) as current:
            fixture.current = current.as_builder()
        fixture.pending = []
        for raw in row['messages']:
          with log.Event.from_bytes(bytes(raw)) as message:
            fixture.pending.append(message.as_builder())
        fixture.machine.rk.avg_dt.sum = fixture.machine.rk.avg_dt.count * (.02 if row.get('lagging', False) else .01)
        operation = row['operation']
        if operation == 'advance':
          for index in range(row['count']):
            fixture.now = row['now'] + index * row['dt']
            fixture.machine.step()
        elif operation == 'step':
          fixture.machine.step()
        else:
          cs = fixture.machine.data_sample() if operation == 'sample' else fixture.current
          if operation == 'events':
            fixture.machine.sm.update(0)
            fixture.machine.update_events(cs)
      case 'alerts':
        with car.CarState.from_bytes(bytes(row['current'])) as cs:
          fixture.machine.update_alerts(cs)
      case 'publish':
        fixture.machine.publish_selfdriveState(fixture.machine.CS_prev)
      case 'params_cycle':
        fixture.params_stop = False
        fixture.machine.params_thread(fixture.stop_event())
      case _:
        raise AssertionError(row['operation'])
  except (KeyError, TypeError, ValueError, IndexError, ZeroDivisionError, RuntimeError, capnp.KjException) as exc:
    error = type(exc).__name__
  return compress_buffers(normalize({'state': source_snapshot(fixture), 'health': source_health(fixture), 'effects': fixture.effects.copy(),
                    'messages': fixture.messages.copy(), 'error': error}))


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  prefix = 'selfdrive' + str(os.getpid())
  os.environ['OPENPILOT_PREFIX'] = prefix
  from selfdrived_cases import cases
  rows = cases()
  with tempfile.TemporaryDirectory(prefix='selfdrive-controller-') as temporary:
    temporary = Path(temporary)
    module, _logger = load_binding(args.binding.resolve(), 'ipc://' + str(temporary / 'source-log'), temporary / 'logs')
    fixture = Fixture(module.Params(str(temporary / 'source')))
    expected = []
    executed = set()
    def trace(frame, action, _argument):
      if action == 'line' and frame.f_code.co_filename == str(SOURCE):
        executed.add(frame.f_lineno)
      return trace
    sys.settrace(trace)
    with contextlib.redirect_stdout(io.StringIO()) as source_console:
      for row in rows:
        expected.append(execute(fixture, row, temporary / 'source', prefix))
    sys.settrace(None)
    (args.output / 'source-console.log').write_text(source_console.getvalue() or 'no source console output\n')
    payload = ''.join(json.dumps(row) + '\n' for row in rows)
    (args.output / 'input.jsonl').write_text(payload)
    (args.output / 'source.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in expected))
    result = subprocess.run([str(args.binary.resolve()), str(temporary / 'native'), prefix, 'ipc://' + str(temporary / 'native-log')],
                            input=payload, capture_output=True, text=True, timeout=120)
    (args.output / 'native.jsonl').write_text(result.stdout)
    (args.output / 'native.stderr').write_text(result.stderr or 'no native stderr\n')
    assert result.returncode == 0, result.stderr
    actual = [normalize(native_snapshot(json.loads(line))) for line in result.stdout.splitlines()]
    assert len(actual) == len(expected), (len(actual), len(expected))
    for index, (source, native) in enumerate(zip(expected, actual, strict=True)):
      if source['error'] == 'KjException':
        assert native['error'] == 'MissingAlertText(MissingAlertText)', (index, native['error'])
        native['error'] = 'KjException'
      if source['error'] == 'KeyError':
        assert native['error'] == 'Personality("10")', (index, native['error'])
        native['error'] = 'KeyError'
      for side in (source, native):
        for message in side['messages']:
          with log.Event.from_bytes(bytes(message['bytes'])) as reader:
            message['bytes'] = reader.to_dict()
        for effect in side['effects']:
          if effect['operation'] == 'event':
            effect['fields'] = json.loads(effect['fields'])
            if effect['name'] == 'process_not_running':
              text = effect['fields']['not_running']
              assert isinstance(text, str), ('source set repr must remain text', text)
              names = ast.literal_eval(text)
              assert isinstance(names, set) and all(isinstance(name, str) for name in names)
              # Python set iteration is unspecified; retain exact members and the wire text type.
              effect['fields']['not_running'] = sorted(names)
      if native != source:
        (args.output / 'difference.json').write_text(json.dumps({'index': index, 'request': rows[index], 'native': native, 'source': source}, indent=2)+'\n')
        raise AssertionError(('controller mismatch', index, args.output / 'difference.json'))
  tree = ast.parse(SOURCE.read_text())
  cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'SelfdriveD')
  statements = {node.lineno for method in cls.body if isinstance(method, ast.FunctionDef) and method.name != 'run'
                for node in ast.walk(method) if isinstance(node, ast.stmt) and not isinstance(node, ast.FunctionDef)}
  coverage = {'executed_lines': sorted(executed), 'missing_statement_lines': sorted(statements-executed)}
  (args.output / 'source-coverage.json').write_text(json.dumps(coverage, indent=2)+'\n')
  report = {'result': 'PASS', 'requests': len(rows), 'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
            'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'input_sha256': hashlib.sha256(payload.encode()).hexdigest(),
            'controller_steps': sum(row.get('count', 1) for row in rows if row['operation'] in ('step', 'advance')),
            'source_exceptions': [{'index': index, 'type': row['error']} for index, row in enumerate(expected) if row['error'] is not None],
            'observed_events': sorted({event for row in expected if row['state'] for event in row['state']['events']})}
  (args.output / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
