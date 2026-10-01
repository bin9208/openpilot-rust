import json

from paramsd_fixture import car_params, event, pose


def cases():
  base = {'car': car_params(), 'seed': {}, 'replay': False, 'debug': True, 'gps': 'gpsLocationExternal', 'simulation': False, 'frames': []}
  result = []
  old = {'steerRatio': 17., 'stiffnessFactor': .7, 'angleOffsetAverageDeg': 2.5}
  variants = [None, b'', b'null', b'{', b'[]', b'42', b'{}', b'\xff', json.dumps(old).encode(),
              json.dumps(old | {'steerRatio': '17'}).encode(), json.dumps(old | {'steerRatio': True}).encode(),
              json.dumps(old | {'steerRatio': float('nan')}).encode(), json.dumps(old | {'stiffnessFactor': float('inf')}).encode(),
              json.dumps(old | {'angleOffsetAverageDeg': -float('inf')}).encode(),
              json.dumps(old | {'steerRatio': 1e300}).encode()]
  variants += [json.dumps(old).encode(encoding) for encoding in ('utf-8-sig', 'utf-16', 'utf-16-le', 'utf-16-be', 'utf-32', 'utf-32-le', 'utf-32-be')]
  for index, value in enumerate(variants):
    seed = {'CarParamsPrevRoute': car_params()}
    if value is not None:
      seed['LiveParameters'] = list(value)
    result.append(base | {'name': f'migration-{index}', 'seed': seed})
  for index, (ratio, previous, std, replay, debug) in enumerate([
    (15., car_params(), [0.1 * (i + 1) for i in range(9)], True, True),
    (15., car_params(), [.3] * 9, False, True), (15., car_params(), [.3] * 9, True, False),
    (7.5, car_params(), [], True, True), (30., car_params(), [], True, True),
    (7.49, car_params(), [], True, True), (30.01, car_params(), [], True, True),
    (float('nan'), car_params(), [], True, True), (15., car_params(1), [], True, True),
    (15., [1, 2], [], True, True), (15., None, [], True, True),
    (15., car_params(), [.1, .2], True, True),
  ]):
    seed = {'LiveParametersV2': event('liveParameters', 20., {'steerRatio': ratio, 'stiffnessFactor': .6,
             'angleOffsetAverageDeg': 1.5, 'valid': False, 'debugFilterState': {'std': std}})}
    if previous is not None:
      seed['CarParamsPrevRoute'] = previous
    result.append(base | {'name': f'cache-{index}', 'seed': seed, 'replay': replay, 'debug': debug})
  result += [base | {'name': 'malformed-v2', 'seed': {'LiveParametersV2': [1, 2], 'CarParamsPrevRoute': car_params()}},
             base | {'name': 'wrong-union', 'seed': {'LiveParametersV2': pose(2.), 'CarParamsPrevRoute': car_params()}},
             base | {'name': 'v2-suppresses-migration', 'seed': {'LiveParameters': list(b'{'), 'LiveParametersV2': [1, 2]}}]
  for gps in ['gpsLocation', 'gpsLocationExternal']:
    frames = []
    for i in range(1210):
      time = 100. + i * .05
      messages = [event('carState', time, {'vEgo': 0. if i < 12 else 18., 'steeringAngleDeg': 2.}, valid=i != 50)]
      if i % 10 == 0:
        messages.append(event('liveCalibration', time, {'rpyCalib': [.01, .02, -.01], 'calStatus': 'calibrated'}))
      if i not in [30, 31]:
        messages.append(pose(time, valid=i != 70))
      if i % 11 == 0:
        messages.append(event(gps, time, {'hasFix': i != 22, 'latitude': 37.5, 'longitude': 127., 'bearingDeg': 22.5}, valid=False))
      if 80 <= i < 90:
        messages = [packet for packet in messages[-1:] if i % 2]
      frames.append({'time': time, 'messages': list(reversed(messages))})
    result.append(base | {'name': f'loop-{gps}', 'gps': gps, 'frames': frames})
  return result
