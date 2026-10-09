from __future__ import annotations
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys

import numpy as np
sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from openpilot.selfdrive.modeld.local_gpu_warp import only_sampling_boundary_differences


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  if shutil.disk_usage(args.evidence.parent).free < (25 << 30) + (16 << 20):
    raise OSError('warp validation requires 25 GiB free plus 16 MiB bounded growth')
  args.evidence.mkdir(parents=True, exist_ok=True)
  rows = []
  for width, height in [(1344, 760), (1928, 1208)]:
    stride, y_height, uv_height = ((width + 127) // 128) * 128, ((height + 31) // 32) * 32, ((height // 2 + 15) // 16) * 16
    size = stride * (y_height + uv_height)
    frames = np.zeros(2 * size, dtype=np.uint8)
    frames[110 * stride + 202:110 * stride + 204] = [30, 87]
    frames[stride * y_height + 55 * stride + 202:stride * y_height + 55 * stride + 205:2] = [21, 193]
    filename = f'frames-{width}x{height}.bin'
    (args.evidence / filename).write_bytes(frames.tobytes())
    base = 55 * 256 + 101
    cases = [('exact', .5, []), ('y-boundary', .5, [(base, 30, 87)]),
             ('beyond-boundary', .5003, [(base, 30, 87)]), ('wrong-value', .5, [(base, 31, 87)]),
             ('wrong-camera', .5, [(6 * 128 * 256 + base, 30, 87)]),
             ('uv-boundary', 1., [(4 * 128 * 256 + base, 21, 193)]),
             ('wrong-plane', 1., [(5 * 128 * 256 + base, 21, 193)]),
             ('invalid-projective-denominator', .5, [(base, 30, 87)])]
    for name, shift, differences in cases:
      matrix = np.tile(np.eye(3, dtype=np.float32), (2, 1, 1))
      matrix[:, 0, 2] = shift
      if name == 'invalid-projective-denominator':
        matrix[:, 2, 2] = .25
      actual, expected = np.zeros((2, 6, 128, 256), dtype=np.uint8), np.zeros((2, 6, 128, 256), dtype=np.uint8)
      for index, a, b in differences:
        actual.reshape(-1)[index], expected.reshape(-1)[index] = a, b
      accepted = only_sampling_boundary_differences(actual, expected, frames, 0, size, (stride, y_height, uv_height), (width, height), matrix)
      rows.append({'name': f'{name}-{width}x{height}', 'camera': [width, height], 'frames': filename,
                   'transforms': list(matrix.tobytes()), 'differences': differences, 'expected': bool(accepted)})
  inputs = args.evidence / 'source-cases.json'
  inputs.write_text(json.dumps(rows, indent=2) + '\n')
  command = [str(args.binary), str(inputs), str(args.evidence / 'comparison.json')]
  with (args.evidence / 'native.log').open('w') as output:
    result = subprocess.run(command, stdout=output, stderr=subprocess.STDOUT, timeout=30)
  assert result.returncode == 0
  receipt = json.loads((args.evidence / 'comparison.json').read_text())
  assert receipt['passed'] and len(receipt['rows']) == 16
  print(json.dumps({'invocation': command, 'exit_code': result.returncode, 'cases': 16, 'passed': True}))


if __name__ == '__main__':
  main()
