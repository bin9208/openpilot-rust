# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run through check_usbgpu_hardware.py; this module supplies the unchanged source oracle.
"""Original USB GPU functions with only filesystem/USB/process boundary fixtures."""

from __future__ import annotations

import ast
from pathlib import Path
import struct
import sys
from types import ModuleType, SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]


def hardware_source() -> ModuleType:
  path = ROOT / 'openpilot/system/hardware/usbgpu.py'
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if not (isinstance(node, ast.ImportFrom) and node.module == 'openpilot.common.basedir')]
  module = ModuleType('usbgpu_source_reference')
  module.__dict__['BASEDIR'] = str(ROOT)
  sys.modules[module.__name__] = module
  exec(compile(tree, str(path), 'exec'), module.__dict__)
  return module


def make_sysfs(root: Path, *, speed: str = '5000', product: str = 'custom ed4e39b7-CLEAN', count: int = 1) -> Path:
  devices = root / 'devices'
  devices.mkdir(parents=True)
  controller = root / 'controller.ssusb'
  controller.mkdir()
  (controller / 'portli').write_text('0x10003')
  for index in range(count):
    device = controller / f'usb1/1-{index + 1}'
    device.mkdir(parents=True)
    for name, value in {
      'idVendor': 'add1' if index == 0 else '3801',
      'idProduct': '1',
      'speed': speed,
      'product': product,
      'manufacturer': 'fixture',
      'busnum': '1',
      'devnum': str(index + 8),
    }.items():
      (device / name).write_text(value)
    (devices / device.name).symlink_to(device)
  return devices


def source_power(source: ModuleType, devices: Path, raw: list[int] | None) -> tuple[dict, list]:
  calls = []
  device = SimpleNamespace(ctrl_transfer=lambda *args, **kwargs: calls.append({'args': args, 'kwargs': kwargs}) or bytes(raw))
  package = ModuleType('usb')
  core = ModuleType('usb.core')
  core.find = lambda **kwargs: device if raw is not None else None
  util = ModuleType('usb.util')
  util.dispose_resources = lambda value: calls.append({'disposed': value is device})
  package.core, package.util = core, util
  prior = {name: sys.modules.get(name) for name in ['usb', 'usb.core', 'usb.util']}
  sys.modules.update({'usb': package, 'usb.core': core, 'usb.util': util})
  try:
    error = source.check_usbgpu_power(devices)
    if raw is not None and len(raw) < 5:
      return {'error': error}, calls
    values = None if raw is None else struct.unpack_from('<HhB', bytes(raw))
    return {'value': None if values is None else {'voltage_mv': values[0], 'current_ma': values[1], 'fault': bool(values[2])}, 'error': error}, calls
  finally:
    for name, value in prior.items():
      if value is None:
        del sys.modules[name]
      else:
        sys.modules[name] = value
