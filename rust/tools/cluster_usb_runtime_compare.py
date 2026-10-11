#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pyzmq", "pyusb==1.3.1", "pycryptodome"]
# ///
# ─── How to run ───
# python3 rust/tools/cluster_usb_runtime_compare.py ROOT BINDING ORACLE LIBRARY NATIVE OUTPUT
"""Compose actual source/native Params and clocks with the owned USB transport recipient."""

from __future__ import annotations

import hashlib
import importlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
from types import ModuleType


def original(root: Path, binding: Path, oracle: Path, library: Path, case_file: Path) -> None:
  sys.path[:0] = [str(root), str(root / 'rust/tools'), str(oracle), str(root / 'openpilot/selfdrive/carrot/cluster')]
  from original_params_binding import load
  import usb.backend.libusb1
  import usb.util
  from openpilot.common import usbgpu_bus_lock

  case = json.loads(case_file.read_text())
  module, logging = load(binding, 'ipc://' + str(case_file.parent / 'source-log'), case_file.parent / 'logs')
  display_module = importlib.import_module('cluster_usb_display')
  backend = usb.backend.libusb1.get_backend(find_library=lambda _: str(library))
  assert backend is not None
  display_module._LIBUSB_BACKEND = backend
  usbgpu_bus_lock.USBGPU_BUS_LOCK_PATH = case['lock']
  import ast
  import time
  from Crypto.Cipher import DES

  vendor = root / 'openpilot/selfdrive/carrot/cluster/.vendor/turing-smart-screen-python-main/library/lcd/lcd_comm_turing_usb.py'
  tree = ast.parse(vendor.read_text())
  names = {'build_command_packet_header', 'encrypt_with_des', 'encrypt_command_packet', 'find_usb_device', 'send_image', 'send_jpeg'}
  assignments = {
    'PRODUCT_ID',
    'CMD_UPLOAD_JPEG',
    'CMD_UPLOAD_PNG',
    'CMD_GET_H264_CHUNK_SIZE',
    'CMD_PLAY_H264_CHUNK',
    'CMD_GET_STREAM_STATUS',
    'CMD_STOP_STREAM',
  }
  nodes = [
    node
    for node in tree.body
    if (isinstance(node, ast.FunctionDef) and node.name in names)
    or (isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id in assignments for target in node.targets))
  ]
  vendor_module = ModuleType('library.lcd.lcd_comm_turing_usb')
  vendor_module.__dict__.update(time=time, struct=struct, DES=DES)
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(vendor), 'exec'), vendor_module.__dict__)
  sys.modules[vendor_module.__name__] = vendor_module
  params = module.Params()
  display = display_module.TuringUsbDisplay(display_fps=5 if case['active'] else 10)
  devices = []
  original_find = usb.core.find

  def retained_find(**kwargs):
    device = original_find(**kwargs)
    if device is not None:
      devices.append(device)
    return device

  usb.core.find = retained_find
  try:
    display.open()
    initial = params.get_bool('ClusterHudConnected')
    try:
      display.send_jpeg(bytes([1, 2, 3]))
      jpeg = {'Ok': None}
    except RuntimeError as error:
      jpeg = {'Err': str(error)}
    after_frame = params.get_bool('ClusterHudConnected')
    size = None
    if case['stream']:
      size = display.start_h264_stream(0)
      display.send_h264_chunk(bytes([0, 0, 1, 65, 12]), wait_for_ack=False, require_ack_response=False)
      display.stop_h264_stream()
    display.close()
    result = {'initial': initial, 'jpeg': jpeg, 'after_frame': after_frame, 'chunk_size': size, 'closed': params.get_bool('ClusterHudConnected')}
  finally:
    for device in devices:
      usb.util.dispose_resources(device)
    devices.clear()
    logging.ipchandler.close()
  print(json.dumps(result))


def payloads(path: Path, bounds: tuple[int, int]) -> list[str]:
  from Crypto.Cipher import DES

  data = path.read_bytes()
  normalized = []
  while data:
    length = struct.unpack('<I', data[:4])[0]
    packet, data = data[4 : 4 + length], data[4 + length :]
    assert len(packet) == length and length >= 512
    plain = bytearray(DES.new(b'slv3tuzx', DES.MODE_CBC, b'slv3tuzx').decrypt(packet[:504]))
    stamp = struct.unpack('<I', plain[4:8])[0]
    assert bounds[0] <= stamp <= bounds[1], (bounds, stamp)
    plain[4:8] = b'\0' * 4
    normalized.append(hashlib.sha256(plain + packet[504:]).hexdigest())
  return normalized


def compare(root: Path, binding: Path, oracle: Path, library: Path, native: Path, output: Path) -> None:
  output.mkdir(parents=True)
  rows = []
  for name, mode, active, loading, stream in [
    ('idle', 'ok', False, False, True),
    ('active', 'ok', True, False, True),
    ('loading-recovery', 'write_error', False, True, False),
    ('disconnect', 'disconnect', False, False, False),
  ]:
    directory = output / name
    values, fingerprints, wires = [], [], []
    for implementation in ['source', 'native']:
      destination = directory / implementation
      params = destination / 'params/fixture'
      params.mkdir(parents=True)
      (params / 'UsbGpuActive').write_bytes(b'1' if active else b'0')
      (params / 'UsbGpuLoading').write_bytes(b'1' if loading else b'0')
      case = {'root': str(params.parent), 'lock': str(destination / 'bus.lock'), 'active': active, 'stream': stream}
      case_file = destination / 'input.json'
      case_file.write_text(json.dumps(case) + '\n')
      env = {
        **os.environ,
        'PARAMS_ROOT': str(params.parent),
        'OPENPILOT_PREFIX': 'fixture',
        'TZ': 'UTC',
        'USB_FIXTURE_MODE': mode,
        'USB_FIXTURE_WIRE': str(destination / 'wire.txt'),
        'USB_FIXTURE_BYTES': str(destination / 'bytes.bin'),
        'USB_FIXTURE_LOG': str(destination / 'libusb.log'),
        'LD_LIBRARY_PATH': str(library.parent),
      }
      command = (
        [sys.executable, str(Path(__file__).resolve()), '--source', str(root), str(binding), str(oracle), str(library), str(case_file)]
        if implementation == 'source'
        else [str(native)]
      )
      before = subprocess.run(
        [sys.executable, str(root / 'rust/tools/cluster_params_runtime_compare.py'), '--clock', str(root)], env=env, capture_output=True, text=True, check=True
      )
      completed = subprocess.run(
        command, input=None if implementation == 'source' else json.dumps(case), env=env, capture_output=True, text=True, timeout=20, check=False
      )
      after = subprocess.run(
        [sys.executable, str(root / 'rust/tools/cluster_params_runtime_compare.py'), '--clock', str(root)], env=env, capture_output=True, text=True, check=True
      )
      bounds = (json.loads(before.stdout), json.loads(after.stdout))
      receipt = {
        'argv': command,
        'input': case,
        'environment': {key: env[key] for key in ['PARAMS_ROOT', 'OPENPILOT_PREFIX', 'TZ', 'USB_FIXTURE_MODE']},
        'returncode': completed.returncode,
        'stdout': completed.stdout,
        'stderr': completed.stderr,
        'clock_bounds': bounds,
      }
      (destination / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
      assert completed.returncode == 0, receipt
      result = json.loads(completed.stdout.splitlines()[-1])
      result['jpeg'] = 'Err' if 'Err' in result['jpeg'] else 'Ok'
      assert result['initial'] is True and result['closed'] is False
      assert (params / 'ClusterHudConnected').read_bytes() == b'0'
      assert (destination / 'libusb.log').read_text().endswith('exit clean\n')
      values.append(result)
      fingerprints.append(payloads(destination / 'bytes.bin', bounds))
      wires.append((destination / 'wire.txt').read_text())
    assert values[0] == values[1] and fingerprints[0] == fingerprints[1] and wires[0] == wires[1], name
    (directory / 'result.json').write_text(
      json.dumps({'source': values[0], 'native': values[1], 'normalized_payload_sha256': fingerprints[0], 'wire': wires[0], 'pass': True}, indent=2) + '\n'
    )
    rows.append({'scenario': name, 'pass': True})
  result = {
    'pass': True,
    'rows': rows,
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    'scope': ('actual original Cython and Rust Params plus real wall clocks drive owned USB ABI; '
              + 'command timestamps separately bracketed, payloads exact after timestamp removal; no physical display/runtime proof'),
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  if sys.argv[1] == '--source':
    original(*(Path(value).resolve() for value in sys.argv[2:]))
  else:
    compare(*(Path(value).resolve() for value in sys.argv[1:]))
