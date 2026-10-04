#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pillow", "pycapnp", "python-xlib"]
# ///
# ─── How to run ───
# 1. Install uv (if not installed):
#      curl -LsSf https://astral.sh/uv/install.sh | sh
# 2. Run with repository native msgq modules on PYTHONPATH:
#      uv run check_ui_calibration.py [ARGS]
# 3. Or use the existing startup-ui Python environment without installing dependencies.
# ──────────────────

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import numpy as np
from typing import TypedDict
from openpilot.cereal import log
from ui_application_qa.calibration_source import oracle


class CalibrationFields(TypedDict, total=False):
  deviceType: str
  sensor: str
  vEgo: float
  calStatus: str
  rpyCalib: list[float]
  wideFromDeviceEuler: list[float]


def packet(name: str, value: CalibrationFields) -> list[int]:
  message = log.Event.new_message()
  message.valid = True
  message.init(name)
  message.from_dict({name: value})
  return list(message.to_bytes())


def main() -> None:
  binary_text, output_text, display = sys.argv[1:]
  binary, output = Path(binary_text).resolve(), Path(output_text).resolve()
  output.mkdir(parents=True, exist_ok=True)
  root = Path(__file__).resolve().parents[2]
  steps = []
  names = []
  pairs = [
    ('pc', 'unknown'),
    ('tici', 'unknown'),
    ('tici', 'ar0231'),
    ('tizi', 'ox03c10'),
    ('mici', 'os04c10'),
    ('tizi', 'os04c10'),
    ('unknown', 'ar0231'),
    ('neo', 'unknown'),
  ]
  for device, sensor in pairs:
    for stream in [0, 2]:
      for speed in [0.0, 10.0, 15.0, 30.0, 45.0]:
        for rotation in [[0.0, 0.0, 0.0], [0.013, -0.027, 0.021], [0.5, 0.3, -0.4]]:
          steps.append(
            {
              'reset': True,
              'rect': {'x': 20.3, 'y': 30.7, 'width': 1560.4 if stream == 0 else 2240.4, 'height': 980.2},
              'stream': stream,
              'speed': speed,
              'messages': [
                packet('deviceState', {'deviceType': device}),
                packet('roadCameraState', {'sensor': sensor}),
                packet('carState', {'vEgo': speed}),
                packet('liveCalibration', {'calStatus': 'calibrated', 'rpyCalib': rotation, 'wideFromDeviceEuler': [0.015, 0.01, -0.023]}),
              ],
            }
          )
          names.append(f'{device}/{sensor}/stream={stream}/speed={speed}/rpy={rotation}')
  for index, (changed, stream, speed) in enumerate([(False, 0, 0.0), (False, 0, 30.0), (False, 2, 0.0), (False, 2, 30.0), (True, 0, 15.0)]):
    messages = [packet('carState', {'vEgo': speed})]
    if index == 0:
      messages += [
        packet('deviceState', {'deviceType': 'tici'}),
        packet('roadCameraState', {'sensor': 'os04c10'}),
        packet('liveCalibration', {'calStatus': 'calibrated', 'rpyCalib': [0.03, 0.04, 0.05]}),
      ]
    if changed:
      messages += [packet('liveCalibration', {'calStatus': 'uncalibrated', 'rpyCalib': [0.4, 0.5, 0.6]})]
    steps.append(
      {
        'reset': index == 0,
        'rect': {'x': 20.3 + index * 10, 'y': 30.7 + index * 10, 'width': 536.4, 'height': 240.2},
        'stream': stream,
        'speed': speed,
        'messages': messages,
      }
    )
    names.append(f'cache-offset-and-speed-{index}')
  (output / 'input.json').write_text(json.dumps(steps))
  results = []
  for big in [False, True]:
    label = 'big' if big else 'compact'
    expected = oracle(root, big, steps)
    with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-calibration148-', dir='/dev/shm') as namespace:
      env = dict(
        os.environ,
        DISPLAY=display,
        BIG=str(int(big)),
        SCALE='1',
        OFFSCREEN='1',
        PARAMS_ROOT=str(output / f'{label}-params'),
        OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'),
      )
      process = subprocess.run([str(binary), str(root), str(output / label)], input=json.dumps(steps), env=env, capture_output=True, text=True, check=True)
    (output / f'{label}.log').write_text(process.stdout + process.stderr)
    line = next(line for line in process.stdout.splitlines() if line.startswith('CALIBRATION_RESULT '))
    actual = json.loads(line.removeprefix('CALIBRATION_RESULT '))
    (output / f'{label}-source.json').write_text(json.dumps(expected))
    (output / f'{label}-native.json').write_text(json.dumps(actual))
    maximum = 0.0
    for name, source, native in zip(names, expected, actual, strict=True):
      assert source['position'] == native['position'], (label, name, source['position'], native['position'])
      for kind in ['camera', 'model']:
        source_matrix, native_matrix = np.array(source[kind]), np.array(native[kind])
        error = float(np.abs(source_matrix - native_matrix).max())
        maximum = max(maximum, error)
        tolerance = 64 * np.finfo(np.float64).eps * np.maximum(1.0, np.abs(source_matrix))
        assert np.all(np.abs(source_matrix - native_matrix) <= tolerance), (label, name, kind, error, source_matrix, native_matrix)
        assert np.array_equal(source_matrix.astype(np.float32), native_matrix.astype(np.float32)), (label, name, kind)
    results.append(
      {'layout': label, 'scenarios': len(names), 'max_absolute_f64_error': maximum, 'all_render_f32_matrices_equal': True, 'device_position_exact': True}
    )
  (output / 'result.json').write_text(
    json.dumps({'verdict': 'PASS', 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'results': results}, indent=2)
  )
  print(json.dumps(results))
  print('PASS original calibration methods versus native transforms and device-position writes')


if __name__ == '__main__':
  main()
