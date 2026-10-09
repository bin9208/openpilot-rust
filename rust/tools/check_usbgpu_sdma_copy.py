from __future__ import annotations
import argparse
import ctypes
import hashlib
import json
from pathlib import Path
import struct
import sys
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'tinygrad_repo'))
from test.mockgpu.amd.amdgpu import SDMAExecutor
from usbgpu_sdma_copy import copy


class Gpu:
  def __init__(self):
    self.data = bytearray(1 << 20)
    self.host = (ctypes.c_ubyte * len(self.data)).from_buffer(self.data)

  def translate_addr(self, address):
    group, offset = divmod(address, 1 << 20)
    physical = offset if offset < 65536 else offset + 65536
    return ctypes.addressof(self.host) + (0 if group == 1 else 1 << 19) + physical


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  size, source, destination = 100352, 1 << 20, 2 << 20
  expected = bytes(i % 251 for i in range(size))
  rows = []
  for name, operation in [('original', SDMAExecutor._execute_copy), ('page-aware', copy)]:
    gpu = Gpu()
    for offset in range(0, size, 4096):
      data = expected[offset:offset + 4096]
      ctypes.memmove(gpu.translate_addr(source + offset), data, len(data))
    packet = ctypes.create_string_buffer(struct.pack('<IIIQQ', 1, size - 1, 0, source, destination))
    executor = SimpleNamespace(base=ctypes.addressof(packet), rptr=[0], size=28, gpu=gpu)
    operation(executor)
    observed = b''.join(ctypes.string_at(gpu.translate_addr(destination + offset), min(4096, size - offset))
                        for offset in range(0, size, 4096))
    (args.evidence / f'{name}.bin').write_bytes(observed)
    rows.append({'implementation': name, 'bytes': size, 'matches': observed == expected,
                 'first_difference': next((i for i, pair in enumerate(zip(observed, expected, strict=True)) if pair[0] != pair[1]), None),
                 'sha256': hashlib.sha256(observed).hexdigest(), 'rptr': executor.rptr[0]})
  result = {'scenario': 'Actual SDMA copy function across a fragmented virtual range', 'rows': rows,
            'passed': not rows[0]['matches'] and rows[1]['matches'] and all(row['rptr'] == 28 for row in rows)}
  (args.evidence / 'comparison.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
  assert result['passed']


if __name__ == '__main__':
  main()
