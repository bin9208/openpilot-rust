from locationd_fixture import camera, packet, sensor


def cases():
  result = []
  for simulation in (False, True):
    frames = []
    for i in range(330):
      now = 100.0 + i * 0.05
      messages = []
      if i not in (120, 121, 122):
        messages.append(packet('carState', now, {'vEgo': 12.0}, valid=i not in (90, 91)))
      if i % 5 == 0:
        messages.append(packet('liveCalibration', now, {'rpyCalib': [0.03, -0.02, 0.04]}))
      if i not in (130, 131):
        messages.append(camera(now, trans_std=[0.0, 0.1, 0.1] if i in (50, 51) else None))
      acceleration, gyroscope = [], []
      for j in range(5):
        t = now - 0.04 + j * 0.01
        acceleration.append(
          sensor('accelerometer', t, [100.0, 0.0, 0.0] if i in (30, 31) else [9.81, 0.02, 0.0], source='bmx055' if i in (32, 33) else 'lsm6ds3', valid=i != 100)
        )
        if i not in (70, 71, 72, 73):
          gyroscope.append(sensor('gyroscope', t, [-0.03, 0.01, -0.02], offset=-0.15 if i in (40, 41) else 0.0))
      frames.append({'time': now, 'messages': messages, 'acceleration': acceleration, 'gyroscope': gyroscope})
    result.append({'name': 'simulation' if simulation else 'real-clock', 'simulation': simulation, 'seed': None, 'frames': frames})
  seed = packet('livePose', 0.0, {'debugFilterState': {'value': [i * 0.001 for i in range(18)], 'std': [0.2 + i * 0.01 for i in range(18)]}})
  result.append({'name': 'persisted-covariance-is-not-squared', 'simulation': True, 'seed': seed, 'frames': result[1]['frames'][:8]})
  empty = packet('livePose', 0.0, {'debugFilterState': {'value': [], 'std': []}})
  result.append({'name': 'persisted-empty-defaults', 'simulation': True, 'seed': empty, 'frames': result[1]['frames'][:8]})
  return result
