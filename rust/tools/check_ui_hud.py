import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import numpy as np
from PIL import Image
from openpilot.cereal import log


def packet(name, values):
  event = log.Event.new_message(valid=True)
  if name == 'onroadEvents':
    target = event.init(name, len(values))
    for item, value in zip(target, values, strict=True):
      item.name = value
  else:
    event.init(name).from_dict(values)
  return list(event.to_bytes())


def step(
  frame,
  *,
  car=None,
  controls=None,
  control=None,
  live_parameters=None,
  longitudinal=None,
  man=None,
  events=(),
  params=None,
  memory=None,
  device=None,
  peripheral=None,
  model=None,
  radar=None,
  **options,
):
  controls = {'lateralControlState': {'torqueState': {}}, **(controls or {})}
  values = {
    'carState': {'vEgo': 13.0, 'vCruiseCluster': 80.0, 'gearShifter': 'drive', **(car or {})},
    'controlsState': controls,
    'selfdriveState': {'enabled': frame < 100},
    'carControl': {'latActive': True, **(control or {})},
    'liveParameters': {'roll': 0.03, 'steerRatio': 13.4, **(live_parameters or {})},
    'carOutput': {'actuatorsOutput': {'torque': -0.88}},
    'longitudinalPlan': {'myDrivingMode': 2, **(longitudinal or {})},
    'carrotMan': {'szPosRoadName': '한강로 / River Road', **(man or {})},
    'carrotNavi': {'connected': True},
    'deviceState': {'cpuTempC': [61.0, 68.5, 73.0], **(device or {})},
    'peripheralState': peripheral or {},
    'gpsLocationExternal': {'hasFix': True},
    'modelV2': model or {},
    'radarState': radar or {},
    'onroadEvents': events,
  }
  return {'frame': frame, 'messages': [packet(name, value) for name, value in values.items()], 'params': params or {}, 'memory': memory or {}, **options}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--display', required=True)
  parser.add_argument('--filter', default='')
  parser.add_argument('--large', action='store_true')
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  root = Path(__file__).resolve().parents[2]
  scenarios = [
    (
      'speed-cluster',
      [
        step(0),
        step(15, car={'vEgoCluster': 12.34}),
        step(40, car={'vEgoCluster': 0.0, 'vCruiseCluster': 0}, controls={'deprecated': {'vCruise': 90}}),
        step(80, car={'vCruiseCluster': -1}),
        step(100),
      ],
    ),
    (
      'torque-lanes',
      [
        step(0, events=['preLaneChangeLeft'], lat_active=True),
        step(20, events=['laneChange']),
        step(40, events=['preLaneChangeRight']),
        step(60, events=['laneChange']),
        step(80, critical=True),
        step(100, controls={'activeLaneLine': True}),
      ],
    ),
    (
      'angle-control',
      [
        step(0, controls={'lateralControlState': {'angleState': {}}, 'curvature': 0.007, 'desiredCurvature': 0.025}),
        step(40, controls={'lateralControlState': {'angleState': {}}, 'curvature': -0.003, 'desiredCurvature': -0.016}, lat_active=True),
        step(80, started_frame=9999),
      ],
    ),
    (
      'override-navigation',
      [
        step(0, longitudinal={'cruiseTarget': 91}),
        step(20, man={'desiredSpeed': 65, 'desiredSource': 'hda', 'decelProvider': 'hda', 'vehicleNaviAvailable': True}),
        step(40, man={'desiredSpeed': 45, 'desiredSource': 'cam:n', 'decelProvider': 'naver_v1', 'naviOwner': 'naver_v1', 'naviLifecycle': 'guiding'}),
        step(80, man={'naviLifecycle': 'idle', 'vehicleNaviAvailable': True}),
      ],
    ),
    (
      'date-debug',
      [
        step(0, params={'ShowDateTime': '1', 'ShowDebugUI': '1'}),
        step(30, params={'ShowDateTime': '2'}),
        step(60, params={'ShowDateTime': '3'}),
        step(90, params={'ShowDateTime': '0'}),
      ],
    ),
    (
      'hold-gears',
      [
        step(0, car={'brakeHoldActive': True, 'gearShifter': 'park'}),
        step(20, car={'softHoldActive': 1, 'gearShifter': 'reverse'}),
        step(40, car={'carrotCruise': 1, 'gearStep': 6}),
        step(60, car={'gearShifter': 'eco'}, longitudinal={'myDrivingMode': 4}),
        step(90, car={'gearShifter': 'manumatic'}),
      ],
    ),
    (
      'traffic',
      [
        step(0, memory={'TrafficLight': '{"lamp":"red","remain":13,"ts":0}'}),
        step(20, memory={'TrafficLight': '{"lamp":"green","remain":8,"ts":1}'}),
        step(40, memory={'TrafficLight': '{"lamp":"left","remain":7}'}),
        step(60, memory={'TrafficLight': '{"lamp":"right","remain":5}'}),
        step(80, memory={'TrafficLight': '{"lamp":"uturn","remain":4}'}),
        step(100, memory={'TrafficLight': '{"lamp":"red","remain":4,"ts":1}'}),
      ],
    ),
    (
      'traffic-numeric',
      [
        step(0, memory={'TrafficLight': '{"lamp":"red","remain":184467440737095516160,"ts":NaN}'}),
        step(20, memory={'TrafficLight': json.dumps({'lamp': '\u001cgreen\u00a0', 'remain': '+１_２', 'ts': '１_０.0'})}),
        step(40, memory={'TrafficLight': json.dumps({'lamp': 'left', 'remain': 1e24, 'ts': float('inf')})}),
        step(60, memory={'TrafficLight': '{"lamp":"right","remain":true,"ts":"nan"}'}),
        step(80, memory={'TrafficLight': json.dumps({'lamp': 'uturn', 'remain': 4, 'ts': 10**400})}),
        step(100, memory={'TrafficLight': '{"lamp":"red","remain":4,"ts":"1__0"}'}),
      ],
    ),
    (
      'egpu',
      [
        step(0, params={'UsbGpuPresent': '1'}),
        step(20, params={'UsbGpuActive': '1'}),
        step(40, params={'UsbGpuLoading': '1'}),
        step(60, params={'UsbGpuStartupFailed': '1'}),
        step(80, params={'UsbGpuStartupFailed': '0', 'UsbGpuLoading': '0'}),
      ],
    ),
  ]
  if args.large:
    scenarios += [
      (
        'device-tpms',
        [
          step(
            0,
            params={'ShowDeviceState': '1', 'ShowTpms': '3'},
            device={'memoryUsagePercent': 92, 'freeSpacePercent': 15, 'cpuTempC': [85.0, 90.0]},
            peripheral={'voltage': 12400},
            car={'tpms': {'fl': 30.5, 'fr': 5.0, 'rl': 60.0, 'rr': 61.0}},
          ),
          step(60, params={'ShowDeviceState': '0', 'ShowTpms': '1'}, car={'tpms': {'fl': 35.5, 'fr': 32.5, 'rl': 0.0, 'rr': 4.9}}),
        ],
      ),
      (
        'navigation-turns',
        [
          step(
            frame,
            man={
              'nGoPosDist': 43210,
              'nGoPosTime': 3551,
              'xTurnInfo': turn,
              'xDistToTurn': 820 if frame < 60 else 2350,
              'szTBTMainText': '강남역 / Gangnam Station',
              'szSdiDescr': '속도 단속 / Speed limit' if frame < 60 else '',
              'atcType': 'prepare' if frame < 40 else 'turn',
              'xSpdLimit': 60,
              'trafficState': 1 if frame < 40 else 2,
            },
          )
          for frame, turn in [(0, 1), (20, 2), (40, 3), (60, 4), (80, 7), (100, 8)]
        ],
      ),
      (
        'plot-modes',
        [
          step(
            frame,
            params={'ShowPlotMode': str(mode)},
            car={'aEgo': -1.25, 'steeringAngleDeg': 2.25},
            control={'actuators': {'steeringAngleDeg': -3.5, 'curvature': 0.0023}},
            live_parameters={'angleOffsetDeg': 0.12},
            longitudinal={'accels': [1.2], 'speeds': [14.6]},
            model={'position': {'x': list(range(33))}, 'velocity': {'x': [17.0] * 33}},
            radar={'leadOne': {'aLeadK': -0.7, 'vRel': -2.2, 'aLead': 1.2, 'jLead': 0.4}},
          )
          for frame, mode in [(40 * index, index + 1) for index in range(8)]
        ],
      ),
    ]
  results = []
  for name, steps in scenarios:
    if args.filter and args.filter not in name:
      continue
    for language in ['en', 'ko']:
      slug = f'{name}-{language}'
      frame_count = 320 if name == 'plot-modes' else 120
      scene = {
        'kind': 'hud',
        'config': {'big': args.large, 'large_viewport': args.large, 'pc': True, 'scale': 1.0},
        'language': language,
        'rect': {'x': 20.3, 'y': 30.7, 'width': 2080.4 if args.large else 1080.4, 'height': 980.2 if args.large else 450.2},
        'frames': frame_count,
        'prime': 0,
        'params': {'IsMetric': '1' if language == 'en' else '0', 'LongitudinalPersonality': '2'},
        'car': {'alpha_longitudinal_available': False, 'openpilot_longitudinal_control': True, 'max_lateral_accel': 3.0},
        'background': [31, 37, 43, 255],
        'capture_frames': list(range(frame_count)),
        'hud': {'steps': steps},
      }
      path = args.output / f'{slug}.json'
      path.write_text(json.dumps(scene))
      outputs = []
      with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-hud148-', dir='/dev/shm') as namespace:
        env = dict(
          os.environ,
          DISPLAY=args.display,
          PYTHONPATH=os.environ['UI_MSGQ_PYTHON'] + os.pathsep + str(root),
          OFFSCREEN='1',
          OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'),
        )
        for lane in ['source', 'native']:
          output = args.output / f'{lane}-{slug}.png'
          command = (
            [sys.executable, str(root / 'rust/tools/ui_application_qa/product_source.py'), str(path), str(output)]
            if lane == 'source'
            else [str(args.binary), str(root), str(path), str(output)]
          )
          with output.with_suffix('.log').open('w') as capture:
            subprocess.run(command, env=env, stdout=capture, stderr=subprocess.STDOUT, check=True)
          outputs.append(output)
      traces = [json.loads(output.with_suffix('.json').read_text()) for output in outputs]
      states = [{'frame': frame, 'source': source, 'native': native} for frame, (source, native) in enumerate(zip(*traces, strict=True)) if source != native]
      pixels = []
      for frame in range(frame_count):
        images = [np.asarray(Image.open(output.with_name(f'{output.stem}-frame-{frame:04}.png'))) for output in outputs]
        if not np.array_equal(*images):
          pixels.append({'frame': frame, 'pixels': int(np.any(images[0] != images[1], axis=-1).sum())})
      result = {'scene': slug, 'frames': len(traces[0]), 'state_differences': states, 'pixel_differences': pixels}
      if name == 'plot-modes':
        result['observed_modes'] = [sorted({entry['hud']['settings'][3] for entry in trace}) for trace in traces]
        assert result['observed_modes'] == [list(range(1, 9))] * 2, result
      results.append(result)
      (args.output / 'results.json').write_text(json.dumps(results, indent=2))
      print(json.dumps(result), flush=True)
      if states or pixels:
        raise AssertionError(result)
  print(f'PASS {len(results)} actual source/native HUD scenarios with exact states and RGBA frames')


if __name__ == '__main__':
  main()
