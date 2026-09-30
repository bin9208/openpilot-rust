"""Unchanged timed/timezone/time_helpers function bodies with isolated external inputs."""
import ast
import datetime
import json
import os
from pathlib import Path
import subprocess
from types import SimpleNamespace
import urllib.request

from logging_producer_reference import source as logger_source

ROOT = Path(__file__).resolve().parents[2]


def definitions(path, scope):
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom)) and not isinstance(node, ast.If)]
  exec(compile(tree, str(path), 'exec'), scope)


class Params:
  def __init__(self, path, logger):
    self.path, self.logger = path, logger

  def get(self, key):
    try:
      data = (self.path / key).read_bytes()
    except OSError:
      return None
    if not data:
      return None
    try:
      return data.decode()
    except UnicodeDecodeError:
      self.logger.warning(f'Failed to cast param {key} with value={data!r} from type t=<ParamKeyType.STRING: 0>')
      return None

  def get_bool(self, key):
    return self.get(key) == '1'

  def put(self, key, value):
    (self.path / key).write_text(value)


class Source:
  def __init__(self, config, params_path):
    self.config = config
    self.logger, self.capture = logger_source()
    self.params = Params(params_path, self.logger)
    self.sleeps, self.publications = [], []
    self.monotonic = config['monotonic'] or 0
    self.queue = []
    class FrozenDatetime(datetime.datetime):
      @classmethod
      def now(cls):
        return cls.fromtimestamp(config['wall'] / 1e9)

    self.scope = {
      '__name__': 'timed_reference', 'datetime': SimpleNamespace(datetime=FrozenDatetime, timedelta=datetime.timedelta), 'subprocess': subprocess,
      'time': SimpleNamespace(time=lambda: config['wall'] / 1e9, time_ns=lambda: config['wall'],
                              monotonic=lambda: self.monotonic / 1e9, sleep=self.sleeps.append),
      'NoReturn': object, 'cloudlog': self.logger, 'Params': Params,
      'os': os, 'json': json, 'urllib': SimpleNamespace(request=SimpleNamespace(
        Request=lambda url, **kw: urllib.request.Request(config['endpoint'], **kw), urlopen=urllib.request.urlopen)),
      'Path': lambda unused: Path(config['paths']['systemd']),
    }
    definitions(ROOT / 'openpilot/common/time_helpers.py', self.scope)
    definitions(ROOT / 'openpilot/common/gps.py', self.scope)
    definitions(ROOT / 'openpilot/system/timezone_helper.py', self.scope)
    self.scope.update(LOCALTIME_PATH=config['paths']['localtime'], ZONEINFO_DIR=config['paths']['zoneinfo'])
    definitions(ROOT / 'openpilot/system/timed.py', self.scope)
    self.scope['Params'] = lambda: self.params

  def action(self, action):
    kind = action['kind']
    if kind == 'apply':
      return self.scope['apply_timezone'](action['zone'], action['source'], self.params)
    if kind == 'gps':
      return self.scope['timezone_from_gps'](action['longitude'])
    if kind == 'internet':
      return self.scope['timezone_from_internet']()
    if kind == 'valid':
      return self.scope['system_time_valid']()
    if kind == 'bounds':
      return [str(self.scope['min_date']()), str(self.scope['MAX_DATE'])]
    if kind == 'set_time':
      return self.scope['set_time'](action['epoch'])
    raise ValueError(kind)

  def loop(self, actions):
    owner = self
    service = self.scope['get_gps_location_service'](self.params)
    queue = iter(actions)

    class Subscriber:
      def __init__(self, names):
        assert names == [service]

      def update(self, timeout):
        assert timeout == 1000
        row = next(queue)
        owner.monotonic = row['monotonic']
        gps = row['gps']
        self.updated = {service: gps['updated']}
        self.logMonoTime = {service: gps['log_mono_time']}
        self.gps = SimpleNamespace(hasFix=gps['has_fix'], longitude=gps['longitude'], unixTimestampMillis=gps['unix_timestamp_millis'])

      def __getitem__(self, key):
        assert key == service
        return self.gps

    def send(name, msg):
      assert name == 'clocks'
      self.publications.append({'valid': msg.valid, 'wall': msg.clocks.wallTimeNanos})

    self.scope['messaging'] = SimpleNamespace(SubMaster=Subscriber, PubMaster=lambda names: SimpleNamespace(send=send),
                                            new_message=lambda name: SimpleNamespace(valid=False, clocks=SimpleNamespace()))
    try:
      self.scope['main']()
    except StopIteration as error:
      self.last_attempt = error.__traceback__.tb_next.tb_frame.f_locals["last_tz_attempt"]
    return service

  def records(self):
    return [{'level': record.levelno, 'msg': record.getMessage(), 'exception': record.exc_info is not None} for record in self.capture.records]
