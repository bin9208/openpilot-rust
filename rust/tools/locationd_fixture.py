from openpilot.cereal import log


def packet(service, time, data, valid=True):
  event = log.Event.new_message()
  event.logMonoTime = int(time * 1e9)
  event.valid = valid
  event.init(service)
  getattr(event, service).from_dict(data)
  return list(event.to_bytes())


def sensor(service, time, values, offset=0.0, source='lsm6ds3', valid=True):
  kind = 'acceleration' if service == 'accelerometer' else 'gyroUncalibrated'
  return packet(service, time, {'timestamp': int((time + offset) * 1e9), 'source': source, kind: {'v': values}}, valid)


def camera(time, rot=None, trans=None, rot_std=None, trans_std=None, delay=0.1, valid=True):
  return packet(
    'cameraOdometry',
    time,
    {
      'timestampEof': int((time + 0.1 - delay) * 1e9),
      'rot': rot if rot is not None else [0.01, -0.02, 0.03],
      'trans': trans if trans is not None else [12.0, 0.2, -0.1],
      'rotStd': rot_std if rot_std is not None else [0.01, 0.02, 0.03],
      'transStd': trans_std if trans_std is not None else [0.1, 0.2, 0.3],
    },
    valid,
  )


def scenarios():
  normal = []
  for i in range(180):
    t = 100 + i * 0.01
    normal.extend([sensor('accelerometer', t, [9.8, 0.05, -0.02]), sensor('gyroscope', t + 0.001, [-0.03, 0.02, -0.01])])
    if i % 5 == 0:
      normal.extend([packet('carState', t + 0.002, {'vEgo': 12.0}), camera(t + 0.003)])
    if i % 25 == 0:
      normal.append(packet('liveCalibration', t + 0.004, {'rpyCalib': [0.03, -0.02, 0.04]}))
  cases = [{'name': 'ordered-and-delayed-camera', 'events': normal}]
  invalid = [sensor('accelerometer', 100, [9.81, 0.0, 0.0]), sensor('gyroscope', 100.01, [0.0, 0.0, 0.0])]
  for t, service, values, offset, source in [
    (100.1, 'accelerometer', [100.0, 0.0, 0.0], 0.0, 'lsm6ds3'),
    (100.11, 'accelerometer', [99.9, 0.0, 0.0], 0.0, 'bmx055'),
    (100.12, 'accelerometer', [9.81, 0.0, 0.0], -0.1001, 'lsm6ds3'),
    (100.13, 'gyroscope', [0.0, 0.0, 0.0], -0.1001, 'lsm6ds3'),
    (100.14, 'gyroscope', [10.0, 0.0, 0.0], 0.0, 'lsm6ds3'),
    (100.15, 'gyroscope', [0.0, 0.0, 0.0], 0.0, 'bmx055'),
    (100.16, 'accelerometer', [9.81, 0.0, 0.0], -100.16, 'lsm6ds3'),
    (101.0, 'accelerometer', [9.81, 0.0, 0.0], 0.0, 'lsm6ds3'),
    (100.1, 'accelerometer', [9.81, 0.0, 0.0], 0.0, 'lsm6ds3'),
  ]:
    invalid.append(sensor(service, t, values, offset, source))
  invalid.extend(
    [
      packet('liveCalibration', 101.1, {'rpyCalib': []}),
      packet('liveCalibration', 101.2, {'rpyCalib': [-0.5, 0.5, 0.0]}),
      packet('liveCalibration', 101.3, {'rpyCalib': [-0.5001, 0.0, 0.0]}),
      packet('liveCalibration', 101.4, {'rpyCalib': [0.0, 0.5001, 0.0]}),
      camera(101.5, rot=[10.1, 0.0, 0.0]),
      camera(101.6, trans=[201.0, 0.0, 0.0]),
      camera(101.7, rot_std=[1e-5, 0.1, 0.1]),
      camera(101.8, trans_std=[0.1, 0.0, 0.1]),
      camera(101.9, rot_std=[101.0, 0.1, 0.1]),
      camera(102.0, trans_std=[2001.0, 0.1, 0.1]),
      camera(102.1, delay=2.0),
    ]
  )
  cases.append({'name': 'timing-source-sanity-boundaries', 'events': invalid})
  cases.append(
    {
      'name': 'sensor-prefix-and-unknown-source',
      'events': [
        sensor('accelerometer', 150.0, [9.81, 0.0, 0.0, 987.0]),
        sensor('gyroscope', 150.01, [0.0, 0.0, 0.0, 987.0]),
        sensor('accelerometer', 150.02, [9.81, 0.0, 0.0], source=42),
      ],
    }
  )
  cases.append(
    {
      'name': 'nonfinite-acceleration-reset',
      'events': [
        sensor('accelerometer', 200, [9.81, 0.0, 0.0]),
        sensor('accelerometer', 200.01, [float('nan'), 0.0, 0.0]),
        sensor('accelerometer', 200.02, [9.81, 0.0, 0.0]),
        sensor('gyroscope', 200.03, [float('inf'), 0.0, 0.0]),
      ],
    }
  )
  cases.append(
    {
      'name': 'low-speed-yawrate-floor',
      'events': [
        camera(300.0, rot=[0.0, 0.0, 0.0], rot_std=[0.0001] * 3),
        sensor('gyroscope', 300.01, [-0.3, 0.0, 0.0]),
        packet('carState', 300.02, {'vEgo': -5.0}),
        sensor('gyroscope', 300.03, [-0.3, 0.0, 0.0]),
        packet('carState', 300.04, {'vEgo': 4.99}),
        sensor('gyroscope', 300.05, [-0.61, 0.0, 0.0]),
      ],
    }
  )
  spike = [packet('carState', 400.0, {'vEgo': 5.1})]
  spike += [camera(400.1 + i * 0.05, trans_std=[0.1, 0.1, 0.1]) for i in range(40)]
  spike += [camera(402.1 + i * 0.05, trans_std=[8.0, 0.1, 0.1]) for i in range(20)]
  spike.append(packet('carState', 403.2, {'vEgo': 5.0}))
  cases.append({'name': 'posenet-spike-speed-boundary', 'events': spike})
  old = [sensor('accelerometer', 500.0 + i * 0.001, [9.81, 0.02, 0.0]) for i in range(530)]
  old += [sensor('accelerometer', 500.005, [9.81, 0.0, 0.0]), sensor('gyroscope', 500.2, [0.0, 0.0, 0.0])]
  cases.append({'name': 'rewind-capacity-and-fast-forward', 'events': old})
  cases.append(
    {
      'name': 'reset-without-rewind-history',
      'reset_time': 600.0,
      'events': [
        sensor('accelerometer', 599.5, [9.81, 0.0, 0.0]),
        sensor('accelerometer', 599.2, [9.81, 0.0, 0.0]),
        sensor('accelerometer', 599.199, [9.81, 0.0, 0.0]),
        sensor('accelerometer', 600.1, [9.81, 0.0, 0.0]),
      ],
    }
  )
  return cases
