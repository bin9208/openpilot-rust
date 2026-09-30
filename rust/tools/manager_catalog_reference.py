"""Run unchanged manager constructors/predicates with real Cython Params.

Only hardware flags, import-time bodyteleop discovery, GPS existence, and unused
Sentry import are fixture boundaries. No process start/prepare method is called.
"""

import argparse
import importlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import sys
from types import ModuleType, SimpleNamespace
from unittest.mock import patch  # noqa: TID251 - subprocess oracle uses patching, not unittest tests.

from original_params_binding import load


def evaluate_case(binding, module, case, index, output):
  root = output / f'params-{index}'
  params = binding.Params(str(root))
  for key, value in case.get('values', {}).items():
    Path(os.fsdecode(params.get_param_path(key))).write_bytes(bytes(value))
  for key in case.get('directories', []):
    target = Path(os.fsdecode(params.get_param_path(key)))
    target.unlink(missing_ok=True)
    target.mkdir()
  trace = []
  trace_path = output / f'trace-{index}.json'

  def record(event):
    trace.append(event)
    trace_path.write_text(json.dumps(trace) + '\n')

  def access(op, key):
    record([op, key])
    if key == case.get('exception_key'):
      raise RuntimeError('fixture parameter exception')

  class TracedParams:
    def get_bool(self, key):
      access('bool', key)
      return params.get_bool(key)

    def get_int(self, key):
      access('int', key)
      return params.get_int(key)

    def get(self, key, return_default=False):
      assert return_default
      access('default', key)
      return params.get(key, return_default=True)

    def put_bool(self, key, value):
      record(['put', key, value])
      params.put_bool(key, value)

  def exists(path):
    record(['exists', path])
    return path in case.get('gps_paths', [])

  if case['predicate'].startswith('process:'):
    callback = module.managed_processes[case['predicate'][8:]].should_run
  else:
    callback = getattr(module, case['predicate'])
  try:
    with patch.object(module.os.path, 'exists', side_effect=exists):
      value = callback(case['started'], TracedParams(), SimpleNamespace(notCar=case['not_car']))
    outcome = {'value': value}
  except Exception:
    outcome = {'error': 'parameter'}
  return {'outcome': outcome, 'trace': trace, 'ublox': params.get_bool('UbloxAvailable')}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binding', type=Path)
  parser.add_argument('request', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  request = json.loads(args.request.read_text())
  args.output.mkdir(parents=True, exist_ok=True)
  binding, _ = load(args.binding, f'ipc://{args.output}/unused-log', args.output / 'logs')
  hardware = ModuleType('openpilot.system.hardware')
  actual_tici = Path('/TICI').is_file()
  config = request.get('config') or {
    'pc': not actual_tici,
    'tici': actual_tici,
    'webcam': 'USE_WEBCAM' in os.environ,
    'carrot_web_external': os.getenv('CARROT_WEB_EXTERNAL') == '1',
    'darwin': platform.system() == 'Darwin',
    'bodyteleop_available': request.get('bodyteleop_available', False),
  }
  hardware.PC, hardware.TICI = config['pc'], config['tici']
  sys.modules[hardware.__name__] = hardware
  sys.modules['openpilot.system.sentry'] = ModuleType('openpilot.system.sentry')
  env = dict(os.environ)
  if 'config' in request:
    for key, enabled in [('USE_WEBCAM', config['webcam']), ('CARROT_WEB_EXTERNAL', config['carrot_web_external'])]:
      env.pop(key, None)
      if enabled:
        env[key] = '1'
  real_find_spec = importlib.util.find_spec

  def find_spec(name, *a, **kw):
    if name == 'openpilot.tools.bodyteleop.web':
      if request.get('body_discovery_missing_parent'):
        raise ModuleNotFoundError(name)
      return SimpleNamespace() if config['bodyteleop_available'] else None
    return real_find_spec(name, *a, **kw)

  with (
    patch.dict(os.environ, env, clear=True),
    patch.object(platform, 'system', return_value='Darwin' if config['darwin'] else 'Linux'),
    patch.object(importlib.util, 'find_spec', side_effect=find_spec),
  ):
    module = importlib.import_module('openpilot.system.manager.process_config')
  descriptors = []
  for process in module.procs:
    kind = type(process).__name__
    descriptors.append(
      {
        'name': process.name,
        'kind': kind,
        'module': getattr(process, 'module', None),
        'cwd': getattr(process, 'cwd', None),
        'argv': getattr(process, 'cmdline', None),
        'pid_key': getattr(process, 'param_name', None),
        'enabled': process.enabled,
        'sigkill': process.sigkill,
        'restart_if_crash': process.restart_if_crash,
        'daemon': process.daemon,
      }
    )
  results = [evaluate_case(binding, module, case, index, args.output) for index, case in enumerate(request.get('cases', []))]
  (args.output / 'result.json').write_text(json.dumps({'catalog': descriptors, 'results': results}, indent=2) + '\n')


if __name__ == '__main__':
  main()
