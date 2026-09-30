"""Run unchanged registration/API definitions with explicit synthetic hardware seams."""

import ast
from datetime import datetime, timedelta, UTC
import json
import os
from pathlib import Path
from types import SimpleNamespace
from typing import cast

import jwt
import requests
import zmq
from original_params_binding import load


def definitions(path, names):
  tree = ast.parse(path.read_text())
  tree.body = [
    node
    for node in tree.body
    if isinstance(node, (ast.FunctionDef, ast.ClassDef))
    and node.name in names
    or isinstance(node, ast.Assign)
    and any(isinstance(target, ast.Name) and target.id in names for target in node.targets)
  ]
  return compile(tree, str(path), 'exec')


def main():
  config = json.loads(input())
  trace = []
  root = Path(config['source_root'])
  binding, swaglog = load(Path(config['binding']), config['endpoint'], Path(config['log_root']))
  params = binding.Params(config['params'])
  if config.get('closed_log'):
    swaglog.ipchandler.connect()
    swaglog.ipchandler.sock.close()
  imeis = iter(config.get('imeis', ['synthetic-imei', None]))

  def serial():
    trace.append(['serial'])
    value = config.get('serial', 'synthetic-serial')
    if not isinstance(value, str):
      raise RuntimeError('serial fixture failure')
    return value

  def imei(slot):
    trace.append(['imei', slot])
    value = next(imeis, {})
    if value is None or isinstance(value, str):
      return value
    raise RuntimeError('IMEI fixture failure')

  class Clock:
    mono = 0.0
    sleeps = 0

    @classmethod
    def monotonic(cls):
      value = cls.mono
      cls.mono += config.get('step', 0.0)
      trace.append(['monotonic', value])
      return value

    @classmethod
    def sleep(cls, duration):
      trace.append(['sleep', duration])
      cls.sleeps += 1
      if cls.sleeps > config.get('max_sleeps', 40):
        raise KeyboardInterrupt('fixture stop')
      cls.mono += duration

  class DateTime:
    @staticmethod
    def now(tz):
      seconds = config.get('utc', 1_700_000_000)
      trace.append(['now', seconds])
      return datetime.fromtimestamp(seconds, tz)

  class Spinner:
    def __init__(self):
      self.action('spinner_start')

    def action(self, name, *values):
      trace.append([name, *values])
      if config.get('spinner_fail') == name:
        raise RuntimeError(f'{name} fixture failure')

    def update(self, text):
      self.action('spinner_update', text)

    def close(self):
      self.action('spinner_close')

  api = {
    'os': os,
    'jwt': jwt,
    'datetime': DateTime,
    'timedelta': timedelta,
    'UTC': UTC,
    'Paths': SimpleNamespace(persist_root=lambda: config['persist']),
    'API_HOST': config['api_host'],
    'BASEDIR': str(root),
    'requests': requests,
  }
  exec(definitions(root / 'openpilot/system/version.py', {'get_version'}), api)
  exec(definitions(root / 'openpilot/common/api.py', {'get_key_pair', 'api_get', 'Api', 'KEYS'}), api)
  scope = {
    'time': Clock,
    'json': json,
    'jwt': jwt,
    'cast': cast,
    'Path': Path,
    'datetime': DateTime,
    'timedelta': timedelta,
    'UTC': UTC,
    'api_get': api['api_get'],
    'get_key_pair': api['get_key_pair'],
    'Params': lambda: params,
    'Spinner': Spinner,
    'HARDWARE': SimpleNamespace(get_serial=serial, get_imei=imei),
    'Paths': api['Paths'],
    'cloudlog': swaglog.cloudlog,
    'UNREGISTERED_DONGLE_ID': 'UnregisteredDevice',
  }
  exec(definitions(root / 'openpilot/system/athena/registration.py', {'register', 'is_registered_device'}), scope)
  try:
    if config.get('mode') == 'is_registered':
      outcome = {'value': scope['is_registered_device']()}
    else:
      outcome = {'value_json': json.dumps(scope['register'](config.get('spinner', False)))}
  except BaseException as error:
    if isinstance(error, zmq.ZMQError):
      category = 'logging'
    elif isinstance(error, KeyboardInterrupt):
      category = 'stop'
    elif isinstance(error, (TypeError, UnicodeEncodeError)):
      category = 'identity_type'
    elif isinstance(error, UnicodeDecodeError):
      category = 'decode'
    elif isinstance(error, OSError):
      category = 'io'
    elif trace and trace[-1][0].startswith('spinner_'):
      category = 'spinner'
    elif trace and trace[-1][0] == 'serial':
      category = 'hardware'
    else:
      category = 'other'
    outcome = {'error': category, 'detail': str(error), 'type': type(error).__name__}
  if config.get('closed_log'):
    swaglog.cloudlog.removeHandler(swaglog.ipchandler)
    swaglog.cloudlog.addHandler(swaglog.UnixDomainSocketHandler(swaglog.SwagFormatter(swaglog.cloudlog)))
  swaglog.cloudlog.debug('registration-fixture-end')
  path = Path(params.get_param_path('DongleId'))
  print(json.dumps({'outcome': outcome, 'trace': trace, 'params_hex': path.read_bytes().hex() if path.is_file() else None}), flush=True)
  input()


if __name__ == '__main__':
  main()
