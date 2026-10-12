"""Original FFmpeg pipeline and TURZX transport with an owned libusb provider."""

from __future__ import annotations

import ast
import importlib
import json
from pathlib import Path
import struct
import sys
import time
from types import ModuleType


def main() -> None:
  root, binding, library, case_file = map(Path, sys.argv[1:])
  sys.path[:0] = [str(root), str(root / 'rust/tools'), str(root / 'openpilot/selfdrive/carrot/cluster')]
  from original_params_binding import load
  from Crypto.Cipher import DES
  import usb.backend.libusb1
  import usb.util
  from openpilot.common import usbgpu_bus_lock

  params_module, logging = load(binding, 'ipc://' + str(case_file.parent / 'source-log'), case_file.parent / 'logs')
  case = json.loads(case_file.read_text())
  display_module = importlib.import_module('cluster_usb_display')
  backend = usb.backend.libusb1.get_backend(find_library=lambda _: str(library))
  assert backend is not None
  display_module._LIBUSB_BACKEND = backend
  usbgpu_bus_lock.USBGPU_BUS_LOCK_PATH = case['lock']
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
  module = ModuleType('library.lcd.lcd_comm_turing_usb')
  module.__dict__.update(time=time, struct=struct, DES=DES)
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(vendor), 'exec'), module.__dict__)
  sys.modules[module.__name__] = module
  source = importlib.import_module('cluster_h264_pipeline')
  params = params_module.Params()
  display = display_module.TuringUsbDisplay(display_fps=case['fps'])
  devices = []
  find = usb.core.find

  def owned_find(**kwargs):
    device = find(**kwargs)
    if device is not None:
      devices.append(device)
    return device

  usb.core.find = owned_find
  pipeline = None
  try:
    display.set_orientation(2)
    display.open()
    pipeline = source.H264UsbPipeline(
      display,
      case['width'],
      case['height'],
      16,
      case['fps'],
      case['bitrate'],
      case['gop'],
      case.get('backend', 'ffmpeg'),
      'missing-native-provider',
      '/usr/bin/ffmpeg',
      case['encoder'],
      'missing-vidc',
      'auto',
      0,
      'vbr-cfr',
      False,
      case['chunk_size'],
      case['wait_ack'],
      case['soft_ack'],
      case['dump'],
      False,
    )
    pipeline.start()
    pid = pipeline.encoder_pid
    command = pipeline._proc.args
    initial = params.get_bool('ClusterHudConnected')
    display.set_brightness(61)
    display.set_orientation(1)
    data = Path(case['input']).read_bytes()
    size = case['width'] * case['height'] * 4
    assert len(data) % size == 0
    submitted = 0
    for offset in range(0, len(data), size):
      pipeline.submit_rgba(data[offset : offset + size], case['width'], case['height'])
      submitted += 1
    pipeline.close()
    display.close()
    print(
      json.dumps(
        {
          'initial': initial,
          'closed': params.get_bool('ClusterHudConnected'),
          'submitted': submitted,
          'bytes_sent': pipeline._debug_usb_bytes,
          'chunks_sent': pipeline._chunks_sent,
          'stderr': list(pipeline._stderr_tail),
          'error': None if pipeline._error is None else str(pipeline._error),
          'reaped': not Path(f'/proc/{pid}').exists(),
          'command': command,
        }
      )
    )
  finally:
    if pipeline is not None:
      pipeline.close()
    display.close()
    for device in devices:
      usb.util.dispose_resources(device)
    logging.ipchandler.close()


if __name__ == '__main__':
  main()
