"""Actual lagd.main body with only external IPC/Params/realtime boundaries replaced."""

from types import SimpleNamespace
import sys
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from lagd_frames import TOPICS
from lagd_reference import source


class EndOfInput(Exception):
  pass


def run(frames, configuration):
  scope = source()
  stored = {
    key: bytes(value) if value is not None else None
    for key, value in [('CarParams', configuration['car']), ('LiveDelay', configuration['saved']), ('CarParamsPrevRoute', configuration['previous'])]
  }
  output = []
  removed = []

  class Params:
    def get(self, key, **kwargs):
      return stored.get(key) or None

    def remove(self, key):
      removed.append(key)
      stored.pop(key, None)

    def put_nonblocking(self, key, value):
      output[-1]['persist'] = True
      stored[key] = value

  pending = iter(frames)

  class SubMaster:
    def __init__(self, names, **kwargs):
      assert names == TOPICS
      self.frame = -1
      self.updated = dict.fromkeys(names, False)
      self.logMonoTime = dict.fromkeys(names, 0)
      self.seen = dict.fromkeys(names, False)
      self.valid = dict.fromkeys(names, False)
      self.values = {}

    def update(self):
      if output:
        estimator = sys._getframe(1).f_locals['lag_learner']
        times, desired, actual, okay = estimator.points.get()
        output[-1]['state'] = {
          'okay': int(estimator.points.num_okay),
          'last': {'time': float(times[-1]), 'desired': float(desired[-1]), 'actual': float(actual[-1]), 'okay': bool(okay[-1])},
          'last_estimate': estimator.last_estimate_t,
          'block_idx': estimator.block_avg.block_idx,
          'idx': estimator.block_avg.idx,
          'valid_blocks': estimator.block_avg.valid_blocks,
          'recovery': [estimator.last_lat_inactive_t, estimator.last_steering_pressed_t, estimator.last_steering_saturated_t, estimator.last_pose_invalid_t],
        }
      try:
        row = next(pending)
      except StopIteration:
        raise EndOfInput from None
      self.frame += 1
      self.updated = dict.fromkeys(TOPICS, False)
      for data in row['messages']:
        with log.Event.from_bytes(bytes(data)) as event:
          topic = event.which()
          self.values[topic] = getattr(event, topic).as_builder()
          self.logMonoTime[topic] = event.logMonoTime
          self.valid[topic] = event.valid
          self.updated[topic] = self.seen[topic] = True
      output.append({'frame': self.frame, 'valid': self.all_checks(), 'publish': False, 'persist': False, 'packet': None, 'removed': bool(removed)})

    def all_checks(self):
      return all(self.seen.values()) and all(self.valid.values())

    def __getitem__(self, name):
      return self.values[name]

  class PubMaster:
    def __init__(self, names):
      assert names == ['liveDelay']

    def send(self, topic, data):
      assert topic == 'liveDelay'
      output[-1].update(publish=True, packet=list(data))

  def read(data, schema):
    with schema.from_bytes(data) as value:
      return value.as_builder()

  scope.update(
    Params=Params,
    SERVICE_LIST=SERVICE_LIST,
    config_realtime_process=lambda *args: None,
    os=SimpleNamespace(getenv=lambda key, default: '1' if key == 'DEBUG' else default),
    cloudlog=SimpleNamespace(error=lambda value: None),
  )
  scope['messaging'].PubMaster = PubMaster
  scope['messaging'].SubMaster = SubMaster
  scope['messaging'].log_from_bytes = read
  try:
    scope['main']()
  except EndOfInput:
    return output
  raise AssertionError('source loop unexpectedly returned')
