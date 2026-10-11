from __future__ import annotations

import argparse
import gc
import importlib
import json
import os
from pathlib import Path
import subprocess
import sys
from typing import Any


def configuration(dev: Any) -> dict[str, Any]:
  cfg = dev.get_active_configuration()
  return {
    'value': cfg.bConfigurationValue,
    'interfaces': [
      {
        'number': intf.bInterfaceNumber,
        'alternate': intf.bAlternateSetting,
        'endpoints': [{'address': ep.bEndpointAddress, 'attributes': ep.bmAttributes, 'packet_size': ep.wMaxPacketSize} for ep in intf],
      }
      for intf in cfg
    ],
  }


def source(args: argparse.Namespace) -> None:
  sys.path[:0] = [str(args.oracle), str(args.root / 'openpilot/selfdrive/carrot/cluster'), str(args.root)]
  import usb.backend.libusb1
  import usb.util
  from openpilot.common import usbgpu_bus_lock

  display_module = importlib.import_module('cluster_usb_display')
  usbgpu_bus_lock.USBGPU_BUS_LOCK_PATH = str(args.output / 'owned-bus.lock')
  backend = usb.backend.libusb1.get_backend(find_library=lambda _: str(args.library))
  assert backend is not None
  display_module._LIBUSB_BACKEND = backend
  display = display_module.TuringUsbDisplay()
  result: dict[str, Any] = {'found': None, 'scan_again': None, 'configuration': None, 'error_code': None, 'error': None}
  try:
    result['found'] = display_module.find_supported_usb_product()
    result['scan_again'] = display_module.find_supported_usb_product()
    if result['found'] is not None:
      try:
        display.dev, display.dev_pid = display._find_expected_usb_device()
        display._cache_out_endpoint()
        result['configuration'] = configuration(display.dev)
        result['source_selection'] = [display._ep_out.bEndpointAddress, display._ep_in.bEndpointAddress]
      except (usb.core.USBError, RuntimeError, ValueError) as exc:
        result['error_code'] = getattr(exc, 'backend_error_code', getattr(exc.__cause__, 'backend_error_code', None))
        result['error'] = str(exc)
  finally:
    try:
      if display.dev is not None:
        usb.util.dispose_resources(display.dev)
    finally:
      display._ep_out = display._ep_in = display.dev = None
      gc.collect()
      backend.finalize()
  print(json.dumps(result))


def run(argv: list[str], env: dict[str, str], directory: Path) -> dict[str, Any]:
  directory.mkdir(parents=True, exist_ok=True)
  env = {**env, 'USB_FIXTURE_LOG': str(directory / 'libusb.log')}
  completed = subprocess.run(argv, cwd='/tmp', env=env, capture_output=True, text=True, timeout=15, check=False)
  receipt = {
    'argv': argv,
    'cwd': '/tmp',
    'environment': {key: env[key] for key in ['LD_LIBRARY_PATH', 'USB_FIXTURE_MODE', 'USB_FIXTURE_LOG', 'PYTHONNOUSERSITE']},
    'returncode': completed.returncode,
    'stdout': completed.stdout,
    'stderr': completed.stderr,
    'foreign_log': (directory / 'libusb.log').read_text() if (directory / 'libusb.log').exists() else '',
  }
  (directory / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
  assert completed.returncode == 0, receipt
  assert receipt['foreign_log'].endswith('exit clean\n'), receipt
  return json.loads(completed.stdout.splitlines()[-1])


def compare(args: argparse.Namespace) -> None:
  args.output.mkdir(parents=True, exist_ok=True)
  assert args.library.is_file() and args.native.is_file()
  env = {**os.environ, 'LD_LIBRARY_PATH': str(args.library.parent), 'PYTHONNOUSERSITE': '1'}
  rows = []
  for mode in ['ok', 'pid123', 'missing', 'open_error', 'config_error']:
    case = args.output / mode
    selected_env = {**env, 'USB_FIXTURE_MODE': mode}
    expected = run(
      [
        sys.executable,
        str(Path(__file__).resolve()),
        '--source',
        '--root',
        str(args.root),
        '--oracle',
        str(args.oracle),
        '--library',
        str(args.library),
        '--output',
        str(case / 'source'),
      ],
      selected_env,
      case / 'source',
    )
    actual = run([str(args.native)], selected_env, case / 'native')
    for key in ['found', 'scan_again', 'configuration', 'error_code']:
      assert actual[key] == expected[key], (mode, key, expected, actual)
    assert bool(actual['error']) == bool(expected['error']), (mode, expected, actual)
    if mode == 'open_error':
      assert actual['found'] == 0x92 and actual['error_code'] == -3
    if expected.get('source_selection'):
      assert expected['source_selection'] == [7, 0x84]
    (case / 'result.json').write_text(json.dumps({'source': expected, 'native': actual}, indent=2) + '\n')
    rows.append({'mode': mode, 'pass': True})
  for mode in ['null_config', 'null_interfaces', 'null_alternates', 'negative_alternates', 'null_endpoints', 'config_error_owned']:
    actual = run([str(args.native)], {**env, 'USB_FIXTURE_MODE': mode}, args.output / mode)
    assert actual['configuration'] is None and actual['error'], actual
    rows.append({'mode': mode, 'pass': True, 'scope': 'native provider contract rejection; unsafe foreign trees not passed to Python'})
  result = {
    'pass': True,
    'rows': rows,
    'scope': 'original discovery/configuration methods and actual PyUSB1.3.1 versus additive native metadata API; owned ABI only',
    'limits': (
      'API error codes/failed ownership compared; raw error strings captured at different adapter layers. '
      + 'No display initialization/frame transport/runtime proof.'
    ),
  }
  (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--oracle', type=Path, required=True)
  parser.add_argument('--library', type=Path, required=True)
  parser.add_argument('--native', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--source', action='store_true')
  args = parser.parse_args()
  args.root, args.oracle, args.library, args.output = (p.resolve() for p in [args.root, args.oracle, args.library, args.output])
  if args.source:
    source(args)
  else:
    assert args.native is not None
    args.native = args.native.resolve()
    compare(args)


if __name__ == '__main__':
  main()
