import datetime
import json
import sys
from types import ModuleType
from product_effects import car_bytes


def install(scene):
  class Params:
    def get(self, key, *args, **kwargs):
      if key in scene.get('raw_params', {}):
        return bytes(scene['raw_params'][key])
      if key == 'CarParamsPersistent':
        return car_bytes(scene)
      value = scene.get('params', {}).get(key)
      if key == 'LastUpdateTime':
        try:
          return datetime.datetime.fromisoformat(value) if value else None
        except ValueError:
          return None
      if key in ['UpdaterCurrentReleaseNotes', 'UpdaterNewReleaseNotes']:
        return value.encode() if value else None
      if key in ['LongitudinalPersonality', 'UpdateFailedCount']:
        try:
          return int(value.encode()) if value else (1 if kwargs.get('return_default') else None)
        except ValueError:
          return 1 if kwargs.get('return_default') else None
      return json.loads(value) if key == 'ApiCache_FirehoseStats' and value else value

    def get_bool(self, key, *args):
      return self.get(key) == '1'

    def put_bool(self, key, value, **kwargs):
      scene.setdefault('params', {})[key] = '1' if value else '0'

    def put(self, key, value):
      scene.setdefault('params', {})[key] = str(value)

    def put_bool_nonblocking(self, key, value):
      self.put_bool(key, value)

    def put_nonblocking(self, key, value):
      self.put(key, value)

    def remove(self, key):
      scene.setdefault('params', {}).pop(key, None)
      scene.setdefault('raw_params', {}).pop(key, None)

    def get_int(self, key, *args):
      return int(self.get(key) or 0)

  module = ModuleType('openpilot.common.params')
  module.Params = Params
  module.UnknownKeyName = KeyError
  sys.modules[module.__name__] = module
  return Params
