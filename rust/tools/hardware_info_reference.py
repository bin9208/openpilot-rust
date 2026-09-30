# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp==2.1.0", "pyzmq==27.2.0", "requests==2.32.5", "pyserial==3.5", "zstandard==0.25.0", "numpy==2.5.3"]
# ///
# Run through check_hardware_info.py; arguments: original Params binding, request, output.
"""Unchanged hardware source oracle using real files, commands and Unix datagrams.

Only filesystem roots, WPA endpoint, uptime and environment are redirected.
All board-control methods remain unreachable in the curated read-only call table.
"""

import argparse
import builtins
import dataclasses
import importlib
import io
import json
import os
from pathlib import Path
import shutil
import socket
import sys
from unittest.mock import patch  # noqa: TID251 - real subprocess oracle uses patching only.

from original_params_binding import load


class BaseFixture:
  """Construct the original abstract base after supplying its required identity."""

  @staticmethod
  def create(base):
    class MinimalHardware(base.HardwareBase):
      def get_device_type(self):
        return 'base'

    return MinimalHardware()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binding', type=Path)
  parser.add_argument('request', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  request = json.loads(args.request.read_text())
  args.output.mkdir(parents=True, exist_ok=True)
  root = Path(request['root'])
  root.mkdir(parents=True, exist_ok=True)
  load(args.binding, 'ipc://' + str(args.output / 'unused-log'), args.output / 'logs')
  sys.modules.pop('openpilot.system.hardware.hw', None)
  base = importlib.import_module('openpilot.system.hardware.base')
  pc = importlib.import_module('openpilot.system.hardware.pc.hardware')
  tici = importlib.import_module('openpilot.system.hardware.tici.hardware')
  hw = importlib.import_module('openpilot.system.hardware.hw')
  hardware_kind = request.get('hardware', 'tici')

  def instance():
    match hardware_kind:
      case 'pc':
        return pc.Pc()
      case 'base':
        return BaseFixture.create(base)
      case 'tici':
        return tici.Tici()
      case _:
        raise AssertionError(hardware_kind)

  hardware = instance()
  zones = {}
  config = None
  original_open, original_io_open, original_listdir, original_glob = builtins.open, io.open, os.listdir, Path.glob
  original_socket = socket.socket
  original_sudo_read = tici.sudo_read
  root_string = str(root)

  def mapped(path):
    if isinstance(path, int):
      return path
    text = os.fsdecode(path)
    prefixes = ('/sys/', '/proc/', '/VERSION', '/dev/shm/modem', '/run/NetworkManager/', '/data/etc/NetworkManager/')
    if text.startswith(prefixes):
      return root / text.lstrip('/')
    return path

  def reading_open(path, mode='r', *pos, **kw):
    assert not any(flag in mode for flag in 'wax+'), ('unexpected source write', path, mode)
    return original_open(mapped(path), mode, *pos, **kw)

  def reading_io_open(path, mode='r', *pos, **kw):
    assert not any(flag in mode for flag in 'wax+'), ('unexpected source write', path, mode)
    return original_io_open(mapped(path), mode, *pos, **kw)

  class EndpointSocket(original_socket):
    def connect(self, endpoint):
      return super().connect(request['wpa_endpoint'] if endpoint == '/run/wpa_supplicant/wlan0' else endpoint)

  results = []
  methods = {
    'device_type': 'get_device_type',
    'os_version': 'get_os_version',
    'serial': 'get_serial',
    'modem_state': 'get_modem_state',
    'network_type': 'get_network_type',
    'sim_info': 'get_sim_info',
    'network_info': 'get_network_info',
    'modem_version': 'get_modem_version',
    'modem_temperatures': 'get_modem_temperatures',
    'current_power': 'get_current_power_draw',
    'som_power': 'get_som_power_draw',
    'brightness': 'get_screen_brightness',
    'gpu_usage': 'get_gpu_usage_percent',
    'networks': 'get_networks',
    'modem_usage': 'get_modem_data_usage',
    'voltage': 'get_voltage',
    'current': 'get_current',
    'internal_panda': 'has_internal_panda',
    'booted': 'booted',
  }
  for step in request['steps']:
    action = step['action']
    if action == 'env':
      for key, value in step['values'].items():
        if value is None:
          os.environb.pop(key.encode(), None)
        else:
          os.environb[key.encode()] = bytes(value)
      continue
    if action == 'new_instance':
      hardware = instance()
      continue
    if action == 'zone':
      zones[step['name']] = base.ThermalZone(step['zone'], json.loads(step.get('scale_json', '1000.0')))
      continue
    if action in ('write', 'directory', 'remove', 'chmod', 'symlink'):
      path = root / step['path'].lstrip('/')
      assert str(path).startswith(root_string + '/') and '..' not in path.relative_to(root).parts
      match action:
        case 'write':
          path.parent.mkdir(parents=True, exist_ok=True)
          path.write_bytes(bytes(step['bytes']))
        case 'directory':
          path.mkdir(parents=True, exist_ok=True)
        case 'remove':
          if path.is_dir() and not path.is_symlink():
            shutil.rmtree(path)
          else:
            path.unlink(missing_ok=True)
        case 'chmod':
          path.chmod(step['mode'])
        case 'symlink':
          path.parent.mkdir(parents=True, exist_ok=True)
          path.symlink_to(step['target'])
      continue
    assert action == 'call'
    method = step['method']
    try:
      with (
        patch.object(builtins, 'open', reading_open),
        patch.object(io, 'open', reading_io_open),
        patch.object(os, 'listdir', side_effect=lambda path: original_listdir(mapped(path))),
        patch.object(Path, 'glob', new=lambda path, pattern: original_glob(Path(mapped(path)), pattern)),
        patch.object(socket, 'socket', EndpointSocket),
        patch.object(tici, 'sudo_read', side_effect=lambda path: original_sudo_read(str(mapped(path)))),
        patch.object(tici.time, 'monotonic', return_value=step.get('uptime', 130.0)),
        patch.object(hw, 'PC', request.get('pc', hardware_kind != 'tici')),
        patch.object(hw.platform, 'system', return_value='Darwin' if request.get('darwin', False) else 'Linux'),
      ):
        if method in methods:
          value = getattr(hardware, methods[method])()
        else:
          match method:
            case 'imei':
              value = hardware.get_imei(step.get('slot', 0))
            case 'strength':
              value = hardware.get_network_strength(step.get('network', 0))
            case 'metered':
              value = hardware.get_network_metered(step.get('network', 0))
            case 'parse_strength':
              value = hardware.parse_strength(json.loads(step['value_json']))
            case 'cmdline':
              value = hardware.get_cmdline()
            case 'route':
              value = tici.get_default_route_iface()
            case 'wpa':
              value = tici.wpa_supplicant_cmd(step.get('command', 'STATUS'), step.get('timeout_ms', 200) / 1000)
            case 'thermal_config':
              config = hardware.get_thermal_config()
              value = dataclasses.asdict(config)
            case 'thermal_read':
              value = config.get_msg()
            case 'zone_read':
              zone = zones[step['name']]
              value = {'reading': zone.read(), 'zone_number': zone.zone_number}
            case 'paths':
              names = ('comma_home', 'log_root', 'swaglog_root', 'swaglog_ipc', 'download_cache_root', 'persist_root', 'stats_root', 'config_root', 'shm_path')
              value = {name: list(os.fsencode(getattr(hw.Paths, name)())) for name in names}
            case _:
              raise AssertionError(method)
        if method in ('network_type', 'strength', 'parse_strength'):
          value = int(value)
      serialized = json.dumps(value, ensure_ascii=True)
      results.append({'method': method, 'value_json': serialized})
    except Exception as error:  # Record the original exception class at the oracle boundary.
      results.append({'method': method, 'error': type(error).__name__})
  (args.output / 'result.json').write_text(json.dumps(results, indent=2) + '\n')


if __name__ == '__main__':
  main()
