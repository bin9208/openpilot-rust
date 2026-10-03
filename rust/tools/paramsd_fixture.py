import math
import struct

from openpilot.cereal import car, log


def car_params(index=0, fingerprint=None):
  profiles = [(1600., 2700., 2.7, 1.1, 80000., 90000., 15.),
              (2450., 4800., 3.05, 1.35, 125000., 140000., 18.7),
              (3100., 6200., 3.2, 1.4, 165000., 155000., 12.5)]
  mass, inertia, wheelbase, front, tire_front, tire_rear, ratio = profiles[index]
  cp = car.CarParams.new_message(carFingerprint=fingerprint or f'SYNTHETIC_{index}', mass=mass, rotationalInertia=inertia,
                                wheelbase=wheelbase, centerToFront=front, tireStiffnessFront=tire_front, tireStiffnessRear=tire_rear, steerRatio=ratio)
  return list(cp.to_bytes())


def event(service, time, data, valid=True):
  message = log.Event.new_message(logMonoTime=int(time * 1e9), valid=valid)
  message.init(service)
  getattr(message, service).from_dict(data)
  return list(message.to_bytes())


def xyz(values, stds, valid=True):
  return dict(zip(('x', 'y', 'z', 'xStd', 'yStd', 'zStd', 'valid'), [*values, *stds, valid], strict=True))


def pose(time, yaw=.03, yaw_std=.03, yaw_valid=True, roll=.02, roll_std=.01, sensors=True, posenet=True, valid=True):
  return event('livePose', time, {'timestamp': int(time * 1e9), 'sensorsOK': sensors, 'posenetOK': posenet,
    'orientationNED': xyz([roll, .03, -.04], [roll_std, .01, .02]),
    'velocityDevice': xyz([12., .1, -.2], [.1, .2, .3]), 'accelerationDevice': xyz([.3, -.2, .1], [.2, .3, .4]),
    'angularVelocityDevice': xyz([-.01, .02, yaw], [.01, .02, yaw_std], yaw_valid)}, valid)


def bits(values):
  return [struct.unpack('<Q', struct.pack('<d', value))[0] for value in values]


def state(values=None, covariance=None, time=500.):
  return {'state': bits(values or [1., 15., 0., 0., 10., 0., 0., 0., 0.]),
          'covariance': bits(covariance or [.001] * 9), 'time': time}


def scenarios():
  cases = []
  for profile in range(3):
    operations = [{'packet': event('liveCalibration', 99., {'rpyCalib': [.02, -.03, .04], 'calStatus': 'calibrated'})}]
    for i in range(160):
      time = 100. + i * .05
      operations += [{'packet': event('carState', time, {'vEgo': 18. + math.sin(i * .1), 'steeringAngleDeg': 4. * math.sin(i * .07)})},
                     {'packet': pose(time + .001, yaw=.03 * math.sin(i * .07), roll=.015 * math.sin(i * .03))}]
    cases.append({'name': f'physical-model-{profile}', 'car': car_params(profile), 'operations': operations})
  boundaries = []
  for i, (speed, steering) in enumerate([(0., 0.), (1., 0.), (1.01, 0.), (20., 44.99), (20., 45.), (20., -45.), (-2., 0.), (10., 0.), (10.01, 0.)]):
    time = 200. + i * .1
    boundaries += [{'packet': event('carState', time, {'vEgo': speed, 'steeringAngleDeg': steering})}, {'packet': pose(time + .01)}]
  for i, options in enumerate([{'yaw_valid': False}, {'yaw_std': 0.}, {'yaw_std': 10.}, {'yaw': 1.}, {'yaw': float('nan')},
                                {'roll_std': float('nan')}, {'roll_std': math.radians(1.5)}, {'roll': math.radians(10.)},
                                {'sensors': False}, {'posenet': False}, {'yaw_std': -.04}]):
    boundaries.append({'packet': pose(202. + i * .05, **options)})
  cases.append({'name': 'active-fallback-and-thresholds', 'car': car_params(), 'operations': boundaries})
  operations = []
  for angle, count in [(11., 14), (9., 4), (7.5, 4), (-11., 24), (-9., 4), (-7.5, 4)]:
    values = [1., 15., math.radians(angle), 0., 10., 0., 0., 0., math.radians(angle)]
    operations.append(state(values))
    operations += [{} for _ in range(count)]
  for stiffness, ratio in [(0.2 - 1e-10, 30. + 1e-8), (0.199, 30.01), (5. + 1e-8, 7.5 - 1e-8), (5.01, 7.49)]:
    operations.append(state([stiffness, ratio, 0., 0., 10., 0., 0., 0., 0.]))
  cases.append({'name': 'clipping-hysteresis-and-float32-validity', 'car': car_params(), 'operations': operations})
  cases.append({'name': 'nonfinite-state-keeps-prereset-std', 'car': car_params(), 'operations': [
    state([1., 15., float('nan'), 0., 10., 0., 0., 0., 0.], [2., 3., 4., 5., 6., 7., 8., 9., 10.]), {},
    state(covariance=[.001] * 8 + [-1.]), {},
  ]})
  rewind = [{'packet': event('carState', 600., {'vEgo': 20., 'steeringAngleDeg': 2.})}]
  rewind += [{'packet': pose(600.05 + i * .01)} for i in range(130)]
  rewind += [{'packet': pose(600.5)}, {'packet': pose(598.)},
             {'packet': event('carState', 603., {'vEgo': 0., 'steeringAngleDeg': 0.})},
             {'packet': pose(700.)}, {'packet': event('carState', 700.1, {'vEgo': 20., 'steeringAngleDeg': 1.})}]
  cases.append({'name': 'rewind-eviction-and-inactive-clock-reset', 'car': car_params(), 'operations': rewind})
  return cases
