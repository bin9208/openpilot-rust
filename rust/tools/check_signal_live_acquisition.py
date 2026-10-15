"""Original readers/geometry context replay actual native receipt/file captures.

Only queue update/clock inputs are replayed; reader/read-cache and final source
prepare context execute unchanged. Full owner/wire replay is a separate gate.
"""
import argparse
import ast
import dataclasses
import json
import itertools
import math
from pathlib import Path
import sys
import tempfile
from types import ModuleType, SimpleNamespace as NS
import numpy as np
from openpilot.cereal import log
from check_signal_geometry import load as load_geometry
from signal_source import classes, source, compare


HOOK = None


def namespace(value):
  if isinstance(value, dict):
    return NS(**{k: namespace(v) for k, v in value.items()})
  if isinstance(value, list):
    return [namespace(v) for v in value]
  return value


class SM(dict):
  def attach(self, metadata):
    for field, key in [('seen', 'seen'), ('valid', 'valid'), ('alive', 'alive'), ('recv_time', 'receive_time')]:
      setattr(self, field, {row['name']: row[key] for row in metadata})
    self.update = lambda _timeout: None


def reader_state(runtime):
  return dict(enabled=runtime.enabled, last_mode_check=runtime.last_mode_check,
              enabled_since=runtime.enabled_since if math.isfinite(runtime.enabled_since) else None,
              bytes=list(runtime._observation_bytes) if runtime._observation_bytes is not None else None,
              observation=runtime._observation)


def function_nodes(path, names):
  return ast.Module(body=[n for n in ast.parse(source(path)).body if isinstance(n, (ast.FunctionDef, ast.ClassDef)) and n.name in names], type_ignores=[])


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--capture', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  rows = json.loads((args.capture / 'trace.json').read_text())
  geometry = load_geometry()
  camera = ModuleType('openpilot.common.transformations.camera')
  sys.modules[camera.__name__] = camera
  code = source('openpilot/common/transformations/camera.py').split('# device/mesh')[0].replace('import openpilot.common.transformations.orientation as orient', '')
  exec(compile(code, 'e8e152db:camera-config', 'exec'), camera.__dict__)
  mode = ModuleType('openpilot.selfdrive.carrot.signal_mode')
  current_mode = [2]
  mode.signal_mode = lambda: current_mode[0]
  flag_scope = classes('openpilot/selfdrive/carrot/signal_mode.py', ['flag_override'])
  flag_scope.update(Path=Path, DIRECTORY=Path('/data/signal-color-shadow'), signal_mode=mode.signal_mode)
  mode.flag_override = flag_scope['flag_override']
  sys.modules[mode.__name__] = mode
  times = iter(())
  scope = dict(Path=Path, json=json, np=np, math=math, asdict=dataclasses.asdict,
               time=NS(monotonic=lambda: next(times)), MAX_BYTES=32768,
               ENABLE_PATH=Path('/data/signal-color-shadow/start_guard_enabled'),
               OBSERVATION_PATH=Path('/dev/shm/carrot_signal_start_observation.json'))
  scope.update(classes('openpilot/selfdrive/carrot/signal_assist.py', ['SignalDecision', 'SignalAssist']))
  exec(compile(function_nodes('openpilot/selfdrive/carrot/signal_assist_runtime.py', ['requested', 'SignalAssistRuntime']), 'e8e152db:SignalAssistRuntime', 'exec'), scope)
  scope.update(classes('openpilot/selfdrive/carrot/signal_start_guard.py', ['SignalStartGuard', 'StartDecision', 'observation_color']))
  exec(compile(function_nodes('openpilot/selfdrive/carrot/signal_start_guard.py', ['SignalStartRuntime']), 'e8e152db:SignalStartRuntime', 'exec'), scope)
  prepare_classes = classes('openpilot/selfdrive/carrot/signal_stop_prepare.py', ['PrepareDecision', 'SignalStopPrepare', 'central_red_priority', 'PrepareAccel'])
  prepare_classes['combinations'] = itertools.combinations
  scope.update(prepare_classes)
  scope.update({name: geometry[name] for name in ['housing_range', 'housing_alignment', 'night_lamp_support']})
  scope.update(ENABLE_PATH=Path('/data/signal-color-shadow/stop_prepare_enabled'), OBSERVATION_PATH=Path('/dev/shm/carrot_signal_prepare_observation.json'))
  exec(compile(function_nodes('openpilot/selfdrive/carrot/signal_stop_prepare_runtime.py', ['SignalPrepareRuntime']), 'e8e152db:SignalPrepareRuntime', 'exec'), scope)
  # classes() adds only math/dataclass helpers; keep the captured clock explicit.
  scope['time'] = NS(monotonic=lambda: next(times))
  expected, actual = [], []
  with tempfile.TemporaryDirectory(prefix='signal-source-live-reader-') as temporary:
    root = Path(temporary)
    start = scope['SignalStartRuntime'](Path('/data/signal-color-shadow/start_guard_enabled'), root / 'start.json')
    prepare = scope['SignalPrepareRuntime']()
    prepare.observation_path = root / 'prepare.json'
    sm, extra = SM(), SM()
    for row in rows:
      raw_mode = bytes(row.get('mode_bytes', [50]))
      assert raw_mode in (b'0', b'1', b'2'), 'raw-prefix mode parsing has its own source gate'
      current_mode[0] = int(raw_mode)
      acquisitions = {v['event']: v['fields'] for v in row['events'] if v.get('event') in ('signalStartAcquisition', 'signalPrepareAcquisition')}
      native_start = acquisitions['signalStartAcquisition']
      native_prepare = acquisitions['signalPrepareAcquisition']
      for file in row['sent']:
        with log.Event.from_bytes(Path(file).read_bytes()) as event:
          value = event.to_dict()
        name = next(k for k in value if k not in ('logMonoTime', 'valid'))
        (extra if name in ('liveCalibration', 'roadCameraState', 'deviceState') else sm)[name] = namespace(value[name])
      sm.attach(native_start['services'])
      extra.attach(native_prepare['extra'])
      prepare.extra = extra
      for name, native in [('start', native_start), ('prepare', native_prepare)]:
        raw = row.get('endpoint_bytes', {}).get(name, native['reader']['bytes'])
        endpoint = root / (name + '.json')
        if raw is None:
          endpoint.unlink(missing_ok=True)
        else:
          endpoint.write_bytes(bytes(raw))
      now = native_start['context']['now']
      times = iter([now] * 8)
      observation, context = start.read(sm)
      expected_start = dict(observation=observation, context=context, reader=reader_state(start))
      previous_mode = prepare.mode
      now = native_prepare['read_now']
      times = iter([now, now, now, native_prepare['context']['now']])
      observation, context = prepare.read(sm)
      expected_prepare = dict(observation=observation, context={key: context[key] for key in ('now', 'enabled', 'valid', 'drive')},
                              reader=reader_state(prepare), actuate=context['actuate'], turning=context['turning'],
                              mode_since=prepare.mode_since, mode_changed=previous_mode != prepare.mode)
      if HOOK is not None:
        HOOK(row, sm, start, prepare, expected_start, observation, context)
      expected.append(dict(start=expected_start, prepare=expected_prepare))
      actual.append(dict(start={key: native_start[key] for key in expected_start},
                         prepare={key: native_prepare[key] for key in expected_prepare}))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  (args.output / 'actual.json').write_text(json.dumps(actual))
  receipt = compare(actual, expected)
  receipt.update(frames=len(rows), scope='actual-capture source reader/cache/current geometry context; full owner/wire separate')
  (args.output / 'receipt.json').write_text(json.dumps(receipt) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()
