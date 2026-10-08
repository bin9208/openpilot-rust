#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "opencv-python-headless==4.13.0.92", "pillow==12.3.0"]
# ///
# Run retained reference Python with --binary jpeg_options --output new evidence directory.
from __future__ import annotations

import argparse
import ast
import hashlib
from io import BytesIO
import json
from pathlib import Path
import subprocess
from typing import Protocol
import cv2
import numpy as np
from PIL import Image, __version__ as pillow_version, features, _imaging
from openpilot.selfdrive.carrot.xiaoge.lane_inference import prepare_lane_image
from openpilot.selfdrive.carrot.xiaoge.nv12 import nv12_y_plane, pack_nv12

ROOT = Path(__file__).resolve().parents[2]

class Snapshots(Protocol):
  def _jpeg_from_nv12(self, data: bytes, width: int, height: int, stride: int, uv_offset: int) -> bytes: ...
  def _lane_jpeg_from_nv12(self, data: bytes, width: int, height: int, stride: int) -> bytes: ...


def snapshots(source: Path) -> Snapshots:
  """Execute only the two unchanged source staticmethods with their original dependencies."""
  parsed = ast.parse(source.read_text())
  owner = next(node for node in parsed.body if isinstance(node, ast.ClassDef) and node.name == 'VASMService')
  methods = [node for node in owner.body if isinstance(node, ast.FunctionDef)
    and node.name in ('_jpeg_from_nv12', '_lane_jpeg_from_nv12')]
  assert len(methods) == 2
  wrapper = ast.ClassDef(name='SnapshotOracle', bases=[], keywords=[], body=methods, decorator_list=[], type_params=[])
  module = ast.fix_missing_locations(ast.Module(body=[wrapper], type_ignores=[]))
  namespace = {'cv2': cv2, 'BytesIO': BytesIO, 'Image': Image, 'pack_nv12': pack_nv12,
    'nv12_y_plane': nv12_y_plane, 'prepare_lane_image': prepare_lane_image}
  exec(compile(module, str(source), 'exec'), namespace)
  return namespace['SnapshotOracle']()


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  assert pillow_version == '12.3.0'
  args.output.mkdir()
  source = ROOT / 'openpilot/selfdrive/carrot/xiaoge/v_asm_server.py'
  oracle = snapshots(source)
  rng = np.random.default_rng(20075)
  rows = []
  cv2.setNumThreads(2)
  for index, (width, height, stride, padding) in enumerate([(8, 4, 8, 0), (36, 24, 48, 32), (64, 48, 80, 64), (6, 10, 16, 96)]):
    uv_offset = stride * height + padding
    data = rng.integers(0, 256, uv_offset + stride * height // 2 + 16, dtype=np.uint8).tobytes()
    (args.output / f'frame-{index}-nv12.bin').write_bytes(data)
    rgb = cv2.cvtColor(pack_nv12(data, width, height, stride, uv_offset), cv2.COLOR_YUV2RGB_NV12)
    gray = prepare_lane_image(nv12_y_plane(data, width, height, stride), width, height)
    wide = oracle._jpeg_from_nv12(data, width, height, stride, uv_offset)
    road = oracle._lane_jpeg_from_nv12(data, width, height, stride)
    old = BytesIO()
    Image.fromarray(rgb).save(old, 'JPEG')
    cases = [('legacy75', rgb, 'legacy', 75, old.getvalue()), ('rgb75', rgb, 'rgb', 75, old.getvalue()),
      ('rgb85', rgb, 'rgb', 85, wide), ('gray50', gray, 'gray', 50, road)]
    for suffix, pixels, color, quality, expected in cases:
      name = f'frame-{index}-{suffix}'
      input_path = args.output / (name + '.raw')
      actual_path = args.output / (name + '-actual.jpg')
      expected_path = args.output / (name + '-expected.jpg')
      input_path.write_bytes(pixels.tobytes())
      expected_path.write_bytes(expected)
      h, w = pixels.shape[:2]
      command = [*args.runner, str(args.binary), str(input_path), str(actual_path), str(w), str(h), color, str(quality)]
      result = subprocess.run(command, text=True, capture_output=True, check=False)
      (args.output / (name + '.stderr')).write_text(result.stderr)
      equal = result.returncode == 0 and actual_path.read_bytes() == expected
      rows.append({'name': name, 'argv': command, 'returncode': result.returncode, 'bitwise_equal': equal,
        'expected_bytes': len(expected), 'actual_bytes': actual_path.stat().st_size if actual_path.exists() else None})
      if result.returncode == 0:
        maps = actual_path.with_suffix('.maps.txt').read_text()
        assert 'libpython' not in maps and 'cv2.abi3.so' not in maps
  retained = [path for path in args.output.iterdir() if path.is_file()]
  retained += [source, ROOT / 'openpilot/selfdrive/carrot/xiaoge/nv12.py', ROOT / 'openpilot/selfdrive/carrot/xiaoge/lane_inference.py', args.binary]
  retained += [Path(_imaging.__file__), *Path(cv2.__file__).parent.glob('*.so')]
  receipt = {'status': 'pass' if all(row['bitwise_equal'] for row in rows) else 'fail', 'cases': rows,
    'pillow_version': pillow_version, 'libjpeg_turbo_version': features.version_feature('libjpeg_turbo'),
    'scope': 'actual unchanged original snapshot methods + Pillow defaults; exact legacyRGB75/newRGB85/newGray50 bytes, no device/CPU claim',
    'sha256': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in retained}}
  (args.output / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps({'status': receipt['status'], 'cases': len(rows), 'failures': [row['name'] for row in rows if not row['bitwise_equal']]}))
  raise SystemExit(0 if receipt['status'] == 'pass' else 1)


if __name__ == '__main__':
  main()
