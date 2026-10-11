# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "zstandard==0.25.0"]
# ///
# Run with PYTHONPATH=.:tinygrad_repo:rust/tools uv run rust/tools/check_usbgpu_local_graph.py --help
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import struct
import subprocess
import time
from pathlib import Path


def native(args: argparse.Namespace) -> dict[str, str | int | float | bool]:
  from usbgpu_model_fixture import Boundary

  boundary = Boundary('byte-addressed', args.evidence)
  argv = [str(args.binary), str(args.firmware), str(args.evidence / 'bundle'), str(args.evidence / 'sequence.json')]
  started = time.monotonic()
  done = None
  with (
    (args.evidence / 'native-stderr.log').open('w') as stderr,
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
      (args.evidence / 'trace.json').write_text(json.dumps(boundary.trace, separators=(',', ':')))
  comparisons = []
  for source in sorted(args.evidence.glob('source-*.bin')):
    target = args.evidence / source.name.replace('source-', 'native-', 1)
    expected, actual = source.read_bytes(), target.read_bytes() if target.exists() else b''
    comparisons.append(
      {
        'name': source.name,
        'bytes': len(expected),
        'source_sha256': hashlib.sha256(expected).hexdigest(),
        'native_sha256': hashlib.sha256(actual).hexdigest(),
        'equal': expected == actual,
        'finite': all(math.isfinite(value) for data in (expected, actual) for (value,) in struct.iter_unpack('<f', data))
        if any(name in source.name for name in ('-model-', '-feat_q-', '-desire_q-'))
        else True,
      }
    )
  result = {
    'exit': code,
    'calls': 0 if done is None else done['calls'],
    'seconds': time.monotonic() - started,
    'kernels': boundary.kernels,
    'equal': bool(comparisons) and all(row['equal'] for row in comparisons),
    'finite': bool(comparisons) and all(row['finite'] for row in comparisons),
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
  }
  (args.evidence / 'comparison.json').write_text(json.dumps({'argv': argv, 'result': result, 'comparisons': comparisons}, indent=2) + '\n')
  assert result['exit'] == 0 and result['equal'] and result['finite']
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--kind', choices=['capture', 'source', 'native'], required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--firmware', type=Path)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  match args.kind:  # MATCH_OK: The CLI boundary rejects unknown external mode strings.
    case 'capture' | 'source':
      os.environ.update(DEV='NULL:AMDLLVM:gfx1200' if args.kind == 'capture' else 'CPU:LLVM', JIT='2', BEAM='0', LRU='0')
      from usbgpu_local_source import run

      run(args.evidence, capture=args.kind == 'capture')
    case 'native':
      print(json.dumps(native(args)))
    case _:
      parser.error('unsupported fixture mode')


if __name__ == '__main__':
  main()
