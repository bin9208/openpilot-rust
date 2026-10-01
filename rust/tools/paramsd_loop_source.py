import json
import sys
import types

from openpilot.cereal import car
from check_message_state import source as messaging_source
from locationd_loop_source import Finished, reader
from paramsd_source import load


def trace(directory, case):
  source, logs = load(directory)
  messaging, environment = messaging_source()
  environment['simulation'] = str(int(case['simulation']))
  operations, rows, published, writes = [], [], [], []
  values = {key: bytes(value) for key, value in case['seed'].items()}

  class Store:
    def get(self, key, block=False):
      if key == 'CarParams':
        return bytes(case['car'])
      operations.append(['get', key, None])
      value = values.get(key) or None
      if key == 'LiveParameters' and value is not None:
        try:
          return json.loads(value)
        except (ValueError, UnicodeError):
          logs.append(['warning', 'Failed to cast param LiveParameters from JSON'])
          return None
      return value

    def put(self, key, data):
      operations.append(['put', key, list(data)])
      values[key] = data

    def remove(self, key):
      operations.append(['remove', key, None])
      values.pop(key, None)

    def put_nonblocking(self, key, data):
      writes.append([key, list(data.encode() if isinstance(data, str) else data)])

  store = Store()
  with car.CarParams.from_bytes(bytes(case['car'])) as cp:
    source['migrate_cached_vehicle_params_if_needed'](store)
    ratio, stiffness, offset, covariance = source['retrieve_initial_vehicle_params'](store, cp, case['replay'], case['debug'])
  initial = {'initial': [ratio, stiffness, offset], 'covariance': None if covariance is None else covariance.flatten().tolist(),
             'operations': list(operations), 'logs': list(logs)}
  if not case['frames']:
    return {'name': case['name'], 'init': initial, 'rows': rows}
  values = {key: bytes(value) for key, value in case['seed'].items()}
  logs.clear()
  current = {'index': -1}

  def capture(local):
    learner = local['learner']
    rows.append({'event': published[0] if published else None, 'cache': any(key == 'LiveParametersV2' for key, _ in writes),
                 'gps': next((data for key, data in writes if key == 'LastGPSPosition'), None),
                 'x': learner.kf.x.tolist(), 'p': learner.kf.P.flatten().tolist(), 'logs': list(logs)})
    assert len(published) <= 1
    if rows[-1]['cache']:
      assert next(data for key, data in writes if key == 'LiveParametersV2') == published_bytes[0]
    logs.clear()
    published.clear()
    published_bytes.clear()
    writes.clear()

  class SubMaster(messaging['SubMaster']):
    def update(self):
      if current['index'] >= 0:
        capture(sys._getframe(1).f_locals)
      else:
        logs.clear()
      current['index'] += 1
      if current['index'] == len(case['frames']):
        raise Finished()
      frame = case['frames'][current['index']]
      self.update_msgs(frame['time'], [reader(packet) for packet in frame['messages']])

  published_bytes = []

  def publish(service, packet):
    assert service == 'liveParameters'
    published_bytes.append(list(packet))
    published.append(reader(packet).to_dict())

  scheduling = []
  source.update(config_realtime_process=lambda cores, priority: scheduling.append([cores, priority]),
                os=types.SimpleNamespace(getenv=lambda key, default: {'DEBUG': str(int(case['debug'])), 'REPLAY': str(int(case['replay']))}.get(key, default)),
                Params=lambda *args: store, get_gps_location_service=lambda params: case['gps'])
  source['messaging'].SubMaster = SubMaster
  source['messaging'].PubMaster = lambda names: types.SimpleNamespace(send=publish)
  source['messaging'].log_from_bytes = lambda data, schema: schema.from_bytes(data).__enter__()
  try:
    source['main']()
  except Finished:
    pass
  assert scheduling == [[[0, 1, 2, 3], 5]]
  return {'name': case['name'], 'init': initial, 'rows': rows}
