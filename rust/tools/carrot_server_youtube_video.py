# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Short owned H264 input at the real low profile; no vehicle logs or camera devices."""

from __future__ import annotations

import hashlib
from pathlib import Path
import subprocess
from typing import TypedDict

import av
from carrot_server_dashcam_upload import save


class Encoded(TypedDict):
  path: str
  header: str
  keyframe: bool
  width: int
  height: int


def generate(output: Path) -> list[Encoded]:
  output.mkdir()
  path = output / 'low.h264'
  argv = [
    'ffmpeg',
    '-hide_banner',
    '-loglevel',
    'error',
    '-f',
    'lavfi',
    '-i',
    'testsrc2=size=854x480:rate=20',
    '-t',
    '8',
    '-c:v',
    'libx264',
    '-preset',
    'ultrafast',
    '-tune',
    'zerolatency',
    '-pix_fmt',
    'yuv420p',
    '-b:v',
    '750k',
    '-minrate',
    '750k',
    '-maxrate',
    '750k',
    '-bufsize',
    '1500k',
    '-g',
    '40',
    '-keyint_min',
    '40',
    '-sc_threshold',
    '0',
    '-f',
    'h264',
    str(path),
  ]
  result = subprocess.run(argv, capture_output=True, timeout=20)
  save(output / 'encoder-invocation.json', {'argv': argv, 'exit': result.returncode, 'stdout': result.stdout.decode(), 'stderr': result.stderr.decode()})
  assert result.returncode == 0
  from openpilot.selfdrive.carrot.server.services.youtube_h264 import annexb_nalus

  rows: list[Encoded] = []
  with av.open(path, format='h264') as container:
    for packet in container.demux(video=0):
      raw = bytes(packet)
      if not raw:
        continue
      nalus = annexb_nalus(raw)
      keyframe = any(nalu[0] & 31 == 5 for nalu in nalus)
      header = b''.join(b'\x00\x00\x00\x01' + nalu for nalu in nalus if nalu[0] & 31 in (7, 8))
      data = output / f'frame-{len(rows):03}.h264'
      config = output / f'header-{len(rows):03}.h264'
      data.write_bytes(raw)
      config.write_bytes(header)
      rows.append({'path': str(data), 'header': str(config), 'keyframe': keyframe, 'width': 854, 'height': 480})
  assert len(rows) == 160 and rows[0]['keyframe']
  save(output / 'input.json', rows)
  save(
    output / 'source.json',
    {
      'frames': len(rows),
      'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
      'bytes': path.stat().st_size,
      'target': {'width': 854, 'height': 480, 'fps': 20, 'kbps': 750, 'gop_seconds': 2},
    },
  )
  return rows
