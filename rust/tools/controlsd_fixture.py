import math

from openpilot.cereal import car, log
from controlsd_parameters import Store
from controlsd_source import load
from paramsd_fixture import event, pose

SETTINGS = {
  'FingerPrints': '{0: {}, 1: {}, 2: {}, 3: {}, 4: {}, 5: {}, 6: {}, 7: {}}',
  'SteerRatioRate': '100',
  'CustomSR': '0',
  'AlwaysLateral': '1',
  'LatSuspendAngleDeg': '45',
  'UseLaneLineCurveSpeed': '0',
  'LatSmoothSec': '15',
  'SteerActuatorDelay': '20',
  'StoppingAccel': '-50',
  'LongTuningKpV': '100',
  'LongTuningKiV': '100',
  'LongTuningKf': '100',
  'DisableDM': '0',
  'SpeedFromPCM': '0',
  'AutoTurnControlSpeedTurn': '35',
  'NNFF': '0',
  'NNFFLite': '0',
  'AutoEngage': '1',
}


def parameters(fingerprint, mode, settings):
  source, _, _, _ = load(Store({key: value.encode() for key, value in settings.items()}))
  if fingerprint == 'PSA_PEUGEOT_208':
    from paramsd_fixture import car_params

    with car.CarParams.from_bytes(bytes(car_params(fingerprint=fingerprint))) as reader:
      cp = reader.as_builder()
    cp.brand = 'psa'
  else:
    cp = source['interfaces'][fingerprint].get_non_essential_params(fingerprint)
  cp.openpilotLongitudinalControl = True
  cp.minSteerSpeed = 0.0
  if mode == 'pid':
    cp.steerControlType = 'torque'
    tune = cp.lateralTuning.init('pid')
    tune.kpBP, tune.kpV = [0.0, 20.0], [0.1, 0.2]
    tune.kiBP, tune.kiV = [0.0, 20.0], [0.02, 0.04]
    tune.kf = 0.0001
  elif mode == 'torque':
    cp.steerControlType = 'torque'
    tune = cp.lateralTuning.init('torque')
    tune.kp, tune.ki, tune.kf = 1.0, 0.1, 1.0
    tune.latAccelFactor, tune.latAccelOffset, tune.friction = 2.5, 0.0, 0.1
    tune.useSteeringAngle = True
  elif mode == 'angle':
    cp.steerControlType = 'angle'
  return list(cp.to_bytes())


def frame(index):
  time = 100.0 + index * 0.01
  active = index >= 3
  speed = 15.0 + 3.0 * math.sin(index * 0.01)
  messages = [
    event(
      'carState',
      time,
      {
        'vEgo': speed,
        'aEgo': 0.1,
        'steeringAngleDeg': math.sin(index * 0.03),
        'steeringRateDeg': 0.2,
        'canValid': True,
        'gearShifter': 'drive',
        'latEnabled': True,
        'vCruise': 80.0,
        'vCruiseCluster': 80.0,
        'vCluRatio': 1.02,
        'cruiseState': {'enabled': True, 'standstill': False},
      },
    ),
    event('liveParameters', time, {'stiffnessFactor': 1.0, 'steerRatio': 15.0, 'angleOffsetDeg': 0.2, 'roll': 0.01}),
    event(
      'liveTorqueParameters', time, {'useParams': True, 'latAccelFactorFiltered': 2.6, 'latAccelOffsetFiltered': 0.01, 'frictionCoefficientFiltered': 0.12}
    ),
    event('liveDelay', time, {'lateralDelay': 0.2}),
    event(
      'modelV2',
      time,
      {
        'action': {'desiredCurvature': 0.001 * math.sin(index * 0.02)},
        'orientation': {'x': [0.01] * 33, 'y': [0.02] * 33},
        'acceleration': {'y': [0.1 + j * 0.003 for j in range(33)]},
        'meta': {'desireState': [0.0] * 8},
      },
    ),
    event(
      'longitudinalPlan',
      time,
      {'aTarget': 0.2 * math.sin(index * 0.01), 'vTargetNow': speed + 0.2, 'jTargetNow': 0.05, 'speeds': [20.0] * 17, 'hasLead': True, 'cruiseTarget': 80.0},
    ),
    event('lateralPlan', time, {'useLaneLines': False, 'psis': [0.0] * 17, 'curvatures': [0.0] * 17, 'distances': [float(j) for j in range(17)]}),
    event('radarState', time, {'leadOne': {'status': True, 'dRel': 25.0, 'vRel': -0.1, 'radar': True, 'dPath': 0.1}}),
    event('carOutput', time, {'actuatorsOutput': {'torque': 0.0, 'curvature': 0.0, 'steeringAngleDeg': 0.0}}),
    event('carrotMan', time, {'desiredSpeed': 70.0, 'vTurnSpeed': 130.0, 'activeCarrot': 2}),
    event('driverMonitoringState', time, {'alertLevel': 'none'}),
    event('driverAssistance', time, {'leftLaneDeparture': False, 'rightLaneDeparture': True}),
    pose(time),
    event('liveCalibration', time, {'rpyCalib': [0.01, -0.02, 0.03], 'calStatus': 'calibrated'}),
    event('selfdriveState', time, {'enabled': active, 'active': active, 'state': 'enabled' if active else 'disabled', 'personality': 'standard'}),
  ]
  events = log.Event.new_message(logMonoTime=int(time * 1e9), valid=True)
  events.init('onroadEvents', 0)
  messages.insert(0, list(events.to_bytes()))
  return {'time': time, 'messages': messages}


def cases():
  result = []
  for fingerprint, mode in [
    ('HONDA_CIVIC', 'pid'),
    ('TOYOTA_RAV4', 'torque'),
    ('VOLKSWAGEN_ID4_MK1', 'angle'),
    ('HYUNDAI_PALISADE', 'torque'),
    ('GMC_ACADIA', 'torque'),
    ('CHEVROLET_VOLT', 'torque'),
  ]:
    settings = SETTINGS | ({'NNFF': '1'} if fingerprint == 'HYUNDAI_PALISADE' else {'NNFFLite': '1'} if fingerprint == 'GMC_ACADIA' else {})
    params = {key: list(value.encode()) for key, value in settings.items()}
    params['CarParams'] = parameters(fingerprint, mode, settings)
    result.append({'name': fingerprint + '-' + mode, 'params': params, 'simulation': False, 'frames': [frame(index) for index in range(200)]})
  return result
