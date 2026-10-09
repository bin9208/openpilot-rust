#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_system_actions_source.py < OWNED_INPUT_JSON
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import types
from typing import assert_never

import anyio
from aiohttp import web
from carrot_server_system_source import definitions
from original_params_binding import load


def main() -> None:
  config = json.loads(sys.stdin.readline())
  root = Path(config['root'])
  os.environ['CARROT_DATA_DIR'] = str(root / 'state')
  os.environ['CARROT_SETTINGS_PATH'] = str(root / 'settings.json')
  os.environ['OPENPILOT_PREFIX'] = 'd'
  load(Path(config['binding']), f'ipc://{root}/logs.sock', root / 'logs')
  from openpilot.common.params import Params
  from openpilot.selfdrive.carrot.server.services import params, settings

  store = Params(str(root / 'params'))
  for key, raw in config.get('initial', {}).items():
    Path(store.get_param_path(key)).write_bytes(bytes.fromhex(raw))
  if namespace := config.get('namespace'):
    (root / 'params/d').unlink()
    if namespace == 'blocked':
      (root / 'params').chmod(0)
  params.Params = lambda: Params(str(root / 'params'))
  (root / 'settings.json').write_text(json.dumps(config.get('data', {'params': []})))
  system = definitions('features/system.py')
  system.__dict__.update(
    {
      'web': web,
      'HAS_PARAMS': config.get('params', True),
      'Params': params.Params,
      'get_settings_cached': settings.get_settings_cached,
      'restore_param_values_validated': params.restore_param_values_validated,
    }
  )
  commands = []
  children: list[subprocess.Popen] = []

  def spawn(args: list[str]) -> subprocess.Popen:
    assert args == ['sudo', 'reboot']
    commands.append(args)
    if config.get('spawn_fail'):
      raise FileNotFoundError(2, 'No such file or directory', 'sudo')
    child = subprocess.Popen(args)
    children.append(child)
    return child

  system.subprocess = types.SimpleNamespace(Popen=spawn)
  match config['mode']:
    case 'calibration':
      device = definitions('services/device_info.py')
      device.HAS_PARAMS, device.Params = config.get('params', True), params.Params
      if calibration := config.get('calibration'):
        from openpilot.cereal import log

        event = log.Event.new_message()
        event.init('liveCalibration')
        event.liveCalibration.calStatus = calibration['status']
        event.liveCalibration.rpyCalib = calibration['rpy']
        Path(store.get_param_path('CalibrationParams')).write_bytes(event.to_bytes())
      body = device.get_calibration_status()
      status = 200
    case 'action' | 'defaults':
      functions = {'reboot': system.api_reboot, 'poweroff': system.api_poweroff, 'recalibrate': system.api_recalibrate, 'defaults': system.api_set_default}
      snapshot = {'services': {'selfdriveState': {'enabled': config.get('engaged', False)}}}
      request = types.SimpleNamespace(app={'realtime_broker': types.SimpleNamespace(last_snapshot=snapshot)})
      response = anyio.run(functions[config.get('action', 'defaults')], request)
      body, status = json.loads(response.body), response.status
    case unexpected:
      assert_never(unexpected)
  keys = set(config.get('initial', {})) | {'DoReboot', 'DoShutdown', 'OnroadCycleRequested', 'CalibrationParams'}
  if config.get('namespace'):
    (root / 'params').chmod(0o700)
  keys.update(row['name'] for row in config.get('data', {}).get('params', []))
  stored = {key: Path(store.get_param_path(key)).read_bytes().hex() for key in sorted(keys) if Path(store.get_param_path(key)).is_file()}
  for child in children:
    assert child.wait(timeout=2) == 0
  print(json.dumps({'status': status, 'body': body, 'commands': commands, 'stored': stored}))


if __name__ == '__main__':
  main()
