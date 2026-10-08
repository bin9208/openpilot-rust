#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
from pathlib import Path
import random
import subprocess
import threading
from types import SimpleNamespace

import numpy as np
from openpilot.selfdrive.carrot.xiaoge import v_asm_server as original
from openpilot.selfdrive.carrot.xiaoge.nv12 import pack_nv12, nv12_y_plane
from openpilot.selfdrive.carrot.xiaoge.v_asm_inference import VASMInference

FIELDS = ('threshold', 'smoothingSeconds', 'baseIntervalSeconds', 'laneThreshold', 'laneIntervalSeconds')
ATTRIBUTES = ('threshold', 'smoothing_seconds', 'base_interval_seconds', 'lane_threshold', 'lane_interval_seconds')
DEFAULTS = (0.45, 0.2, 0.25, 0.25, 0.4)
KEYS = tuple(original.PARAM_SETTING_DEFAULTS)


def service():
  result = original.VASMService.__new__(original.VASMService)
  result.lock = threading.Lock()
  result.params = None
  result.last_param_refresh_at = 0.0
  for key, value in zip(ATTRIBUTES, DEFAULTS, strict=True):
    setattr(result, key, value)
  result.status = lambda: {key: getattr(result, attr) for key, attr in zip(FIELDS, ATTRIBUTES, strict=True)}
  return result


def expected(request):
  try:
    match request['op']:
      case 'config':
        return {'value': original.normalize_config(request['value'])}
      case 'settings':
        target = service()
        saved = []
        target._persist_settings = lambda values: saved.extend(map(list, values.items()))
        return {'value': target.set_settings(request['value']), 'parameters': saved}
      case 'params':
        target = service()
        values = dict(zip(KEYS, request['values'], strict=True))
        target.params = SimpleNamespace(get=lambda name: None if values[name] is None else bytes(values[name]))
        target._refresh_settings_from_params(force=True)
        return {'value': target.status()}
      case 'gate':
        target = service()
        value = request['input']
        direction = getattr(original.log.LaneChangeDirection, value['direction'])
        messages = {'carState': SimpleNamespace(vEgo=value['speed']),
                    'modelV2': SimpleNamespace(meta=SimpleNamespace(laneChangeDirection=direction,
                        laneWidthLeft=value['left_width'], laneWidthRight=value['right_width']))}
        class Inputs(dict):
          def update(self, timeout):
            assert timeout == 0
          def all_alive(self, names):
            return value['alive_valid']
          def all_valid(self, names):
            return value['alive_valid']
        target.sm = Inputs(messages)
        target._update_vasm_gate()
        return {'value': target.vasm_gate}
      case 'smoothing':
        target = VASMInference()
        target._prepare_geometry = lambda height, width: None
        rows = []
        for step in request['steps']:
          target._confidence = lambda frame, height, side, confidence=step['confidence']: confidence
          target.update(np.empty((0, 0), dtype=np.uint8), 0, 0, step['side'], step['threshold'], step['smoothing'], step['dt'])
          rows.append([{'score': target.scores[side], 'active': target.active[side], 'confidence': target.confidence[side]}
                       for side in ('left', 'right')])
        return {'value': rows}
      case 'nv12':
        raw = bytes(request['bytes'])
        width, height, stride = (request[key] for key in ('width', 'height', 'stride'))
        packed = pack_nv12(raw, width, height, stride, request['uv_offset'])
        y = nv12_y_plane(raw, width, height, stride)
        size = min(width, height)
        x, top = (width - size) // 2, (height - size) // 2
        return {'packed': packed.flatten().tolist(), 'square': y[top:top + size, x:x + size].flatten().tolist()}
      case _:
        raise AssertionError(request)
  except ValueError as error:
    return {'error': str(error)}
  except OverflowError as error:
    return {'exception': 'OverflowError', 'error': str(error)}


def cases():
  result = []
  config = {'width': 100, 'height': 80, 'poly_left': [[0, 0], [20, 20], [0, 79]], 'poly_right': []}
  for value in [None, False, [], {}, original.DEFAULT_POLYGONS, config]:
    result.append({'op': 'config', 'value': value})
  for field, value in itertools.product(('width', 'height'),
      [0, -1, 1, 8192, 8193, 100.5, '100', '１００', '1_00', '1e2', '100.0', False, None, [], {}, 1e100]):
    result.append({'op': 'config', 'value': config | {field: value}})
  for side, value in itertools.product(('poly_left', 'poly_right'), [None, {}, [], [[1, 1]], [[1, 1]] * 65,
      [[0, 0], [1.5, 2.5], [10, 10]], [[0, 0], [100, 5], [10, 10]],
      [[0, 0], ['2', '３'], [10, 10]], [[0, 0], ['inf', 'nan'], [10, 10]],
      [[0, 0], [None, 3], [10, 10]], [[0, 0], [1, 2, 3], [10, 10]]]):
    result.append({'op': 'config', 'value': config | {side: value}})
  for value in [None, False, [], {}, dict(zip(FIELDS, DEFAULTS, strict=True))]:
    result.append({'op': 'settings', 'value': value})
  for field, value in itertools.product(FIELDS,
      [None, False, True, '0.5', '０.５', 'NaN', 'Infinity', -1, 0, .049, .05, .1, .25, .455, .465, .5, 1, 2, 2.001, [], {}]):
    result.append({'op': 'settings', 'value': {field: value}})
  for raw in [None, b'', b'-1', b'1_000', b'5000', b'300.0', b' 300 ', b'NaN', b'Infinity', b'1e2', b'\xff', b'1' * 400, b'0' * 4301]:
    result.append({'op': 'params', 'values': [None if raw is None else list(raw)] * 5})
  for alive, direction, speed, width in itertools.product((False, True), ('none', 'left', 'right'),
      (0, 29.9 / 3.6, 30 / 3.6, 20, 120 / 3.6, 120.1 / 3.6), (0, 2.99, 3, 3.01)):
    result.append({'op': 'gate', 'input': {'alive_valid': alive, 'direction': direction, 'speed': speed, 'left_width': width, 'right_width': width + .01}})
  rng = random.Random(200)
  result.append({'op': 'smoothing', 'steps': [{'side': rng.choice(('left', 'right')), 'confidence': rng.choice((0, .25, .45, .65, 1)),
      'threshold': rng.choice((.25, .45, 1)), 'smoothing': rng.choice((0, .1, .2, .5)), 'dt': rng.choice((0, .05, .1, .25, 1))} for _ in range(400)]})
  for width, height, padding, gap in itertools.product((2, 4, 8), (2, 4, 8), (0, 2), (0, 12)):
    stride = width + padding
    offset = stride * height + gap
    size = offset + stride * height // 2
    raw = [i % 256 for i in range(size + 8)]
    request = {'op': 'nv12', 'bytes': raw, 'width': width, 'height': height, 'stride': stride, 'uv_offset': offset}
    result.extend([request, request | {'bytes': raw[:size - 1]}, request | {'uv_offset': stride * height - 1}])
  return result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  inputs = cases()
  data = json.dumps(inputs).encode()
  (args.output / 'input.json').write_bytes(data)
  source = [expected(request) for request in inputs]
  (args.output / 'source.json').write_text(json.dumps(source, indent=2) + '\n')
  process = subprocess.run([args.binary.resolve()], input=data, capture_output=True, check=False)
  (args.output / 'native.json').write_bytes(process.stdout)
  (args.output / 'native.stderr').write_bytes(process.stderr)
  process.check_returncode()
  native = json.loads(process.stdout)
  assert len(source) == len(native)
  for index, (wanted, got) in enumerate(zip(source, native, strict=True)):
    assert wanted == got, (index, inputs[index], wanted, got)
  result = {'status': 'PASS', 'cases': len(inputs), 'source': str(Path(original.__file__).resolve()),
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'source_sha256': hashlib.sha256(Path(original.__file__).read_bytes()).hexdigest()}
  (args.output / 'receipt.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
