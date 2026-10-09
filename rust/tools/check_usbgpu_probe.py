"""Compare the source random/download probe with compiled AMD kernels at owned MMIO."""

from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--firmware', type=Path, required=True)
  parser.add_argument('--descriptor', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  os.environ['DEV'] = 'CPU:LLVM'
  sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
  sys.path.insert(0, str(args.source))
  from tinygrad import Tensor

  Tensor.manual_seed(42)
  source = Tensor.rand(1 << 20).numpy().tobytes()
  (args.evidence / 'source.bin').write_bytes(source)
  from usbgpu_model_fixture import Boundary

  boundary = Boundary()
  command = [str(args.binary), str(args.firmware), str(args.descriptor), str(args.evidence / 'native.bin')]
  done = None
  with (args.evidence / 'stderr.log').open('w') as stderr:
    with subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True,
                          env={**os.environ, 'USBGPU_FIXTURE_VRAM_BYTES': str(4 << 30)}) as child:
      try:
        for line in child.stdout:
          request = json.loads(line)
          if 'done' in request:
            done = request['done']
            break
          child.stdin.write(json.dumps({'value': boundary.call(request)}) + '\n')
          child.stdin.flush()
        child.stdin.close()
        code = child.wait(timeout=10)
      finally:
        if child.poll() is None:
          child.kill()
          child.wait()
        (args.evidence / 'trace.json').write_text(json.dumps(boundary.trace, separators=(',', ':')) + '\n')
  native = (args.evidence / 'native.bin').read_bytes()
  downloads = [event for event in boundary.trace if event['op'] == 'arm_read' and event['size'] == 256 << 10]
  result = {'invocation': command, 'exit_code': code, 'result': done, 'bytes': len(native), 'kernels': boundary.kernels,
            'download_chunks': len(downloads), 'download_bytes': sum(event['size'] for event in downloads),
            'source_sha256': hashlib.sha256(source).hexdigest(), 'native_sha256': hashlib.sha256(native).hexdigest(),
            'exact': source == native, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.evidence / 'comparison.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
  assert code == 0 and done is not None and source == native and boundary.kernels == 2 and len(downloads) == 128


if __name__ == '__main__':
  main()
