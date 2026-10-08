#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pillow", "pycapnp", "python-xlib"]
# ///
# ─── How to run ───
# 1. Install uv (if not installed):
#      curl -LsSf https://astral.sh/uv/install.sh | sh
# 2. Run with repository native msgq modules on PYTHONPATH:
#      uv run check_ui_augmented.py [ARGS]
# 3. Or use the existing startup-ui Python environment without installing dependencies.
# ──────────────────

from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
from openpilot.cereal import log
import numpy as np
from PIL import Image
from ui_application_qa.camera_lane import capture
from ui_application_qa.runtime_peer import SERVICES


def messages(speed: float, experimental: bool, alert: bool = False, *, standstill: bool = False) -> list[list[int]]:
  x = [float(index * 3) for index in range(33)]
  values = {
    'deviceState': {'deviceType': 'pc', 'screenBrightnessPercent': 20, 'cpuTempC': [60.0, 61.0]},
    'carState': {
      'vEgo': speed,
      'vEgoCluster': speed,
      'vCruiseCluster': 80.0,
      'gearShifter': 'drive',
      'standstill': standstill,
      'logCarrot': 'native/source border',
      'leftBlinker': alert,
    },
    'selfdriveState': {
      'enabled': False,
      'experimentalMode': experimental,
      'alertSize': 'small' if alert else 'none',
      'alertText1': 'LOOK AHEAD' if alert else '',
      'alertText2': 'source overlay' if alert else '',
      'alertType': 'prompt/test' if alert else '',
    },
    'carControl': {'latActive': False},
    'controlsState': {'lateralControlState': {'torqueState': {}}},
    'driverStateV2': {
      side: {'faceOrientation': [0.0, 0.0, 0.0], 'faceOrientationStd': [0.1, 0.1, 0.1], 'facePosition': [0.0, 0.0], 'faceProb': 1.0}
      for side in ['leftDriverData', 'rightDriverData']
    },
    'liveCalibration': {'calStatus': 'calibrated', 'rpyCalib': [0.0, 0.01, -0.02], 'wideFromDeviceEuler': [0.0, 0.0, 0.0]},
    'modelV2': {
      'position': {'x': x, 'y': [0.0] * 33, 'z': [0.0] * 33},
      'velocity': {'x': [speed] * 33},
      'laneLines': [{'x': x, 'y': [offset] * 33, 'z': [0.0] * 33} for offset in [-4.0, -2.0, 2.0, 4.0]],
      'laneLineProbs': [0.1, 0.9, 0.9, 0.1],
      'roadEdges': [{'x': x, 'y': [offset] * 33, 'z': [0.0] * 33} for offset in [-5.0, 5.0]],
      'roadEdgeStds': [1.0, 1.0],
    },
    'longitudinalPlan': {'myDrivingMode': 3, 'speeds': [speed] * 17, 'accels': [0.0] * 17},
    'liveParameters': {'steerRatio': 13.4},
    'roadCameraState': {'sensor': 'unknown'},
    'wideRoadCameraState': {'sensor': 'unknown'},
    'carParams': {'maxLateralAccel': 3.0, 'openpilotLongitudinalControl': True},
  }
  packets = []
  for name in SERVICES:
    event = log.Event.new_message()
    event.valid = True
    event.init(name, 0) if name in ['pandaStates', 'onroadEvents', 'customReservedRawData0'] else event.init(name)
    if name in values:
      event.from_dict({name: values[name]})
    packets.append(list(event.to_bytes()))
  return packets


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--peer', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--display', required=True)
  parser.add_argument('--filter', default='')
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  assert shutil.disk_usage(root).free > (25 + 0.5) * 1024**3
  args.output.mkdir(parents=True, exist_ok=True)
  results = []
  for big in [False, True]:
    for language in ['en', 'ko']:
      for name in ['baseline', 'alert-stream', 'cluster', 'view-border']:
        label = f'{"big" if big else "compact"}-{name}-{language}'
        if args.filter and args.filter not in label:
          continue
        base = {'frame': 0, 'messages': messages(20.0, False), 'started': True, 'status': 0, 'params': {'IsMetric': '1', 'CustomSR': '135'}, 'memory': {}}
        steps = [copy.deepcopy(base)]
        if name == 'alert-stream':
          steps += [dict(base, frame=10, messages=messages(0.0, True, True)), dict(base, frame=20, messages=messages(20.0, True))]
        elif name == 'cluster':
          steps += [dict(base, frame=10, suppress=True), dict(base, frame=20, suppress=False)]
        elif name == 'view-border':
          steps = (
            [
              dict(base, params=dict(base['params'], ShowModelView='2', ShowCustomBrightness='50'), started_time=-20.0),
              dict(base, frame=10, params=dict(base['params'], ShowModelView='3', ShowCustomBrightness='50'), started_time=-20.0),
              dict(base, frame=20, status=2, lat_active=True, params=dict(base['params'], ShowModelView='1', ShowCustomBrightness='50'), started_time=-20.0),
            ]
            if not big
            else [
              dict(base, params=dict(base['params'], CustomSR='0.1tail')),
              dict(base, frame=10, status=1, lat_active=True, messages=messages(20.0, False, True)),
              dict(base, frame=20, status=2),
            ]
          )
        scene = {
          'kind': 'augmented',
          'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0},
          'language': language,
          'rect': {'x': 0.0, 'y': 0.0, 'width': 2160 if big else 536, 'height': 1080 if big else 240},
          'frames': 32,
          'prime': 0,
          'params': base['params'],
          'capture_frames': list(range(32)),
          'background': [0, 0, 0, 255],
          'camera': {'stream': 0},
          'road': {'steps': steps},
        }
        path = args.output / f'{label}.json'
        path.write_text(json.dumps(scene))
        traces = [
          capture(root, args.binary.resolve(), args.peer.resolve(), path, args.output / f'{lane}-{label}.png', args.display, 32, lane)
          for lane in ['source', 'native']
        ]
        state = [{'frame': index, 'source': a, 'native': b} for index, (a, b) in enumerate(zip(*traces, strict=True)) if a != b]
        pixels = []
        for frame in range(32):
          images = [np.asarray(Image.open(args.output / f'{lane}-{label}-frame-{frame:04}.png')).astype(np.int16) for lane in ['source', 'native']]
          delta = np.abs(images[0] - images[1])
          if np.any(delta):
            pixels.append({'frame': frame, 'different_pixels': int(np.any(delta, axis=-1).sum()), 'maximum_channel': int(delta.max())})
        result = {'scene': label, 'frames': len(traces[0]), 'state_differences': state, 'pixel_differences': pixels}
        results.append(result)
        (args.output / 'results.json').write_text(json.dumps(results, indent=2))
        print(json.dumps(result), flush=True)
        assert not state and not pixels, result
  (args.output / 'receipt.json').write_text(
    json.dumps(
      {
        'verdict': 'PASS',
        'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
        'scenarios': len(results),
        'frames': sum(result['frames'] for result in results),
      },
      indent=2,
    )
  )
  print('PASS original augmented road composition versus native pixels and camera state')


if __name__ == '__main__':
  main()
