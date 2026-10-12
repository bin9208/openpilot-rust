from __future__ import annotations

from dataclasses import asdict
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def run(root: Path, binary: Path, output: Path) -> None:
  sys.path.insert(0, str(root))
  sys.path.insert(0, str(root / 'openpilot/selfdrive/carrot/cluster'))
  from check_message_state import compare, source
  from cluster_route_replay import RouteLogParser, frame_to_state

  messaging, _ = source()
  parser = RouteLogParser(recompute_cutins=False)
  output.mkdir(parents=True, exist_ok=True)
  methods = {
    'carParams': '_update_car_params',
    'deviceState': '_update_device_state',
    'roadCameraState': '_update_road_camera_state',
    'controlsState': '_update_controls_state',
    'selfdriveState': '_update_selfdrive_state',
    'carControl': '_update_car_control',
    'longitudinalPlan': '_update_longitudinal_plan',
    'lateralPlan': '_update_lateral_plan',
    'cameraOdometry': '_update_camera_odometry',
    'liveCalibration': '_update_live_calibration',
    'navInstruction': '_update_nav_instruction',
    'navInstructionCarrot': '_update_nav_instruction',
    'modelV2': '_update_model_v2',
    'drivingModelData': '_update_driving_model',
    'radarState': '_update_radar_state',
    'liveTracks': '_update_live_tracks',
  }
  steps, expected = [], []

  def message(service, fields, now, valid=True):
    packet = messaging['new_message'](service)
    packet.logMonoTime = int(now * 1e9)
    packet.valid = valid
    getattr(packet, service).from_dict(fields)
    return packet

  def step(now, packets, car_fields=None, onroad=None):
    if onroad is not None:
      parser.trip_report_tracker.set_onroad(onroad)
    paths = []
    for index, packet in enumerate(packets):
      service = packet.which()
      path = output / f'{len(steps):04}-{index}-{service}.capnp'
      path.write_bytes(packet.to_bytes())
      paths.append(str(path))
      method = getattr(parser, methods[service])
      value = getattr(packet, service)
      if service in ('selfdriveState', 'navInstruction', 'navInstructionCarrot', 'modelV2', 'radarState', 'liveTracks'):
        method(value, now)
      elif service in ('longitudinalPlan', 'cameraOdometry', 'liveCalibration'):
        method(value, packet.valid)
      else:
        method(value)
    fields = {
      'vEgo': 20.0,
      'vEgoCluster': 21.0,
      'aEgo': 1.5,
      'steeringAngleDeg': 9.0,
      'vCruiseCluster': 100.5,
      'cruiseState': {'available': True},
      'leftBlinker': True,
      **(car_fields or {}),
    }
    car = message('carState', fields, now)
    path = output / f'{len(steps):04}-carState.capnp'
    path.write_bytes(car.to_bytes())
    paths.append(str(path))
    state = frame_to_state(parser._frame_from_car_state(car.carState, now))
    expected.append(json.loads(json.dumps(asdict(state))))
    steps.append({'now': now, 'messages': paths, 'onroad': onroad})

  step(10.0, [])
  for index, gear in enumerate(['unknown', 'park', 'reverse', 'neutral', 'drive', 'sport', 'low', 'brake', 'eco']):
    step(
      10.1 + index * 0.1,
      [],
      {'gearShifter': gear, 'gearStep': index, 'pcmCruiseGap': index, 'vEgoCluster': 0.0, 'leftLaneLine': index * 5, 'rightLaneLine': 20 - index},
    )
  for now, limit in [(11.0, 30.0), (11.1, 31.0), (11.2, 13.4), (21.2, 0.0), (21.201, 0.0)]:
    step(now, [message('navInstruction', {'speedLimit': limit}, now)])
  step(
    21.3,
    [
      message(
        'selfdriveState',
        {'enabled': True, 'personality': 'relaxed', 'alertSize': 'small', 'alertText1': 'Owned source alert', 'alertStatus': 'userPrompt'},
        21.3,
      )
    ],
    onroad=True,
  )
  step(
    21.4,
    [
      message(
        'longitudinalPlan',
        {
          'speeds': [-2, 20, 100],
          'accels': [-9, 1, 9],
          'jerks': [-20, 2, 20],
          'myDrivingMode': 4,
          'trafficState': 2,
          'tFollow': 1.8,
          'desiredDistance': 40.0,
          'vTargetNow': 18.0,
          'jTargetNow': 2.0,
        },
        21.4,
        False,
      )
    ],
  )
  step(
    21.5,
    [
      message(
        'lateralPlan',
        {'laneWidth': 7.0, 'curvatures': [0.02, 0.09, -0.1], 'curvatureRates': [0.04, 0.1], 'solverCost': 2_000_000, 'latDebugText': 'Z' * 80},
        21.5,
      )
    ],
  )
  step(
    21.6,
    [
      message(
        'cameraOdometry',
        {'trans': [25.0, 0.0, 0.0], 'rot': [0.0, 0.0, 0.2], 'transStd': [22.0, 0.0, 0.0], 'rotStd': [0.0, 0.0, 3.0], 'roadTransformTrans': [0.0, 0.0, 1.2]},
        21.6,
        False,
      )
    ],
  )
  step(21.7, [message('liveCalibration', {'rpyCalib': [0.0, 0.1, 0.2], 'height': [1.6]}, 21.7)])
  step(21.8, [message('cameraOdometry', {'roadTransformTrans': [0.0, 0.0, 2.2]}, 21.8)])
  step(21.9, [message('liveCalibration', {'rpyCalib': [0.0, 0.4, 0.5], 'height': [2.4]}, 21.9, False)])
  step(22.0, [message('deviceState', {'cpuUsagePercent': [10, 50, 120], 'cpuTempC': [-80, 50, 230], 'memoryUsagePercent': 44, 'freeSpacePercent': 65}, 22.0)])
  for index, (accel, speed, angle) in enumerate([(3.0, 20, 9.0), (1.0, 20, 9.0), (-4.0, 20, 9.0), (0.0, 0, 0.0), (3.0, 20, 100.0)] * 4):
    step(22.1 + index * 0.5, [], {'vEgo': speed, 'aEgo': accel, 'steeringAngleDeg': angle})
  step(40.0, [], onroad=False)
  step(40.1, [], onroad=True)
  model_fields = {
    'frameAge': 3,
    'frameDropPerc': 125.0,
    'modelExecutionTime': 0.035,
    'laneLines': [{'x': [0.0, 10.0, 30.0, 80.0], 'y': [offset] * 4, 'yStd': [0.2] * 4} for offset in [-5.4, -1.8, 1.8, 5.4]],
    'laneLineProbs': [0.9, 0.8, 0.9, 0.7],
    'roadEdges': [{'x': [0.0, 10.0, 30.0, 80.0], 'y': [offset] * 4} for offset in [-6.0, 6.0]],
    'roadEdgeStds': [0.2, 0.5],
    'position': {'x': [0.0, 2.0, 5.0, 10.0, 20.0, 40.0, 90.0, 141.0], 'y': [0.0, 0.1, 0.2, 0.3, 0.5, 0.6, 0.8, 1.0], 'yStd': [0.1] * 8},
    'velocity': {'x': [20.0] * 8},
    'acceleration': {'x': [1.0] * 8},
    'orientation': {'z': [0.01] * 8},
    'orientationRate': {'z': [0.02] * 8},
    'leadsV3': [{'prob': 0.9, 'x': [25.0], 'y': [0.5], 'v': [18.0], 'a': [-0.5], 'xStd': [0.2], 'yStd': [0.3]}],
    'action': {'desiredVelocity': 18.0, 'desiredAcceleration': 7.0, 'desiredCurvature': 0.01, 'shouldStop': True},
    'meta': {
      'engagedProb': 0.8,
      'desireState': [0, 0, 0, 0.4, 0.3, 0, 0, 0],
      'desirePrediction': [0.1] * 18,
      'laneChangeProb': 0.7,
      'laneChangeState': 'off',
      'laneChangeDirection': 'none',
      'laneChangeAvailableLeft': True,
      'laneChangeAvailableRight': True,
      'modelTurnSpeed': 15.0,
      'laneWidthLeft': 8.0,
      'laneWidthRight': 3.8,
      'distanceToRoadEdgeLeft': 30.0,
      'distanceToRoadEdgeRight': 4.0,
      'disengagePredictions': {
        't': [2.0, 4.0, 6.0],
        'brakeDisengageProbs': [0.2, 0.8, 0.3],
        'gasDisengageProbs': [0.1, 0.3],
        'steerOverrideProbs': [0.4],
        'brake3MetersPerSecondSquaredProbs': [0.5],
        'brake4MetersPerSecondSquaredProbs': [0.6],
        'brake5MetersPerSecondSquaredProbs': [0.7],
        'gasPressProbs': [0.3, 0.9],
        'brakePressProbs': [0.4, 0.8],
      },
    },
  }
  step(41.0, [message('modelV2', model_fields, 41.0)])
  for index, state in enumerate(['preLaneChange', 'laneChangeStarting', 'laneChangeStarting', 'laneChangeFinishing', 'off', 'off', 'off']):
    fields = {
      **model_fields,
      'meta': {**model_fields['meta'], 'laneChangeState': state, 'laneChangeDirection': 'left', 'desireState': [0, 0, 0, 0.8 - index * 0.1, 0.2, 0, 0, 0]},
    }
    step(41.1 + index * 0.3, [message('modelV2', fields, 41.1 + index * 0.3)])
  step(
    44.0,
    [
      message(
        'drivingModelData',
        {'laneLineMeta': {'leftY': -2.0, 'rightY': 2.2, 'leftProb': 0.2, 'rightProb': 0.9}, 'action': {'desiredVelocity': 80.0, 'desiredAcceleration': -9.0}},
        44.0,
      )
    ],
  )
  step(45.0, [])
  lead_one = {'status': True, 'dRel': 30.0, 'yRel': 2.0, 'vRel': -2.0, 'vLead': 18.0, 'vLat': 0.5, 'aLeadK': -0.2, 'radarTrackId': 11, 'radar': True}
  lead_two = {'status': True, 'dRel': 15.0, 'yRel': -2.0, 'vRel': -3.0, 'vLead': 17.0, 'vLat': -0.5, 'radarTrackId': 42, 'radar': True}
  cutin = {'status': True, 'dRel': 20.0, 'yRel': 1.0, 'vRel': -4.0, 'vLead': 16.0, 'radarTrackId': 99}
  step(46.0, [message('radarState', {'leadOne': lead_one, 'leadTwo': lead_two, 'leadsCutIn': [lead_two, cutin]}, 46.0)])
  step(46.03, [message('radarState', {'leadOne': lead_one, 'leadTwo': {**lead_two, 'status': False}, 'leadsCutIn': []}, 46.03)])
  step(46.081, [message('radarState', {'leadOne': lead_one, 'leadTwo': {**lead_two, 'status': False}, 'leadsCutIn': []}, 46.081)])
  step(46.88, [])
  tracks = [
    {'trackId': index, 'dRel': distance, 'yRel': lateral, 'vRel': -2.0, 'vLead': 18.0, 'yvRel': 0.3, 'aRel': -0.2, 'measured': measured, 'radarSource': source}
    for index, distance, lateral, measured, source in [
      (11, 30, 2, True, 'frontRadar'),
      (210, 15, 3, True, 'corner235'),
      (243, 20, -3, False, 'corner180'),
      (310, 40, 2, True, 'corner430'),
      (1005, 50, 1, True, 'corner430'),
      (20, 0, 0, True, 'frontRadar'),
      (21, 181, 0, True, 'frontRadar'),
      (22, 20, 13, True, 'frontRadar'),
    ]
  ]
  step(47.0, [message('liveTracks', {'points': tracks}, 47.0)])
  step(47.249, [])
  step(47.25, [])
  step(
    47.3,
    [message('carParams', {'brand': 'hyundai', 'extFlags': 4096}, 47.3)],
    {
      'leftLongDist': 8.0,
      'leftLatDist': 3.0,
      'rightLongDist': 20.0,
      'rightLatDist': 4.0,
      'leftRearLongDist': 15.0,
      'leftRearLatDist': 2.0,
      'rightRearLongDist': 25.0,
      'rightRearLatDist': 3.0,
      'leftBlindspot': True,
      'rightBlindspot': False,
    },
  )
  step(47.4, [message('liveTracks', {'points': tracks[:1]}, 47.4)])
  step(48.0, [message('modelV2', model_fields, 48.0)])
  for index, (left_y, right_y) in enumerate([([], []), ([2.0], [-2.0]), ([1.0], [1.0]), ([float('nan')], [2.0])]):
    lines = [dict(line) for line in model_fields['laneLines']]
    lines[1]['y'], lines[2]['y'] = left_y, right_y
    now = 48.1 + index * 0.1
    step(now, [message('modelV2', {**model_fields, 'laneLines': lines}, now)])
  input_path = output / 'input.json'
  input_path.write_text(json.dumps(steps, indent=2))
  command = [str(binary), str(input_path)]
  completed = subprocess.run(command, capture_output=True, text=True, timeout=30)
  (output / 'invocation.json').write_text(
    json.dumps({'command': command, 'exit_code': completed.returncode, 'stdout': completed.stdout, 'stderr': completed.stderr}, indent=2)
  )
  assert completed.returncode == 0, completed.stderr
  actual = json.loads(completed.stdout)
  (output / 'expected.json').write_text(json.dumps(expected, indent=2))
  (output / 'actual.json').write_text(json.dumps(actual, indent=2))
  mismatches = []
  for index, (left, right) in enumerate(zip(expected, actual, strict=True)):
    try:
      compare(left, right)
    except (AssertionError, TypeError) as error:
      mismatches.append({'step': index, 'detail': str(error)})
  result = {
    'pass': not mismatches,
    'steps': len(steps),
    'mismatches': mismatches,
    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'scope': 'original production RouteLogParser methods and frame_to_state, all 148 output fields, including model/lane and recorded radar; navigation media is separate',
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2))
  print(json.dumps(result))
  assert not mismatches


if __name__ == '__main__':
  run(*(Path(argument).resolve() for argument in sys.argv[1:]))
