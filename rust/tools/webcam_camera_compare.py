"""Compare original Camera pixels and actual owned-file capture against Rust."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

import av
import cv2
import numpy as np

from openpilot.tools.webcam.camera import Camera


def save(path: Path, value: object) -> None:
  path.write_text(json.dumps(value, indent=2) + '\n')


def pixels(binary: Path, output: Path) -> list[dict[str, object]]:
  rows = []
  for width, height in ((2, 2), (18, 10), (1280, 720), (3, 4), (4, 3), (3, 6), (1, 1)):
    for kind in ('black', 'white', 'gradient', 'random'):
      folder = output / f'{width}x{height}-{kind}'
      folder.mkdir()
      if kind in ('black', 'white'):
        frame = np.full((height, width, 3), 255 if kind == 'white' else 0, dtype=np.uint8)
      elif kind == 'gradient':
        frame = np.arange(width * height * 3, dtype=np.uint32).astype(np.uint8).reshape(height, width, 3)
      else:
        frame = np.random.default_rng(width * 1000 + height).integers(0, 256, (height, width, 3), dtype=np.uint8)
      source_error = None
      try:
        expected = Camera.bgr2nv12(cv2.flip(frame, -1)).data.tobytes()
      except (AssertionError, ValueError) as error:
        expected, source_error = None, f'{type(error).__name__}: {error}'
      raw = frame.tobytes()
      (folder / 'input.bgr').write_bytes(raw)
      argv = [str(binary), 'pixels', str(width), str(height)]
      native = subprocess.run(argv, input=raw, capture_output=True, check=False, timeout=10)
      (folder / 'native.nv12').write_bytes(native.stdout)
      row = {
        'case': folder.name,
        'argv': argv,
        'returncode': native.returncode,
        'source_error': source_error,
        'native_error': native.stderr.decode(),
        'match': False,
      }
      if expected is None:
        row['match'] = native.returncode != 0
      else:
        (folder / 'source.nv12').write_bytes(expected)
        row.update(source_sha256=hashlib.sha256(expected).hexdigest(), native_sha256=hashlib.sha256(native.stdout).hexdigest())
        row['match'] = native.returncode == 0 and native.stdout == expected
      save(folder / 'result.json', row)
      rows.append(row)
  return rows


def capture(binary: Path, output: Path) -> dict[str, object]:
  path = output / 'owned.avi'
  with av.open(str(path), 'w', format='avi') as container:
    stream = container.add_stream('rawvideo', rate=25)
    stream.width, stream.height, stream.pix_fmt = 128, 72, 'bgr24'
    for index in range(5):
      array = np.random.default_rng(index).integers(0, 256, (72, 128, 3), dtype=np.uint8)
      frame = av.VideoFrame.from_ndarray(array, format='bgr24')
      for packet in stream.encode(frame):
        container.mux(packet)
    for packet in stream.encode():
      container.mux(packet)
  camera = Camera('roadCameraState', 0, str(path))
  source_info = {'width': camera.W, 'height': camera.H, 'fps': camera.cap.get(cv2.CAP_PROP_FPS)}
  source = list(camera.read_frames())
  source_closed = not camera.cap.isOpened()
  folder = output / 'native-capture'
  argv = [str(binary), 'capture', str(path), str(folder)]
  native = subprocess.run(argv, capture_output=True, check=False, timeout=10)
  (output / 'capture-native.stdout').write_bytes(native.stdout)
  (output / 'capture-native.stderr').write_bytes(native.stderr)
  observed = json.loads(native.stdout) if native.returncode == 0 else None
  for index, data in enumerate(source):
    (output / f'source-{index}.nv12').write_bytes(data)
  match = (
    native.returncode == 0
    and source_closed
    and observed
    == {
      'info': source_info,
      'frames': len(source),
      'opened_after_eof': False,
    }
    and all((folder / f'{index}.nv12').read_bytes() == data for index, data in enumerate(source))
  )
  row = {
    'argv': argv,
    'returncode': native.returncode,
    'source_info': source_info,
    'source_frames': len(source),
    'source_closed_after_eof': source_closed,
    'native': observed,
    'match': match,
  }
  save(output / 'capture-result.json', row)
  return row


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--reserve-mib', type=int, default=32)
  args = parser.parse_args()
  root = args.output.resolve()
  if shutil.disk_usage(root.parent).free < 25 * 1024**3 + args.reserve_mib * 1024**2:
    raise OSError('owned webcam fixtures require the 25 GiB floor plus combined reservations')
  root.mkdir(exist_ok=False)
  rows = pixels(args.binary.resolve(strict=True), root)
  captured = capture(args.binary.resolve(strict=True), root)
  save(root / 'result.json', {'pixels': rows, 'capture': captured})
  print(json.dumps({'pixels': len(rows), 'pixel_failures': [row['case'] for row in rows if not row['match']], 'capture': captured}, indent=2))
  assert all(row['match'] for row in rows), 'source/native pixel mismatch'
  assert captured['match'], 'source/native capture mismatch'


if __name__ == '__main__':
  main()
