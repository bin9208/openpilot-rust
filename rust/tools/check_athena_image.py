#!/usr/bin/env python3
import argparse
import ast
import hashlib
import io
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
import numpy as np
from PIL import Image, JpegImagePlugin
from athena_reference import ROOT


def snapshot_source():
  tree = ast.parse((ROOT / 'openpilot/system/camerad/snapshot.py').read_text())
  nodes = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in ['yuv_to_rgb', 'extract_image', 'jpeg_write']]
  scope = {'np': np, 'Image': Image}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), 'openpilot/system/camerad/snapshot.py', 'exec'), scope)
  return scope


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  source = snapshot_source()
  rng = np.random.default_rng(146)
  cases = []
  for width, height, stride in [(8, 4, 16), (16, 16, 32), (64, 50, 80), (128, 64, 160), (512, 512, 512)]:
    offset = stride * height
    data = rng.integers(0, 256, offset + stride * (((height // 2 + 15) // 16) * 16), dtype=np.uint8)
    if width == 512:
      y = data[:offset].reshape(height, stride)
      y[::2, ::2] = 0
      y[::2, 1::2] = 127
      y[1::2, ::2] = 128
      y[1::2, 1::2] = 255
      uv = data[offset:].reshape(height // 2, stride)
      uv[:, ::2] = np.arange(256)[:, None]
      uv[:, 1::2] = np.arange(256)[None, :]
    cases.append({'width': width, 'height': height, 'stride': stride, 'uv_offset': offset, 'data': data.tolist()})
  process = subprocess.run([args.binary], input=''.join(json.dumps(row) + '\n' for row in cases), text=True, capture_output=True, check=True)
  native = [json.loads(line) for line in process.stdout.splitlines()]
  records = []
  for index, (row, actual) in enumerate(zip(cases, native, strict=True)):
    expected = source['extract_image'](SimpleNamespace(**row))
    rgb = np.array(actual['rgb'], dtype=np.uint8).reshape(expected.shape)
    original = io.BytesIO()
    source['jpeg_write'](original, expected)
    original = original.getvalue()
    encoded = bytes(actual['jpeg'])
    (args.output / f'{index}-source.jpg').write_bytes(original)
    (args.output / f'{index}-native.jpg').write_bytes(encoded)
    left = Image.open(io.BytesIO(original))
    right = Image.open(io.BytesIO(encoded))
    left.load()
    right.load()
    difference = np.abs(np.array(left, dtype=np.int16) - np.array(right, dtype=np.int16))
    records.append({'case': index, 'layout': {key: value for key, value in row.items() if key != 'data'}, 'rgb_sha256': hashlib.sha256(rgb.tobytes()).hexdigest(), 'source_rgb_sha256': hashlib.sha256(expected.tobytes()).hexdigest(), 'rgb_exact': np.array_equal(rgb, expected), 'jpeg_tables_equal': left.quantization == right.quantization, 'sampling': [JpegImagePlugin.get_sampling(left), JpegImagePlugin.get_sampling(right)], 'dimensions': [left.size, right.size], 'jpeg_bytes_equal': original == encoded, 'decoded_mean_absolute_difference': float(difference.mean()), 'decoded_max_absolute_difference': int(difference.max())})
  (args.output / 'result.json').write_text(json.dumps(records, indent=2) + '\n')
  assert all(row['rgb_exact'] and row['jpeg_bytes_equal'] and row['jpeg_tables_equal'] and row['sampling'] == [2, 2] and row['dimensions'][0] == row['dimensions'][1] for row in records), records
  print('PASS: five exact source NV12/RGB and byte-identical Pillow JPEG cases, including all 65536 UV pairs at four Y levels')


if __name__ == '__main__':
  main()
