from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def source(args, descriptor, frames, transforms):
  os.environ['DEV'] = 'CPU:LLVM'
  sys.path.insert(0, str(args.runtime))
  import numpy as np
  from tinygrad import Tensor
  from examples.openpilot.compile_warp import NV12Frame, make_frame_prepare

  width, height = descriptor['camera']
  stride, y_height, uv_height = ((width + 127) // 128) * 128, ((height + 31) // 32) * 32, ((height // 2 + 15) // 16) * 16
  frame_size = descriptor['frame_size']
  prepare = make_frame_prepare(NV12Frame(width, height, stride, y_height, uv_height, frame_size), 512, 256)
  input_frames = Tensor(np.frombuffer(frames, dtype=np.uint8).copy().reshape(2, frame_size))
  matrices = Tensor(np.frombuffer(transforms, dtype=np.float32).copy().reshape(2, 3, 3))
  output = Tensor.stack(*(prepare(input_frames[i], matrices[i]) for i in range(2))).numpy().tobytes()
  (args.evidence / 'source.bin').write_bytes(output)
  return {'bytes': len(output), 'sha256': hashlib.sha256(output).hexdigest()}


def native(args):
  from usbgpu_model_fixture import Boundary

  boundary = Boundary()
  command = [
    str(args.binary),
    str(args.firmware),
    str(args.descriptor),
    str(args.evidence / 'frames.bin'),
    str(args.evidence / 'transforms.bin'),
    str(args.evidence / 'native.bin'),
  ]
  with (args.evidence / 'native-stderr.log').open('w') as stderr:
    with subprocess.Popen(
      command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True, env={**os.environ, 'USBGPU_FIXTURE_VRAM_BYTES': str(4 << 30)}
    ) as process:
      done = None
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
  return {'invocation': command, 'exit_code': code, 'result': done, 'kernels': boundary.kernels}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--kind', choices=['source', 'native'], required=True)
  parser.add_argument('--runtime', type=Path)
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--firmware', type=Path)
  parser.add_argument('--descriptor', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(exist_ok=True, parents=True)
  descriptor = json.loads(args.descriptor.read_text())
  if args.kind == 'source':
    import struct

    frames = bytes(i % 251 for i in range(descriptor['frame_size'] * 2))
    transforms = struct.pack('<18f', 1.1, 0.01, 9.25, -0.01, 1.2, 4.5, 0.0001, -0.00002, 1, 1.01, -0.03, -10.25, 0.02, 1.09, -5.5, -0.0001, 0.00002, 1)
    (args.evidence / 'frames.bin').write_bytes(frames)
    (args.evidence / 'transforms.bin').write_bytes(transforms)
    result = source(args, descriptor, frames, transforms)
  else:
    result = native(args)
    result['exact'] = result['exit_code'] == 0 and (args.evidence / 'source.bin').read_bytes() == (args.evidence / 'native.bin').read_bytes()
  (args.evidence / f'{args.kind}.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
  if args.kind == 'native':
    assert result['exit_code'] == 0 and result['exact']


if __name__ == '__main__':
  main()
