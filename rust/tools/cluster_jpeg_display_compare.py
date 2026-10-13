# Run: source interpreter cluster_jpeg_display_compare.py ROOT NATIVE LIBUSB OUTPUT PYUSB_ORACLE.
"""Compare source codec/display packets with the native worker-owned composition."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def main() -> None:
  root, native, library, output, oracle = (Path(value).resolve() for value in sys.argv[1:])
  output.mkdir(parents=True, exist_ok=True)
  sys.path[:0] = [str(oracle), str(root), str(root / 'openpilot/selfdrive/carrot/cluster')]
  import cluster_usb_display

  records = []
  for width, height in [(63, 35), (1920, 480)]:
    case_dir = output / f'{width}x{height}'
    case_dir.mkdir()
    rgba = bytes((n * 29 + n // 4 * 19) % 256 for n in range(width * height * 4))
    pixels = case_dir / 'frame.rgba'
    pixels.write_bytes(rgba)
    display = cluster_usb_display.TuringUsbDisplay(jpeg_quality=82, jpeg_encoder='pillow')
    jpeg = display.encode_jpeg(rgba, width, height)
    (case_dir / 'source.jpg').write_bytes(jpeg)
    case = {
      'lock': str(case_dir / 'source.lock'),
      'actions': [
        {'op': 'open'},
        {'op': 'brightness', 'value': 77, 'force': False},
        {'op': 'orientation', 'value': 2, 'force': False},
        {'op': 'fps', 'value': 10, 'force': False},
        {'op': 'jpeg', 'bytes': list(jpeg)},
        {'op': 'close'},
      ],
    }
    case_path = case_dir / 'source-case.json'
    case_path.write_text(json.dumps(case))
    invocations = []
    for implementation in ['source', 'native']:
      directory = case_dir / implementation
      directory.mkdir()
      env = {
        **os.environ,
        'LD_LIBRARY_PATH': str(library.parent),
        'USB_FIXTURE_MODE': 'ok',
        'USB_FIXTURE_LOG': str(directory / 'libusb.log'),
        'USB_FIXTURE_WIRE': str(directory / 'wire.txt'),
        'USB_FIXTURE_BYTES': str(directory / 'bytes.bin'),
      }
      (directory / 'wire.txt').write_bytes(b'')
      (directory / 'bytes.bin').write_bytes(b'')
      argv = [sys.executable, str(root / 'rust/tools/cluster_usb_transport_source.py'), str(root), str(oracle), str(library), str(case_path)]
      query = None
      if implementation == 'native':
        argv = [str(native)]
        query = json.dumps({'lock': str(case_dir / 'native.lock'), 'pixels': str(pixels), 'width': width, 'height': height})
      run = subprocess.run(argv, input=query, env=env, capture_output=True, text=True, timeout=20)
      invocation = {'argv': argv, 'exit_code': run.returncode, 'stdout': run.stdout, 'stderr': run.stderr}
      (directory / 'invocation.json').write_text(json.dumps(invocation, indent=2) + '\n')
      assert run.returncode == 0, invocation
      assert (directory / 'libusb.log').read_text().endswith('exit clean\n')
      invocations.append(json.loads(run.stdout.splitlines()[-1]))
    actual = invocations[1]
    assert actual['dimensions'] == invocations[0]['dimensions'] == [1920, 462]
    assert actual['closed'] is True
    assert 'usb_async.encode_jpeg' in actual['stages'] and 'usb_async.send_jpeg' in actual['stages']
    source_bytes = (case_dir / 'source/bytes.bin').read_bytes()
    native_bytes = (case_dir / 'native/bytes.bin').read_bytes()
    assert source_bytes == native_bytes
    assert (case_dir / 'source/wire.txt').read_bytes() == (case_dir / 'native/wire.txt').read_bytes()
    records.append({'width': width, 'height': height, 'packet_bytes': len(source_bytes), 'sha256': hashlib.sha256(source_bytes).hexdigest(), 'native': actual})
  result = {
    'all_equal': True,
    'records': records,
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    'libusb_sha256': hashlib.sha256(library.read_bytes()).hexdigest(),
    'scope': 'owned libusb recipient; no hardware',
  }
  (output / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
