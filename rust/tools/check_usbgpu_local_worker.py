# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "zstandard==0.25.0"]
# ///
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import struct
import subprocess
from pathlib import Path
from typing import TypedDict

from usbgpu_model_fixture import Boundary
from openpilot.system.camerad.cameras.nv12_info import get_nv12_info


class NativeDone(TypedDict):
  calls: int
  frame_size: int
  camera: list[int]
  nonfinite_rejected: bool


class NativeResult(TypedDict):
  exit: int
  done: NativeDone | None
  kernels: int
  observations: int
  equal: bool
  finite: bool
  binary_sha256: str


def run(binary: Path, model: Path, assets: Path, evidence: Path, camera: tuple[int, int], poison: bool) -> NativeResult:
  sequence = json.loads((evidence / 'sequence.json').read_text())
  observed, index = [], 0
  while index < len(sequence):
    selected = sequence[index]
    index += 1
    if index < len(sequence) and sequence[index]['entrypoint'] == 'policy':
      selected = sequence[index]
      index += 1
    observed.extend(selected['outputs'].values())
  boundary = Boundary('byte-addressed', evidence)
  argv = [str(binary), str(model), str(assets), str(evidence / 'sequence.json'), *map(str, camera)]
  if poison:
    argv.append('--poison')
  done: NativeDone | None = None
  with (
    (evidence / 'native-stderr.log').open('w') as stderr,
    subprocess.Popen(
      argv,
      stdin=subprocess.PIPE,
      stdout=subprocess.PIPE,
      stderr=stderr,
      text=True,
      env={**os.environ, 'USBGPU_FIXTURE_VRAM_BYTES': str(4 << 30)},
    ) as process,
  ):
    try:
      for line in process.stdout:
        request = json.loads(line)
        if 'done' in request:
          done = request['done']
          break
        process.stdin.write(json.dumps({'value': boundary.call(request)}) + '\n')
        process.stdin.flush()
      process.stdin.close()
      code = process.wait(timeout=10)
    finally:
      if process.poll() is None:
        process.kill()
        process.wait()
      (evidence / 'trace.json').write_text(json.dumps(boundary.trace, separators=(',', ':')))
  comparisons = []
  for target in observed:
    expected = (evidence / target.replace('native-', 'source-', 1)).read_bytes()
    actual = (evidence / target).read_bytes() if (evidence / target).exists() else b''
    finite = (
      all(math.isfinite(value) for data in (expected, actual) for (value,) in struct.iter_unpack('<f', data))
      if any(name in target for name in ('-model-', '-feat_q-', '-desire_q-'))
      else True
    )
    comparisons.append(
      {
        'name': target,
        'bytes': len(expected),
        'equal': expected == actual,
        'finite': finite,
        'source_sha256': hashlib.sha256(expected).hexdigest(),
        'native_sha256': hashlib.sha256(actual).hexdigest(),
      }
    )
  layouts = []
  for width, height in ((1344, 760), (1928, 1208)):
    stride, y_height, uv_height, full = get_nv12_info(width, height)
    layouts.append(
      {
        'camera': [width, height],
        'stride': stride,
        'y_offset': 0,
        'y_bytes': stride * y_height,
        'uv_offset': stride * y_height,
        'uv_bytes': stride * uv_height,
        'plane_bytes': stride * (y_height + uv_height),
        'planes': [
          {'name': 'y', 'offset': 0, 'stride': stride, 'height': y_height},
          {'name': 'uv', 'offset': stride * y_height, 'stride': stride, 'height': uv_height},
        ],
        'source_allocation_bytes': full,
        'received_buffer_bytes': (evidence / 'frame-0.bin').stat().st_size if (width, height) == camera else None,
      }
    )
  result: NativeResult = {
    'exit': code,
    'done': done,
    'kernels': boundary.kernels,
    'observations': len(comparisons),
    'equal': bool(comparisons) and all(row['equal'] for row in comparisons),
    'finite': bool(comparisons) and all(row['finite'] for row in comparisons),
    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
  }
  (evidence / 'comparison.json').write_text(json.dumps({'argv': argv, 'result': result, 'allocation': layouts, 'comparisons': comparisons}, indent=2) + '\n')
  assert code == 0 and result['equal'] and result['finite']
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ('binary', 'model', 'assets', 'evidence'):
    parser.add_argument(f'--{name}', type=Path, required=True)
  parser.add_argument('--camera', choices=['1344x760', '1928x1208'], default='1344x760')
  parser.add_argument('--poison', action='store_true')
  args = parser.parse_args()
  width, height = (int(value) for value in args.camera.split('x'))
  print(json.dumps(run(args.binary, args.model, args.assets, args.evidence, (width, height), args.poison)))


if __name__ == '__main__':
  main()
