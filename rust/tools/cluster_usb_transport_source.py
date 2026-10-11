#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["pyusb==1.3.1", "pycryptodome"]
# ///
# ─── How to run ───
# python3 rust/tools/cluster_usb_transport_source.py ROOT ORACLE LIBRARY CASE
# ORACLE contains the pinned PyUSB provider; other source imports use existing cached providers.
"""Drive unchanged original display methods through an owned libusb recipient."""

from __future__ import annotations

import ast
import gc
import importlib
import json
from pathlib import Path
import struct
import sys
import time
from types import ModuleType, SimpleNamespace


def source(root: Path, oracle: Path, library: Path, case_path: Path) -> None:
  sys.path[:0] = [str(oracle), str(root / 'openpilot/selfdrive/carrot/cluster'), str(root)]
  from Crypto.Cipher import DES
  import usb.backend.libusb1
  import usb.util
  from openpilot.common import usbgpu_bus_lock

  case = json.loads(case_path.read_text())
  display_module = importlib.import_module('cluster_usb_display')
  usbgpu_bus_lock.USBGPU_BUS_LOCK_PATH = case['lock']
  backend = usb.backend.libusb1.get_backend(find_library=lambda _: str(library))
  assert backend is not None
  display_module._LIBUSB_BACKEND = backend
  devices = []
  original_find = usb.core.find

  def retained_find(**kwargs):
    device = original_find(**kwargs)
    if device is not None:
      devices.append(device)
    return device

  usb.core.find = retained_find
  trace = {'gpu_active': case.get('gpu_active', False), 'reset_protected': case.get('reset_protected', False), 'sleeps_ms': [], 'connected': []}
  display_module._usbgpu_transfer_active = lambda: trace['gpu_active']
  display_module._usbgpu_reset_protected = lambda: trace['reset_protected']
  display_module._set_cluster_hud_connected = trace['connected'].append
  display_module.time = SimpleNamespace(sleep=lambda seconds: trace['sleeps_ms'].append(round(seconds * 1000)), perf_counter=time.perf_counter)
  vendor = root / 'openpilot/selfdrive/carrot/cluster/.vendor/turing-smart-screen-python-main/library/lcd/lcd_comm_turing_usb.py'
  tree = ast.parse(vendor.read_text())
  names = {'build_command_packet_header', 'encrypt_with_des', 'encrypt_command_packet', 'find_usb_device', 'send_image', 'send_jpeg'}
  nodes = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
  assert {node.name for node in nodes} == names
  namespace = {
    'time': SimpleNamespace(time=lambda: 1_790_000_043.21, localtime=lambda: (2026, 10, 10), mktime=lambda _: 1_790_000_000.0),
    'struct': struct,
    'DES': DES,
  }
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(vendor), 'exec'), namespace)
  module = ModuleType('library.lcd.lcd_comm_turing_usb')
  module.__dict__.update(namespace)
  constants = [
    node
    for node in tree.body
    if isinstance(node, ast.Assign)
    and any(
      isinstance(t, ast.Name)
      and t.id
      in {'PRODUCT_ID', 'CMD_UPLOAD_JPEG', 'CMD_UPLOAD_PNG', 'CMD_GET_H264_CHUNK_SIZE', 'CMD_PLAY_H264_CHUNK', 'CMD_GET_STREAM_STATUS', 'CMD_STOP_STREAM'}
      for t in node.targets
    )
  ]
  exec(compile(ast.Module(body=constants, type_ignores=[]), str(vendor), 'exec'), module.__dict__)
  sys.modules[module.__name__] = module
  display = display_module.TuringUsbDisplay(**case.get('options', {}))
  if 'profile_enabled' in case:
    display.set_profile_enabled(case['profile_enabled'])
  results = []
  try:
    for item in case['actions']:
      try:
        match item['op']:
          case 'open':
            value = display.open()
          case 'close':
            value = display.close()
          case 'jpeg':
            value = display.send_jpeg(bytes(item['bytes']))
          case 'png':
            value = display.send_png(bytes(item['bytes']))
          case 'brightness':
            value = display.set_brightness(item['value'], force=item['force'])
          case 'orientation':
            value = display.set_orientation(item['value'], force=item['force'])
          case 'fps':
            value = display.set_display_fps(item['value'], force=item['force'])
          case 'start_h264':
            value = display.start_h264_stream(item['requested'])
          case 'chunk':
            value = display.send_h264_chunk(bytes(item['bytes']), is_last=item['last'], wait_for_ack=item['wait'], require_ack_response=item['require'])
          case 'stop_h264':
            value = display.stop_h264_stream()
          case 'profile_enabled':
            value = display.set_profile_enabled(item['value'])
          case 'clear_profile':
            value = display.clear_profile_samples()
          case _:
            raise AssertionError(item)
        results.append({'Ok': value})
      except (RuntimeError, ValueError, usb.core.USBError) as exc:
        results.append({'Err': str(exc)})
    dimensions = [display.landscape_width, display.landscape_height]
    display.close()
  finally:
    # PyUSB's device resource manager owns handles even after disconnect clears display.dev.
    display._ep_out = display._ep_in = display.dev = None
    for device in devices:
      usb.util.dispose_resources(device)
    if devices:
      del device
    devices.clear()
    gc.collect()
    backend.finalize()
  print(json.dumps({'results': results, 'dimensions': dimensions, 'runtime': trace, 'profiles': display.profile_samples()}))


if __name__ == '__main__':
  source(*(Path(value).resolve() for value in sys.argv[1:]))
