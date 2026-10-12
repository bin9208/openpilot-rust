# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Compare source helper results and exact OBJ float buffers with a native executable."""

from __future__ import annotations
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess


def mismatch(left, right, path=''):
  if isinstance(left, dict) and isinstance(right, dict):
    if left.keys() != right.keys():
      return {'path': path, 'source': list(left), 'native': list(right)}
    for key in left:
      found = mismatch(left[key], right[key], f'{path}.{key}')
      if found:
        return found
  elif isinstance(left, list) and isinstance(right, list):
    if len(left) != len(right):
      return {'path': path, 'source_length': len(left), 'native_length': len(right)}
    for index, (a, b) in enumerate(zip(left, right, strict=True)):
      found = mismatch(a, b, f'{path}[{index}]')
      if found:
        return found
  elif isinstance(left, float) and isinstance(right, (int, float)):
    if not math.isclose(left, right, rel_tol=1e-14, abs_tol=1e-14):
      return {'path': path, 'source': left, 'native': right}
  elif left != right:
    return {'path': path, 'source': left, 'native': right}
  return None


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('reference', type=Path)
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  reference = json.loads(args.reference.read_text())
  args.output.mkdir(parents=True, exist_ok=True)
  assert hashlib.sha256(Path(reference['source']).read_bytes()).hexdigest() == reference['source_sha256']
  queries = [{key: value for key, value in row.items() if key != 'expected'} for row in reference['rows']]
  command = [str(args.binary.resolve())]
  request = ''.join(json.dumps(row) + '\n' for row in queries)
  result = subprocess.run(command, input=request, text=True, capture_output=True, check=False)
  (args.output / 'native-output.jsonl').write_text(result.stdout)
  (args.output / 'native-stderr.log').write_text(result.stderr)
  assert result.returncode == 0, (result.returncode, result.stderr[-1000:])
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  assert len(actual) == len(queries)
  comparisons = []
  for index, (row, native) in enumerate(zip(reference['rows'], actual, strict=True)):
    expected = row['expected']
    difference = None
    buffers = {}
    if row['op'] == 'obj':
      for name, fmt in [('vertices', 'f'), ('normals', 'f'), ('colors', 'B')]:
        left = struct.pack(f'<{len(expected[name])}{fmt}', *expected[name])
        right = struct.pack(f'<{len(native[name])}{fmt}', *native[name])
        buffers[name] = {'source_sha256': hashlib.sha256(left).hexdigest(), 'native_sha256': hashlib.sha256(right).hexdigest(), 'bytes': len(left)}
        if left != right:
          difference = {
            'buffer': name,
            'first_byte': next((i for i, (a, b) in enumerate(zip(left, right, strict=False)) if a != b), None),
            'source_bytes': len(left),
            'native_bytes': len(right),
          }
          break
    else:
      difference = mismatch(expected, native)
    comparisons.append({'index': index, 'op': row['op'], 'pass': difference is None, 'difference': difference, 'buffers': buffers})
  failures = [row for row in comparisons if not row['pass']]
  receipt = {
    'command': command,
    'exit_code': result.returncode,
    'source_sha256': reference['source_sha256'],
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'total': len(comparisons),
    'passed': len(comparisons) - len(failures),
    'comparisons': comparisons,
  }
  (args.output / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps({'total': len(comparisons), 'passed': receipt['passed'], 'failures': failures[:5]}))
  return int(bool(failures))


if __name__ == '__main__':
  raise SystemExit(main())
