"""Owned cereal inputs for model renderer state and real-GL comparisons."""

import copy
import math
from openpilot.cereal import car, log

SERVICES = [
  'modelV2',
  'controlsState',
  'onroadEvents',
  'liveCalibration',
  'radarState',
  'deviceState',
  'pandaStates',
  'carParams',
  'driverMonitoringState',
  'carState',
  'driverStateV2',
  'roadCameraState',
  'wideRoadCameraState',
  'managerState',
  'selfdriveState',
  'longitudinalPlan',
  'gpsLocationExternal',
  'carOutput',
  'carControl',
  'liveParameters',
  'rawAudioData',
  'carrotMan',
  'carrotNavi',
  'peripheralState',
  'liveDelay',
  'liveTorqueParameters',
  'lateralPlan',
  'customReservedRawData0',
]
PARAMS = {
  'ShowLaneInfo': '2',
  'ShowRadarInfo': '3',
  'ShowPathMode': '13',
  'ShowPathColor': '14',
  'ShowPathModeLane': '13',
  'ShowPathColorLane': '14',
  'ShowPathColorCruiseOff': '14',
  'CarrotTireTrajectory': '1',
  'IsMetric': '1',
}


def xyz(xs, ys, zs):
  return {'x': xs, 'y': ys, 'z': zs}


def lead(distance=30.0, lateral=0.0, speed=20.0, **changes):
  return {
    'status': True,
    'dRel': distance,
    'yRel': lateral,
    'vRel': -3.0,
    'vLeadK': speed,
    'vLat': 0.0,
    'radar': True,
    'radarTrackId': 2,
    'modelProb': 0.95,
    **changes,
  }


def messages(curve=0.0, hill=0.0):
  xs = [i * 3.125 for i in range(33)]
  ys = [math.sin(x / 40.0) * curve for x in xs]
  zs = [math.sin(x / 30.0) * hill for x in xs]
  path = xyz(xs, ys, zs)
  lanes = [xyz(xs, [y + offset for y in ys], [z + 1.22 for z in zs]) for offset in [5.4, 1.8, -1.8, -5.4]]
  roads = [xyz(xs, [y + offset for y in ys], [z + 1.22 for z in zs]) for offset in [7.0, -7.0]]
  model = {
    'position': path,
    'laneLines': lanes,
    'laneLineProbs': [0.8, 0.95, 0.95, 0.8],
    'roadEdges': roads,
    'roadEdgeStds': [0.2, 0.8],
    'acceleration': {'x': [math.sin(x / 20.0) * 2.0 for x in xs]},
    'leadsV3': [{'prob': 0.9, 'x': [31.52]}],
    'meta': {'laneChangeState': 'off', 'laneChangeDirection': 'none'},
  }
  return {
    'modelV2': model,
    'liveCalibration': {'height': [1.22]},
    'carParams': {'openpilotLongitudinalControl': True},
    'selfdriveState': {'enabled': True, 'experimentalMode': False},
    'carState': {'vEgo': 20.0, 'vEgoCluster': 20.0, 'aEgo': 0.0, 'leftLaneLine': 20, 'rightLaneLine': 11, 'useLaneLineSpeed': 1.0},
    'carOutput': {'actuatorsOutput': {'torque': 0.0}},
    'controlsState': {'activeLaneLine': False},
    'lateralPlan': {'position': xyz(xs, [y + 0.25 for y in ys], zs)},
    'longitudinalPlan': {'xState': 2, 'allowThrottle': True, 'tFollow': 1.5, 'desiredDistance': 25.0, 'accels': [0.6], 'longitudinalPlanSource': 'cruise'},
    'radarState': {
      'leadOne': lead(),
      'leadTwo': lead(42.0, -0.25),
      'leadLeft': lead(12.0, 3.6),
      'leadRight': lead(80.0, -3.6),
      'leadsLeft': [lead(35.0, 3.6, 15.0, vLat=-2.0), lead(16.0, 3.5, 0.0)],
      'leadsRight': [lead(20.0, -3.5, -10.0, modelProb=0.01), lead(50.0, -3.5, 12.0, radar=False)],
      'leadsCenter': [lead(65.0, 0.5, 21.0)],
    },
  }


def packet(name, body, time, valid=True):
  event = log.Event.new_message(valid=valid, logMonoTime=round(time * 1e9))
  event.init(name)
  getattr(event, name).from_dict(body)
  return list(event.to_bytes())


def scene(name, big, bodies=None, settings=None, count=12, mutate=None):
  data = messages() if bodies is None else bodies
  params = {key: list(value.encode()) for key, value in (PARAMS | (settings or {})).items()}
  params['CarParams'] = list(car.CarParams.new_message(openpilotLongitudinalControl=True).to_bytes())
  transform = [[1080.0, 900.0, 0.0], [540.0, 0.0, 900.0], [1.0, 0.0, 0.0]] if big else [[268.0, 220.0, 0.0], [95.0, 0.0, 220.0], [1.0, 0.0, 0.0]]
  steps = []
  for index in range(count):
    frame_data = copy.deepcopy(data)
    meta = {'status': 'engaged', 'lat_active': True, 'started_frame': 0, 'is_metric': True, 'show_radar_info': 3}
    step = {'now': 100.0 + index * 0.05, 'ui': meta, 'params': {}, 'capture': True}
    if index == 0:
      step['transform'] = transform
    invalid, omit = set(), set()
    if mutate:
      mutate(index, frame_data, step, invalid, omit)
    step['messages'] = [packet(service, body, step['now'], service not in invalid) for service, body in frame_data.items() if service not in omit]
    steps.append(step)
  return {'name': name, 'params': params, 'rect': {'x': 0.0, 'y': 0.0, 'width': 2160.0 if big else 536.0, 'height': 1080.0 if big else 240.0}, 'steps': steps}


def suite(big, smoke=False):
  result = [scene('baseline', big)]
  if smoke:
    return result
  if big:
    for mode in range(16):
      result.append(scene(f'path-mode-{mode}', big, messages(curve=4.0, hill=0.3), {'ShowPathMode': str(mode), 'ShowPathColor': str(10 + mode % 10)}))

  def transitions(i, bodies, step, invalid, omit):
    bodies['selfdriveState']['experimentalMode'] = i >= 10
    bodies['longitudinalPlan']['allowThrottle'] = i < 5 or i >= 30
    bodies['carOutput']['actuatorsOutput']['torque'] = 0.9 if i < 25 else -0.9
    step['ui']['status'] = ['disengaged', 'engaged', 'override'][i // 12 % 3]
    step['ui']['lat_active'] = i % 16 >= 8

  result.append(scene('throttle-experimental-torque', big, messages(curve=-3.0, hill=0.5), count=48, mutate=transitions))

  def validity(i, bodies, step, invalid, omit):
    if 3 <= i < 8:
      omit.update(['modelV2', 'radarState'])
    if 8 <= i < 12:
      invalid.update(['modelV2', 'carState', 'radarState'])
    if 12 <= i < 15:
      step['ui']['started_frame'] = 1000
    if i == 16:
      step['transform'] = [[100.0, -200.0, 0.0], [100.0, 0.0, 200.0], [0.0, 0.0, 0.0]]
    if i == 18:
      step['transform'] = [[268.0, 220.0, 0.0], [95.0, 0.0, 220.0], [1.0, 0.0, 0.0]]

  result.append(scene('validity-cache-depth', big, count=24, mutate=validity))

  def markings(i, bodies, step, invalid, omit):
    bodies['carState']['leftLaneLine'] = [24, 20, 11, -1, -6][i // 4 % 5]
    bodies['carState']['rightLaneLine'] = [20, 24, -1, 0, 31][i // 4 % 5]
    bodies['modelV2']['laneLineProbs'] = [0.3, 0.30001 if i % 2 else 0.3, 0.9 if i % 3 else 0.1, 0.8]
    bodies['modelV2']['roadEdgeStds'] = [-0.2, 3.0]
    bodies['carState']['leftBlindspot'] = 4 <= i < 12
    bodies['carState']['rightBlindspot'] = 8 <= i < 16
    bodies['modelV2']['meta'] = {'laneChangeState': 'preLaneChange', 'laneChangeDirection': 'left' if i < 10 else 'right'}
    bodies['radarState']['leadRight']['dRel'] = 12.0

  result.append(scene('markings-roads-blindspots-assist', big, messages(curve=3.0), count=20, mutate=markings))

  def lead_states(i, bodies, step, invalid, omit):
    bodies['radarState']['leadOne'].update(status=i % 8 != 0, radar=i % 6 != 0, radarTrackId=i % 4 - 1, dRel=4.0 + i * 3.0, yRel=math.sin(i / 3.0))
    bodies['radarState']['leadTwo'].update(dRel=50.0, radar=i % 5 != 0)
    bodies['longitudinalPlan']['longitudinalPlanSource'] = 'lead0' if i % 2 else 'cruise'
    bodies['modelV2']['leadsV3'][0]['prob'] = 0.9 if i % 4 else 0.4
    step['ui']['is_metric'] = i < 12
    step['ui']['show_radar_info'] = i % 4

  result.append(scene('leads-radar-metric', big, count=24, mutate=lead_states))

  def path_states(i, bodies, step, invalid, omit):
    bodies['controlsState']['activeLaneLine'] = i % 2 == 0
    bodies['selfdriveState']['enabled'] = i % 5 != 0
    bodies['carState'].update(
      brakeLights=i % 3 == 0,
      brakeHoldActive=i == 6,
      softHoldActive=1 if i == 7 else 0,
      carrotCruise=1 if i == 8 else 0,
      aEgo=-2.0 if i < 10 else 1.0,
      vEgoCluster=0.0 if i % 9 == 0 else 22.0,
    )
    bodies['longitudinalPlan'].update(xState=i % 6, trafficState=1001 if i % 2 else 1, accels=[[-1.0, 0.0, 0.5][i % 3]])
    bodies['carState']['vEgo'] = 0.0 if i % 3 == 0 else 20.0
    if i >= 20:
      step['params'] = {'ShowPathMode': list(b'7'), 'ShowPathModeLane': list(b'10'), 'ShowLaneInfo': list(b'0'), 'ShowRadarInfo': list(b'0')}

  result.append(
    scene('path-holds-color-refresh', big, settings={'ShowPathColor': '20', 'ShowPathColorLane': '20', 'ShowPathModeLane': '12'}, count=44, mutate=path_states)
  )
  for drift in [-0.7, -0.3, 0.0, 0.3, 0.7]:
    bodies = messages(curve=1.0, hill=-0.4)
    for line in bodies['modelV2']['laneLines']:
      line['y'] = [y - drift for y in line['y']]
    result.append(scene(f'tire-drift-{drift}', big, bodies, count=12))
  for level in [0, 1, 2]:
    result.append(scene(f'radar-setting-{level}', big, settings={'ShowRadarInfo': str(level)}, count=4))

  def empty_and_long(i, bodies, step, invalid, omit):
    bodies['carParams']['openpilotLongitudinalControl'] = i >= 4
    if i == 0:
      omit.update(SERVICES)
    elif i in (1, 2, 6):
      bodies['modelV2']['position'] = xyz([], [], [])
    if i >= 7:
      bodies['liveCalibration']['height'] = []
    if i == 8:
      bodies['radarState']['leadOne']['status'] = False
      bodies['radarState']['leadTwo']['status'] = False

  result.append(scene('empty-start-longitudinal-recovery', big, count=10, mutate=empty_and_long))

  def shape_boundary(i, bodies, step, invalid, omit):
    length = [1, 2, 4, 8, 16, 33][i % 6]
    for value in [bodies['modelV2']['position'], *bodies['modelV2']['laneLines'], *bodies['modelV2']['roadEdges']]:
      for axis in ['x', 'y', 'z']:
        value[axis] = value[axis][:length]
    if i in (3, 7):
      bodies['modelV2']['position']['x'][1] = bodies['modelV2']['position']['x'][0]

  result.append(scene('short-model-arrays', big, count=12, mutate=shape_boundary))
  return result
