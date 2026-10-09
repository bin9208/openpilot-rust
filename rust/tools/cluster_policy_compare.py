"""Compare the first cluster policy/packet slice with unchanged source functions."""

from __future__ import annotations

import argparse
import ast
import hashlib
import itertools
import json
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace

from Crypto.Cipher import DES


def subset(path: Path, names: set[str], namespace: dict) -> dict:
  tree = ast.parse(path.read_text())
  nodes = [node for node in ast.walk(tree) if isinstance(node, ast.FunctionDef) and node.name in names]
  assert {node.name for node in nodes} == names
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)
  return namespace


def capture_frames(root: Path) -> tuple[list[dict], list]:
  import struct

  cluster = root / 'openpilot/selfdrive/carrot/cluster'
  wall, midnight = 1_790_086_399.999, 1_790_000_000.0
  clock = SimpleNamespace(time=lambda: wall, localtime=lambda: (2026, 10, 10), mktime=lambda _value: midnight)
  vendor = cluster / '.vendor/turing-smart-screen-python-main/library/lcd/lcd_comm_turing_usb.py'
  packets = subset(vendor, {'build_command_packet_header', 'encrypt_with_des', 'encrypt_command_packet'}, {'time': clock, 'struct': struct, 'DES': DES})
  methods = subset(cluster / 'cluster_usb_display.py', {'_build_frame_payload', '_build_h264_chunk_payload'}, {})
  display = SimpleNamespace(
    _build_command_packet_header=packets['build_command_packet_header'],
    _encrypt_command_packet=packets['encrypt_command_packet'],
    _cmd_play_h264_chunk=121,
    _profile_start=lambda: 0.0,
    _profile_add=lambda _name, _start: None,
  )
  requests, expected = [], []
  for command, size, last in itertools.product((101, 102, 121), (0, 1, 7, 8, 500, 65536), (False, True)):
    data = bytes(index % 251 for index in range(size))
    requests.append({'op': 'frame', 'id': command, 'wall': wall, 'midnight': midnight, 'bytes': list(data), 'last': last})
    source = methods['_build_h264_chunk_payload'](display, data, is_last=last) if command == 121 else methods['_build_frame_payload'](display, command, data)
    expected.append(list(source))
  return requests, expected


def capture(root: Path) -> tuple[list[dict], list]:
  cluster = root / 'openpilot/selfdrive/carrot/cluster'
  sys.path[:0] = [str(root), str(cluster)]
  import cluster_config as config

  constants = {name: getattr(config, name) for name in dir(config) if name.startswith(('CLUSTER_', 'H264_'))}
  constants.update(USBGPU_DISPLAY_FPS=5, ENCODER_AUTO=0, ENCODER_JPEG=1, ENCODER_HARDWARE=2, ENCODER_SOFTWARE=3)
  autorun = subset(
    cluster.parent / 'cluster_autorun.py',
    {'_encoder_sequence', '_encoder_args', '_cluster_args', '_decode_uevent', '_parse_hex_int', '_usb_uevent_matches'},
    constants.copy(),
  )
  rates = subset(cluster / 'main.py', {'resolved_usb_display_fps', 'resolved_h264_encoder_fps'}, constants.copy())
  requests, expected = [], []
  encoders = ('auto', 'jpeg', 'hardware', 'software')
  for board, output, active, configured, gpu in itertools.product((False, True), ('usb', 'both', 'window'), range(4), range(4), (False, True)):
    autorun['TICI'] = board
    request = {
      'op': 'run',
      'request': {
        'hud_mode': 1,
        'configured_encoder': encoders[configured],
        'active_encoder': encoders[active],
        'output': output,
        'usbgpu_active': gpu,
      },
      'board': board,
      'orientation': 2,
      'debug': 0,
      'onroad': True,
    }
    requests.append(request)
    expected.append(
      {
        'args': autorun['_cluster_args'](1, configured, active, output, gpu),
        'sequence': autorun['_encoder_sequence'](configured),
        'product': 0x92,
        'orientation': 2,
        'allowed': True,
        'fixed_fps': 5 if gpu else 10,
      }
    )
  payloads = [
    b'add@/devices/pci/usb\0SUBSYSTEM=usb\0PRODUCT=1cbe/92/100\0',
    b'bind@x\0SUBSYSTEM=usb\0PRODUCT=1CBE/0092/1\0',
    b'move@x\0SUBSYSTEM=usb\0ID_VENDOR_ID=1cbe\0ID_MODEL_ID=0092\0',
    b'change@x\0SUBSYSTEM=usb\0DEVTYPE=usb_device\0',
    b'remove@x\0SUBSYSTEM=usb\0PRODUCT=1cbe/92/1\0',
    b'add@x\0SUBSYSTEM=pci\0PRODUCT=1cbe/92/1\0',
    b'add@x\0SUBSYSTEM=usb\0PRODUCT=1cbe/123/1\0',
    b'add@x\0SUBSYSTEM=usb\0PRODUCT=no/92\0DEVTYPE=usb_device\0',
    b'add@x\0SUBSYSTEM=usb\0PRODUCT=broken\0DEVTYPE=usb_device\0',
    b'add@x\0SUBSYSTEM=usb\0ID_VENDOR_ID=_1cbe\0ID_MODEL_ID=92\0DEVTYPE=usb_device\0',
    b'add@x\0SUBSYSTEM=usb\0PRODUCT=0x_1cbe/0x0092/1\0',
    b'add@x\0SUBSYSTEM=usb\0PRODUCT=1_c_be/0_092\0',
    b'add@x\0SUBSYSTEM=usb\0DEVTYPE=usb_device\0BROKEN=\xff\0ACTION=remove\0',
    b'add@x\0SUBSYSTEM=usb\0SUBSYSTEM=usb\0ACTION=change\0PRODUCT=\0DEVTYPE=usb_device\0',
  ]
  for payload, product in itertools.product(payloads, (0x92, 0x123)):
    requests.append({'op': 'uevent', 'payload': list(payload), 'product': product})
    expected.append({'decoded': autorun['_decode_uevent'](payload), 'matched': autorun['_usb_uevent_matches'](payload, product)})
  for target, requested, h264 in itertools.product((0.0, -1.0, 1.5, 2.5, 5.0, 10.5, 29.97, 30.5, 255.5, 1000.0), (None, -3, 0, 60, 300), (False, True)):
    requests.append({'op': 'rate', 'requested': requested, 'h264': h264, 'target': target, 'fallback': 30, 'bitrate': ' AUTO '})
    expected.append(
      {
        'display_fps': rates['resolved_usb_display_fps'](requested, 'h264' if h264 else 'jpeg', target, 30),
        'encoder_fps': rates['resolved_h264_encoder_fps'](target, 30),
        'bitrate': config.resolved_usb_h264_bitrate(' AUTO ', target, 30),
      }
    )
  for mode, speed, wide in itertools.product(range(-1, 6), (-5.0, 0.0, 35.999, 36.0, 36.001, 53.999, 54.0, 54.001, 120.0), (False, True)):
    requests.append({'op': 'camera', 'mode': mode, 'speed': speed, 'wide': wide})
    expected.append({'wide': config.cluster_camera_view_prefers_wide(mode, speed, wide), 'zoom': config.cluster_wide_camera_zoom_factor(speed)})
  vendor = cluster / '.vendor/turing-smart-screen-python-main/library/lcd/lcd_comm_turing_usb.py'
  clock = SimpleNamespace(time=lambda: 0.0, localtime=lambda: (2026, 10, 10), mktime=lambda _value: 0.0)
  import struct

  packets = subset(vendor, {'build_command_packet_header', 'encrypt_with_des', 'encrypt_command_packet'}, {'time': clock, 'struct': struct, 'DES': DES})
  for command, delta in itertools.product((0, 10, 13, 14, 15, 17, 52, 101, 102, 111, 112, 121, 122, 123, 255), (0.0, 0.123, 43200.999999, 86399.999)):
    wall, midnight = 1_790_000_000.0 + delta, 1_790_000_000.0
    clock.time = lambda wall=wall: wall
    clock.mktime = lambda _value, midnight=midnight: midnight
    fields = [(8, 64), (12, -1), (499, 256)]
    requests.append({'op': 'command', 'id': command, 'wall': wall, 'midnight': midnight, 'fields': fields})
    raw = packets['build_command_packet_header'](command)
    for offset, value in fields:
      raw[offset] = value & 0xFF
    expected.append(list(packets['encrypt_command_packet'](raw)))
  frame_requests, frame_expected = capture_frames(root)
  requests.extend(frame_requests)
  expected.extend(frame_expected)
  return requests, expected


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--only', choices=('all', 'frames'), default='all')
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  requests, expected = capture_frames(root) if args.only == 'frames' else capture(root)
  args.output.mkdir(parents=True, exist_ok=False)
  encoded = json.dumps(requests).encode()
  (args.output / 'request.json').write_bytes(encoded)
  (args.output / 'source.json').write_text(json.dumps(expected))
  result = subprocess.run([str(args.binary.resolve())], input=encoded, capture_output=True, timeout=10, check=False)
  (args.output / 'native.json').write_bytes(result.stdout)
  (args.output / 'native.stderr').write_bytes(result.stderr)
  observed = json.loads(result.stdout) if result.returncode == 0 else []
  mismatches = [index for index, (source, native) in enumerate(zip(expected, observed, strict=False)) if source != native]
  report = {
    'argv': [str(args.binary.resolve())],
    'returncode': result.returncode,
    'scenarios': len(requests),
    'source_count': len(expected),
    'native_count': len(observed),
    'mismatches': mismatches,
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'scope': 'unchanged extracted policy/vendor functions, actual cached PyCryptodome; no USB/display/runtime/Params claim',
  }
  (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report, indent=2))
  assert result.returncode == 0 and len(observed) == len(expected) and not mismatches


if __name__ == '__main__':
  main()
