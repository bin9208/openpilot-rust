"""Branch and boundary inputs for the unchanged-source controlsd loop oracle."""

import random

from openpilot.cereal import car, log
from controlsd_fixture import SETTINGS, frame, parameters
from paramsd_fixture import event


def change(row, updates=None, omit=(), settings=None):
  updates = updates or {}
  packets = []
  for packet in row['messages']:
    with log.Event.from_bytes(bytes(packet)) as message:
      name = message.which()
      if name in omit:
        continue
      if name in updates:
        data = message.to_dict()[name]
        data.update(updates[name])
        packet = event(name, row['time'], data)
      packets.append(packet)
  row['messages'] = packets
  if settings:
    row['params'] = {key: list(str(value).encode()) for key, value in settings.items()}
  return row


def case(name, fingerprint, mode, frames, settings=None, cp_changes=None):
  settings = SETTINGS | (settings or {})
  params = {key: list(value.encode()) for key, value in settings.items()}
  cp = bytes(parameters(fingerprint, mode, settings))
  with car.CarParams.from_bytes(cp) as reader:
    builder = reader.as_builder()
    for key, value in (cp_changes or {}).items():
      if key == 'useSteeringAngle':
        builder.lateralTuning.torque.useSteeringAngle = value
      else:
        setattr(builder, key, value)
    params['CarParams'] = list(builder.to_bytes())
  return {'name': name, 'params': params, 'simulation': False, 'frames': frames}


def cases():
  result = []
  for fingerprint, mode in [
    ('COMMA_BODY', 'pid'),
    ('CHRYSLER_PACIFICA_2018', 'pid'),
    ('FORD_ESCAPE_MK4', 'angle'),
    ('MAZDA_3', 'pid'),
    ('MOCK', 'pid'),
    ('NISSAN_LEAF', 'angle'),
    ('RIVIAN_R1_GEN1', 'torque'),
    ('SUBARU_ASCENT', 'pid'),
    ('TESLA_MODEL_3', 'angle'),
  ]:
    result.append(case('family-' + fingerprint, fingerprint, mode, [frame(i) for i in range(40)]))
  for fingerprint, mode in [
    ('HONDA_CIVIC', 'pid'),
    ('TOYOTA_RAV4', 'torque'),
    ('HYUNDAI_PALISADE', 'torque'),
    ('VOLKSWAGEN_ID4_MK1', 'angle'),
    ('TESLA_MODEL_3', 'angle'),
  ]:
    rows = []
    for i in range(620):
      phase = i // 20
      speed = [0.0, 0.3, 0.300001, 1.0, 4.999, 5.0, 5.001, 10.0, 10.001, -1.0, 30.0][phase % 11]
      values = {
        'carState': {
          'vEgo': speed,
          'standstill': i < 40,
          'brakePressed': 60 <= i < 80,
          'gasPressed': 100 <= i < 120,
          'softHoldActive': int(160 <= i < 190),
          'steeringPressed': 220 <= i < 340,
          'steeringAngleDeg': 46.0 if 220 <= i < 340 else 0.0,
          'steerFaultTemporary': 420 <= i < 430,
          'steerFaultPermanent': 430 <= i < 440,
          'latEnabled': not 440 <= i < 460,
          'gearShifter': ['neutral', 'park', 'reverse', 'unknown', 'drive'][(i // 100) % 5] if i < 200 else 'drive',
          'cruiseState': {'enabled': True, 'standstill': 120 <= i < 140},
        },
        'longitudinalPlan': {'shouldStop': 120 <= i < 210, 'aTarget': -0.7 if 120 <= i < 200 else 0.2},
        'selfdriveState': {'enabled': i >= 3 and i < 500, 'active': i >= 3 and i < 500, 'state': 'softDisabling' if 400 <= i < 420 else 'enabled'},
        'modelV2': {'jetlink': {'lossLatched': 380 <= i < 390, 'source': 'jetlink' if 390 <= i < 400 else 'native'}},
      }
      settings = {
        'StoppingAccel': ['-200', '-50', 'nan', '-70'][phase % 4],
        'SteerRatioRate': [29, 30, 100, 200, 201][phase % 5],
        'CustomSR': 0 if phase % 3 else 150,
        'AlwaysLateral': int(i < 560),
        'DisableDM': int(phase % 2),
        'SpeedFromPCM': phase % 4,
      }
      rows.append(change(frame(i), values, settings=settings))
    result.append(case('transitions-' + fingerprint, fingerprint, mode, rows, cp_changes={'startingState': True, 'steerAtStandstill': True}))
  for fingerprint, settings in [('HYUNDAI_PALISADE', {'NNFF': '1'}), ('GMC_ACADIA', {'NNFFLite': '1'}), ('CHEVROLET_VOLT', {})]:
    rng = random.Random(150)
    rows = []
    for i in range(1000):
      acceleration = [rng.uniform(-3.0, 3.0) for _ in range(33)]
      values = {
        'carState': {'steeringPressed': i % 60 < 10, 'steeringAngleDeg': rng.uniform(-3.0, 3.0), 'aEgo': rng.uniform(-1.0, 1.0)},
        'modelV2': {
          'orientation': {'x': [rng.uniform(-0.1, 0.1) for _ in range(33)], 'y': [0.03] * 33},
          'acceleration': {'y': acceleration},
          'action': {'desiredCurvature': rng.uniform(-0.015, 0.015)},
        },
      }
      custom = {100: 1, 200: 0, 300: 2, 400: 0}.get(i)
      params = (
        None
        if custom is None
        else {
          'LateralTorqueCustom': custom,
          'LateralTorqueAccelFactor': 2800,
          'LateralTorqueFriction': 130,
          'LateralTorqueKpV': 80,
          'LateralTorqueKiV': 12,
          'LateralTorqueKf': 100,
          'LateralTorqueKd': 1,
        }
      )
      rows.append(change(frame(i), values, settings=params))
    result.append(case('neural-history-' + fingerprint, fingerprint, 'torque', rows, settings, {'useSteeringAngle': False}))
  rows = []
  for i in range(420):
    phase = i // 20
    values = {
      'carState': {
        'vEgo': 20.5,
        'vCruise': 80.0,
        'brakePressed': phase == 4,
        'gasPressed': phase == 5,
        'carrotCruise': int(phase == 6),
        'cruiseState': {'standstill': phase == 7},
      },
      'longitudinalPlan': {
        'aTarget': -0.7,
        'vTargetNow': 20.0,
        'cruiseCoastingTarget': 20.0,
        'cruiseCoastingPercent': 5,
        'longitudinalPlanSource': 'lead0' if phase == 8 else 'cruise',
        'fcw': phase == 9,
        'cruiseTarget': 79.0 if phase == 10 else 80.0,
      },
      'radarState': {'leadOne': {'status': phase == 11}, 'leadTwo': {'status': phase == 12}, 'leadCutInRisk': {'status': phase == 13}},
    }
    rows.append(change(frame(i), values, omit=['longitudinalPlan'] if phase == 14 else []))
  result.append(case('coasting-all-vetoes', 'HONDA_CIVIC', 'pid', rows))
  rows = []
  for i in range(260):
    phase = i // 20
    values = {
      'carrotMan': {'desiredSpeed': [0.0, -1.0, 250.0, 251.0, 60.0][phase % 5]},
      'lateralPlan': {'useLaneLines': True, 'psis': [0.001 * j for j in range(17)], 'curvatures': [0.001] * 17, 'distances': [j * 0.5 for j in range(17)]},
      'modelV2': {
        'meta': {
          'laneChangeState': 'laneChangeStarting',
          'laneChangeDirection': 'left' if phase % 2 else 'right',
          'desireState': [0.0, 0.2 if phase % 4 == 0 else 0.0, 0.2 if phase % 4 == 1 else 0.0, 0.2 if phase % 4 == 2 else 0.0, 0.2 if phase % 4 == 3 else 0.0],
        }
      },
    }
    rows.append(change(frame(i), values, omit=['carrotMan'] if i > 60 else [], settings={'UseLaneLineCurveSpeed': 0, 'LatSmoothSec': 0 if phase % 2 else 30}))
  result.append(case('lanes-carrot-stale', 'HONDA_CIVIC', 'pid', rows))
  rows = []
  navigation = [
    {'vTurnSpeed': 20.0, 'xSpdType': 1, 'xSpdLimit': 40},
    {'vTurnSpeed': -20.0, 'xSpdType': 22, 'xSpdLimit': 30},
    {'vTurnSpeed': 130.0, 'atcType': 'turn left'},
    {'vTurnSpeed': 130.0, 'atcType': 'fork right', 'nRoadLimitSpeed': 60},
    {'vTurnSpeed': 130.0, 'xTurnInfo': 5, 'xDistToTurn': 100},
    {'vTurnSpeed': 130.0, 'szSdiDescr': 'Bottleneck point'},
    {'vTurnSpeed': 130.0, 'desiredSource': 'road', 'desiredSpeed': 60, 'nRoadLimitSpeed': 80},
    {'vTurnSpeed': 130.0, 'atcType': 'prepare turn left'},
  ]
  for i in range(160):
    rows.append(change(frame(i), {'carrotMan': navigation[(i // 20) % len(navigation)]}, settings={'SpeedFromPCM': (i // 10) % 4}))
  result.append(case('meb-navigation-events', 'VOLKSWAGEN_ID4_MK1', 'angle', rows))
  rows = [change(frame(i), {'longitudinalPlan': {'jTargetNow': value}}) for i, value in enumerate([float('nan'), float('inf'), -float('inf'), 0.0, 0.1])]
  result.append(case('nonfinite-actuator-sanitation', 'HONDA_CIVIC', 'pid', rows))
  result.append(case('zero-neural-jerk-time', 'GMC_ACADIA', 'torque', [frame(i) for i in range(30)], {'NNFFLite': '1', 'SteerActuatorDelay': '-30'}))
  for camera in ('0', '1'):
    result.append(case('hyundai-hda2-bus-' + camera, 'HYUNDAI_IONIQ_5', 'torque', [frame(i) for i in range(35)], {'HyundaiCameraSCC': camera}, {'flags': 8193}))
  return result
