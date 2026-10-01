from contextlib import redirect_stdout
import io
import sys
import types

from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from check_message_state import source as messaging_source
from locationd_source import load


class Finished(Exception):
  pass


def reader(packet):
  with log.Event.from_bytes(bytes(packet)) as event:
    return event.as_builder().as_reader()


def trace(directory, case):
  source, logs = load(directory)
  messaging, environment = messaging_source()
  environment['simulation'] = str(int(case['simulation']))
  current = {'index': -1, 'time': 0.0}
  output, published, scheduling = [], [], []
  prints = io.StringIO()

  def capture(local):
    estimator = local['estimator']
    critical = ('accelerometer', 'gyroscope', 'cameraOdometry')
    output.append(
      {
        'initialized': local['filter_initialized'],
        'invalid': [local['observation_input_invalid'][name] for name in critical],
        'sensor_valid': [local['sensor_valid'][name] for name in critical[:2]],
        'sensor_alive': [local['sensor_alive'][name] for name in critical[:2]],
        'sensor_received': [local['sensor_recv_time'][name] for name in critical[:2]],
        'x': estimator.kf.x.tolist(),
        'p': estimator.kf.P.flatten().tolist(),
        'publications': list(published),
        'logs': list(logs),
        'printed': prints.getvalue(),
      }
    )
    published.clear()
    logs.clear()
    prints.seek(0)
    prints.truncate()

  class SubMaster(messaging['SubMaster']):
    def update(self):
      if current['index'] >= 0:
        capture(sys._getframe(1).f_locals)
      current['index'] += 1
      if current['index'] == len(case['frames']):
        raise Finished()
      frame = case['frames'][current['index']]
      current['time'] = frame['time']
      self.update_msgs(frame['time'], [reader(packet) for packet in frame['messages']])

  def drain(name):
    key = 'acceleration' if name == 'accelerometer' else 'gyroscope'
    return [reader(packet) for packet in case['frames'][current['index']][key]]

  def publish(name, event):
    assert name == 'livePose'
    published.append(event.to_dict())

  source['time'].monotonic = lambda: current['time']
  source.update(
    SERVICE_LIST=SERVICE_LIST,
    config_realtime_process=lambda cores, priority: scheduling.append([cores, priority]),
    os=types.SimpleNamespace(getenv=lambda name, default: {'DEBUG': '1', 'SIMULATION': str(int(case['simulation']))}.get(name, default)),
    Params=lambda: types.SimpleNamespace(get=lambda key: bytes(case['seed']) if case['seed'] is not None else None),
  )
  source['messaging'].SubMaster = SubMaster
  source['messaging'].PubMaster = lambda names: types.SimpleNamespace(send=publish)
  source['messaging'].sub_sock = lambda name, timeout: name
  source['messaging'].drain_sock = drain
  with redirect_stdout(prints):
    try:
      source['main']()
    except Finished:
      pass
  assert scheduling == [[[0, 1, 2, 3], 5]]
  return {'name': case['name'], 'rows': output}
