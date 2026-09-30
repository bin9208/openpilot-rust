"""Execute unchanged manager.py with native Cython Params and isolated OS boundaries."""
import contextlib
import datetime
import importlib.util
import json
import os
from pathlib import Path
import sys
from types import ModuleType, SimpleNamespace
from unittest.mock import patch  # noqa: TID251 - external-boundary source oracle, not a unittest suite.

from original_params_binding import load
for package in ('openpilot.system', 'openpilot.system.manager', 'openpilot.cereal'):
  importlib.import_module(package)


def main():
  binding, output, scenario = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
  output.mkdir(parents=True, exist_ok=True)
  trace = []
  def record(*values):
    trace.append(list(values))
  original, _ = load(binding, 'inproc://manager-oracle', output / 'logs')
  params = original.Params(str(output / 'params'))
  for key, value in {'RecordFrontLock': b'1', 'UseWideCamera': b'0', 'HardwareC3xLite': b'1',
                     'RecordAudio': b'1', 'CarParams': b'stale', 'GitCommit': b'old'}.items():
    Path(os.fsdecode(params.get_param_path(key))).write_bytes(value)

  if scenario == 'default_edges':
    for key, value in {'CarrotYouTubeLive': '_1', 'CarrotYouTubeQuality': '9' * 53,
                       'UptimeOffroad': '1_2.5', 'UptimeOnroad': '1__2.5'}.items():
      Path(os.fsdecode(params.get_param_path(key))).write_text(value)

  class TracedParams:
    def __getattr__(self, name):
      return getattr(params, name)

    def clear_all(self, flags):
      record('clear', int(flags))
      params.clear_all(flags)

    def put_bool(self, key, value):
      if key in ('IsOnroad', 'IsOffroad', 'RecordAudio'):
        record('put', key, '1' if value else '0')
      params.put_bool(key, value)

    def put(self, key, value):
      if key == 'LastManagerExitReason':
        record('put', key, value)
      params.put(key, value)

  def module(name, **fields):
    value = ModuleType(name)
    value.__dict__.update(fields)
    sys.modules[name] = value
    return value

  module('openpilot.common.params', Params=TracedParams, ParamKeyFlag=original.ParamKeyFlag)
  hardware = SimpleNamespace(get_serial=lambda: (record('serial'), 'fixture-serial')[1], get_device_type=lambda: 'pc',
                             uninstall=lambda: record('exit', 'uninstall'), reboot=lambda: record('exit', 'reboot'),
                             shutdown=lambda: record('exit', 'shutdown'))
  module('openpilot.system.hardware', HARDWARE=hardware)
  sys.modules['openpilot.system.hardware.hw'].Paths.shm_path = lambda: str(output / 'shm')
  module('openpilot.system.sentry', SentryProject=SimpleNamespace(SELFDRIVE=0), init=lambda _: None,
         capture_exception=lambda: record('capture'))
  module('openpilot.common.text_window', TextWindow=None)
  module('openpilot.common.repo_update', release_boot_lock=lambda: record('unlock'))
  process = SimpleNamespace(prepare=lambda: record('prepare'), stop=lambda block=True: record('stop', block), proc=None,
                            get_process_state_msg=lambda: None)
  module('openpilot.system.manager.process_config', managed_processes={'fixture': process})
  module('openpilot.system.manager.process', ensure_running=lambda processes, started, params, CP, not_run:
         record('ensure', started, CP.notCar, not_run))
  module('openpilot.system.manager.update_status', UpdateStatus=lambda _: (record('checkout'), SimpleNamespace(update=lambda _: False))[1])
  def register(**_):
    record('register')
    if scenario == 'registration_failure':
      return None
    params.put('DongleId', 'fixture-id')
    return 'fixture-id'
  module('openpilot.system.athena.registration', register=register, UNREGISTERED_DONGLE_ID='UnregisteredDevice')
  metadata = SimpleNamespace(release_channel=True, tested_channel=True, channel='release-tizi', openpilot=SimpleNamespace(
    version='fixture', git_commit='a' * 40, git_commit_date='date', git_origin='https://github.com/commaai/openpilot.git',
    git_normalized_origin='github.com/commaai/openpilot', is_dirty=False))
  module('openpilot.system.version', get_build_metadata=lambda: (record('metadata'), metadata)[1])
  class SubMaster:
    def __init__(self, *_args, **_kwargs):
      self.frame = 0
    def update(self, timeout):
      record('poll', timeout)
      if scenario == 'poll_failure':
        raise RuntimeError('fixture poll failure')
      if scenario == 'interrupted':
        raise SystemExit(1)
      self.frame += 1
      if self.frame == 3:
        for key in ('DoUninstall', 'DoShutdown', 'DoReboot'):
          params.put_bool(key, True)
    def __getitem__(self, key):
      if key == 'deviceState':
        return SimpleNamespace(started=self.frame < 3)
      if key == 'carParams':
        return SimpleNamespace(notCar=False)
      return [SimpleNamespace(ignitionLine=self.frame != 2, ignitionCan=False, pandaType=1)]
    def all_checks(self, _):
      return True
  module('openpilot.cereal.messaging', SubMaster=SubMaster,
         PubMaster=lambda _: SimpleNamespace(send=lambda _, msg: record('publish', msg.managerState.rebootRequired)),
         new_message=lambda *_args, **_kwargs: SimpleNamespace(managerState=SimpleNamespace()))
  spec = importlib.util.spec_from_file_location('manager_source', 'openpilot/system/manager/manager.py')
  manager = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(manager)
  manager.save_bootlog = lambda: record('bootlog')
  manager.cloudlog = SimpleNamespace(bind_global=lambda **_: record('logging'), bind=lambda **_: None,
    info=lambda *_: None, debug=lambda *_: None, warning=lambda text: record('warning', text)
    if text.startswith('Shutting') else None, exception=lambda text: record('exception', text))
  @contextlib.contextmanager
  def watchdog(*_, **__):
    record('watchdog')
    raise PermissionError('fixture watchdog failure')
    yield
  manager.atomic_write = watchdog
  class DateTime(datetime.datetime):
    @classmethod
    def now(cls):
      return cls(2026, 10, 1, 12)
  manager.datetime = SimpleNamespace(datetime=DateTime)
  def cars(name):
    brand = name.split('.')[-2]
    record('cars', brand)
    if brand == 'gm':
      raise ImportError('fixture cars failure')
    return SimpleNamespace(CAR=[SimpleNamespace(config=SimpleNamespace(car_docs=[SimpleNamespace(name='Z car'), SimpleNamespace(name='A car')]))])
  manager.importlib = SimpleNamespace(import_module=cars)
  os.environ['NOBOARD'] = ''
  os.environ['BLOCK'] = 'ui,,custom'
  if scenario == 'prepare_only':
    os.environ['PREPAREONLY'] = ''
  else:
    os.environ.pop('PREPAREONLY', None)
  status = 'ok'
  try:
    with patch.object(manager.signal, 'signal'):
      if scenario == 'reset_defaults':
        manager.set_default_params()
      else:
        manager.main()
  except SystemExit:
    status = 'interrupted'
  except Exception:
    import traceback
    traceback.print_exc()
    status = 'error'
  values = {path.name: list(path.read_bytes()) for path in Path(os.fsdecode(params.get_param_path())).iterdir() if path.is_file()}
  environment = {key: os.environ[key] for key in ('DISABLE_WIDE_ROAD', 'DONGLE_ID', 'GIT_ORIGIN', 'GIT_BRANCH', 'GIT_COMMIT', 'CLEAN') if key in os.environ}
  (output / 'result.json').write_text(json.dumps({'status': status, 'trace': trace, 'params': values, 'environment': environment}, indent=2))


if __name__ == '__main__':
  main()
