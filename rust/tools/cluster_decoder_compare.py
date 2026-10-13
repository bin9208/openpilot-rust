from __future__ import annotations

import ast
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import threading


def original(root):
  import av

  path = root / 'openpilot/selfdrive/carrot/cluster/cluster_navi_source.py'
  tree = ast.parse(path.read_text())
  nodes = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name in ('DecodedH264Frame', 'H264Decoder')]
  scope = {'dataclass': dataclass, 'threading': threading, 'HardwareH264DecoderError': RuntimeError, 'create_tici_h264_decoder': lambda *_: None}
  exec(compile(ast.fix_missing_locations(ast.Module(body=nodes, type_ignores=[])), str(path), 'exec'), scope)
  return scope['H264Decoder'], av


def run(root, binary, output):
  output.mkdir(parents=True, exist_ok=False)
  source, av = original(root)
  expected, steps = [], []
  decoder = source()

  def step(payload, reset=False):
    path = output / f'{len(steps):04}.h264'
    path.write_bytes(payload)
    steps.append({'packet': str(path), 'reset': reset})
    if reset:
      decoder.reset()
    frame = decoder.decode(payload, sequence=len(steps), width=96, height=64)
    expected.append(
      None if frame is None else {'width': frame.width, 'height': frame.height, 'planes': [list(plane) for plane in frame.planes], 'strides': frame.strides}
    )

  for dimensions, pixel_format in [((96, 64), 'yuv420p'), ((96, 64), 'yuv444p'), ((80, 48), 'yuv420p')]:
    width, height = dimensions
    raw = bytearray()
    for frame in range(12):
      for y in range(height):
        for x in range(width):
          raw.extend(((x * 3 + frame * 7) % 256, (y * 4 + frame * 3) % 256, (x + y + frame * 11) % 256, 255))
    raw_path = output / f'input-{width}-{height}-{pixel_format}.rgba'
    raw_path.write_bytes(raw)
    command = [
      'ffmpeg',
      '-hide_banner',
      '-loglevel',
      'error',
      '-f',
      'rawvideo',
      '-pixel_format',
      'rgba',
      '-video_size',
      f'{width}x{height}',
      '-framerate',
      '10',
      '-i',
      str(raw_path),
      '-an',
      '-c:v',
      'libx264',
      '-preset',
      'ultrafast',
      '-tune',
      'zerolatency',
      '-pix_fmt',
      pixel_format,
      '-x264-params',
      'keyint=6:min-keyint=6:scenecut=0',
      '-f',
      'h264',
      'pipe:1',
    ]
    encoded = subprocess.run(command, capture_output=True, timeout=30, check=True)
    (output / f'encode-{width}-{height}-{pixel_format}.json').write_text(
      json.dumps(
        {'command': command, 'exit_code': encoded.returncode, 'stderr': encoded.stderr.decode(), 'stream_sha256': hashlib.sha256(encoded.stdout).hexdigest()}
      )
    )
    codec = av.CodecContext.create('h264', 'r')
    packets = codec.parse(encoded.stdout) + codec.parse(b'')
    for index, packet in enumerate(packets):
      step(bytes(packet), reset=index == 0)
    step(b'malformed access unit')
    step(bytes(packets[0]), reset=True)
  case = output / 'input.json'
  case.write_text(json.dumps(steps))
  completed = subprocess.run([str(binary), str(case)], capture_output=True, text=True, timeout=30)
  (output / 'invocation.json').write_text(
    json.dumps({'command': [str(binary), str(case)], 'exit_code': completed.returncode, 'stdout': completed.stdout, 'stderr': completed.stderr}, indent=2)
  )
  assert completed.returncode == 0, completed.stderr
  actual = json.loads(completed.stdout)
  (output / 'expected.json').write_text(json.dumps(expected))
  (output / 'actual.json').write_text(json.dumps(actual))
  mismatches = []
  decoded_count = 0
  for index, (left, right) in enumerate(zip(expected, actual, strict=True)):
    if left is None or right is None:
      if left != right:
        mismatches.append({'step': index, 'presence': [left is not None, right is not None]})
      continue
    decoded_count += 1
    if (left['width'], left['height'], list(left['strides'])) != (right['width'], right['height'], right['strides']):
      mismatches.append({'step': index, 'layout': [[left['width'], left['height'], left['strides']], [right['width'], right['height'], right['strides']]]})
      continue
    for plane in range(3):
      width = left['width'] if plane == 0 else (left['width'] + 1) // 2
      height = left['height'] if plane == 0 else (left['height'] + 1) // 2
      stride = left['strides'][plane]
      for row in range(height):
        begin = row * stride
        if left['planes'][plane][begin : begin + width] != right['planes'][plane][begin : begin + width]:
          mismatches.append({'step': index, 'plane': plane, 'row': row})
          break
  receipt = {
    'steps': len(steps),
    'decoded_frames': decoded_count,
    'pass': not mismatches,
    'mismatches': mismatches[:10],
    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'source_av': {'version': av.__version__, 'libraries': av.library_versions},
    'scope': 'Original H264Decoder/PyAV vs native FFmpeg; exact visible YUV samples and strides; allocator row padding excluded',
  }
  (output / 'result.json').write_text(json.dumps(receipt, indent=2))
  print(json.dumps(receipt))
  assert not mismatches


if __name__ == '__main__':
  run(*(Path(value).resolve() for value in sys.argv[1:]))
