#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Source System AppRunner; INPUT_JSON supplies every owned path/recipient before startup.
from __future__ import annotations

import asyncio  # Execute original asyncio cleanup and to_thread cancellation unchanged.
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys
import types

from aiohttp import web
from carrot_server_system_source import Paths, definitions
from original_params_binding import load


async def main() -> None:
  config = json.loads(sys.stdin.readline())
  root = Path(config['root'])
  assert os.environ['PARAMS_ROOT'] == str(root / 'params')
  assert os.environ['SYSTEM_FIXTURE_ROOT'] == str(root)
  load(Path(config['binding']), f'ipc://{root}/logs.sock', root / 'logs')
  from openpilot.common.params import Params
  from openpilot.selfdrive.carrot.server.services import params, settings

  params.HAS_PARAMS = config.get('params', True)
  params.Params = lambda: Params(str(root / 'params'))
  device = definitions('services/device_info.py')
  device.__dict__.update({'HAS_PARAMS': params.HAS_PARAMS, 'Params': params.Params, 'subprocess': subprocess, 'get_param_value': params.get_param_value})
  sync = definitions('services/time_sync.py')
  paths = Paths(root)

  def run(command: list[str] | str, **kwargs):
    actual = [paths.owned(arg) for arg in command] if isinstance(command, list) else command
    try:
      return subprocess.run(actual, **kwargs)
    except subprocess.CalledProcessError as failure:
      raise subprocess.CalledProcessError(failure.returncode, command) from None

  sync.__dict__.update(
    {
      'datetime': datetime,
      'os': types.SimpleNamespace(path=paths),
      'time': types.SimpleNamespace(time=lambda: config.get('now', 1700000000)),
      'subprocess': types.SimpleNamespace(run=run, CalledProcessError=subprocess.CalledProcessError),
    }
  )
  system = definitions('features/system.py')
  system.__dict__.update(
    {
      'asyncio': asyncio,
      'web': web,
      'os': os,
      'subprocess': subprocess,
      'HAS_PARAMS': params.HAS_PARAMS,
      'Params': params.Params,
      'TIME_SYNC_DEBUG_DEFAULT': sync.TIME_SYNC_DEBUG_DEFAULT,
      'DEVICE_NETWORK_REFRESH_INTERVAL_SEC': device.DEVICE_NETWORK_REFRESH_INTERVAL_SEC,
      'get_device_network_snapshot': device.get_device_network_snapshot,
      'refresh_device_network': device.refresh_device_network,
      'get_calibration_status': device.get_calibration_status,
      'get_settings_cached': settings.get_settings_cached,
      'restore_param_values_validated': params.restore_param_values_validated,
      'sync_system_time_from_browser': sync.sync_system_time_from_browser,
      'OFFROAD_ASSETS_DIR': str(root / 'offroad'),
    }
  )
  app = web.Application()
  app['realtime_broker'] = types.SimpleNamespace(last_snapshot={'services': {'selfdriveState': {'enabled': config.get('engaged', False)}}})
  app.cleanup_ctx.append(system.device_network_context)
  for name, function in [
    ('device_network', system.api_device_network),
    ('calibration_status', system.api_calibration_status),
    ('regulatory', system.api_regulatory),
  ]:
    app.router.add_get('/api/' + name, function)
  for name, function in [
    ('reboot', system.api_reboot),
    ('poweroff', system.api_poweroff),
    ('recalibrate', system.api_recalibrate),
    ('set_default', system.api_set_default),
    ('time_sync', system.api_time_sync),
  ]:
    app.router.add_post('/api/' + name, function)
  runner = web.AppRunner(app, access_log=None)
  await runner.setup()
  try:
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    print(json.dumps({'ready': True, 'port': site._server.sockets[0].getsockname()[1]}), flush=True)
    while line := await asyncio.to_thread(sys.stdin.readline):
      if not line.strip() or json.loads(line).get('stop'):
        break
  finally:
    await runner.cleanup()
  print(json.dumps({'stopped': True}), flush=True)


if __name__ == '__main__':
  asyncio.run(main())
