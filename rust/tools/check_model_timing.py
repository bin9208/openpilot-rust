#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["pytest==9.0.2"]
# ///
# Run: uv run rust/tools/check_model_timing.py --binary rust/target/debug/examples/model_timing_trace --output /tmp/model-timing
"""Execute the original modeld record expression against its actual RuntimeDiagnostics."""
import argparse
import ast
import importlib.util
import json
import math
import random
import struct
import subprocess
from pathlib import Path
from types import SimpleNamespace
import pytest


def exact(left, right) -> None:
  assert type(left) is type(right), (left, right)
  match left:
    case dict():
      assert list(left) == list(right), (left, right)
      for key in left:
        exact(left[key], right[key])
    case list():
      assert len(left) == len(right)
      for first, second in zip(left, right, strict=True):
        exact(first, second)
    case float():
      assert math.isnan(left) and math.isnan(right) or struct.pack('!d', left) == struct.pack('!d', right), (left, right)
    case _:
      assert left == right, (left, right)


def check(binary: Path, output: Path) -> None:
  root = Path(__file__).resolve().parents[2]
  output.mkdir(parents=True)
  spec = importlib.util.spec_from_file_location('source_diagnostics', root / 'openpilot/common/runtime_diagnostics.py')
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  source = root / 'openpilot/selfdrive/modeld/modeld.py'
  tree = ast.parse(source.read_text())
  calls = [node for node in ast.walk(tree) if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
           and isinstance(node.func.value, ast.Name) and node.func.value.id == 'diagnostics' and node.func.attr == 'record']
  assert len(calls) == 1
  code = compile(ast.fix_missing_locations(ast.Module(body=[ast.Expr(value=calls[0])], type_ignores=[])), str(source), 'exec')
  events = []
  samples = []
  expected = []
  rng = random.Random(53)
  clock = [10.0]
  scheduler = [[100, 200, 3]]
  with pytest.MonkeyPatch.context() as patched:
    patched.setattr(module.time, 'monotonic', lambda: clock[0])
    patched.setattr(module.os, 'getpid', lambda: 123)
    patched.setattr(module.RuntimeDiagnostics, '_schedstat', lambda _self: tuple(scheduler[0]) if scheduler[0] is not None else ())
    patched.setattr(module.Path, 'read_text', lambda _self: '0')
    diagnostics = module.RuntimeDiagnostics('modeld', lambda _name, **fields: events.append(fields))
    original_record = diagnostics.record

    def record(context=None, **values):
      before = len(events)
      original_record(context, **values)
      event = events[-1] if len(events) > before else None
      if event is not None:
        assert event['backend'] == 'ModelState'
        event['backend'] = "openpilot_driving_modeld::runtime::DrivingRuntime<'_>"
      expected.append({'values': values, 'event': event})

    diagnostics.record = record
    for index in range(2053):
      start = clock[0]
      cpu = index * .01
      wait = rng.choice([0.0, .05, .1, 1.1])
      infer = rng.uniform(.001, .04)
      post = rng.uniform(0, .005)
      boundary = [10.999999999, 11.0, 11.000000001, 12.0]
      if index < len(boundary):
        span = boundary[index] - start
        wait, infer, post = span * .4, span * .4, span * .19
      now = start + wait + infer + post
      sample = {'frame_id': index, 'loop_start': start, 'cpu_start': cpu, 'camera_ready': start + wait,
                'camera_age_at_run_ms': rng.uniform(-1, 80), 'inference_seconds': infer, 'inference_cpu_ms': infer * 500,
                'inference_finished': start + wait + infer, 'postprocess_end': now, 'loop_end': now + .000001,
                'cpu_end': cpu + infer * .5 + post, 'dropped': rng.choice([0, 0, 1, 8, 2**32 - 1]),
                'published': index % 3 != 0, 'now': now + .00001,
                'scheduler': None if index % 11 == 0 else [index * 1000, index * 10, index]}
      if index < len(boundary):
        sample['loop_end'] = now + span * .001
        sample['now'] = boundary[index]
      samples.append(sample)
      clock[0], scheduler[0] = sample['now'], sample['scheduler']
      source_clock = SimpleNamespace(perf_counter=lambda value=sample['postprocess_end']: value, monotonic=lambda value=sample['loop_end']: value,
                                     thread_time=lambda value=sample['cpu_end']: value)
      scope = {'diagnostics': diagnostics, 'time': source_clock, 'loop_start': start, 'cpu_start': cpu,
               'camera_ready': sample['camera_ready'], 'camera_age_at_run_ms': sample['camera_age_at_run_ms'],
               'model_execution_time': infer, 'inference_cpu_ms': sample['inference_cpu_ms'], 'mt2': sample['inference_finished'],
               'vipc_dropped_frames': sample['dropped'], 'model_output': {} if sample['published'] else None,
               'model': type('ModelState', (), {'usbgpu': False})(), 'meta_main': SimpleNamespace(frame_id=index)}
      exec(code, scope)
  data = ''.join(json.dumps(sample) + '\n' for sample in samples)
  (output / 'inputs.jsonl').write_text(data)
  result = subprocess.run([str(binary)], input=data, capture_output=True, text=True, check=True)
  (output / 'rust.jsonl').write_text(result.stdout)
  (output / 'source.jsonl').write_text(''.join(json.dumps(value) + '\n' for value in expected))
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  assert len(actual) == len(expected)
  for reference, observed in zip(expected, actual, strict=True):
    exact(reference, observed)
  assert [value['event'] is not None for value in actual[:4]] == [False, True, False, True]
  report = {'result': 'pass', 'steps': len(actual), 'aggregates': len(events), 'comparison': 'ordered keys, exact types and float64 bits',
            'backend_mapping': {'ModelState': "openpilot_driving_modeld::runtime::DrivingRuntime<'_>"},
            'source_record_line': calls[0].lineno, 'interval_seconds': 1.0, 'cpu_clock': 'runtime uses CLOCK_THREAD_CPUTIME_ID'}
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  check(args.binary.resolve(), args.output.resolve())
