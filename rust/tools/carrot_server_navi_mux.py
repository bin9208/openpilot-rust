#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_navi_mux.py --video OWNED_H264 --output NEW_DIR [--binary NATIVE_EXAMPLE]
# Caller supplies existing PyAV. Input is a short synthetic H264 stream, never vehicle media.
from __future__ import annotations

import argparse
import base64
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import time
from types import ModuleType
from typing import TypedDict

import av
from carrot_server_dashcam_upload import save
from carrot_server_dashcam_sync_probe import Json


class Frame(TypedDict):
  path: str
  sequence: int
  timestamp_ms: int
  keyframe: bool


class MuxInput(TypedDict):
  config: str
  width: int
  height: int
  session: str
  frames: list[Frame]


class Initialization(TypedDict):
  payload: str
  mime: str
  width: int
  height: int


class Segment(TypedDict):
  payload: str
  sequence: int
  timestamp_ms: int
  duration_ms: int
  keyframe: bool


class MuxRow(TypedDict):
  initialization: Initialization | None
  segments: list[Segment]


def source_module() -> ModuleType:
  path = Path('openpilot/selfdrive/carrot/server/features/carrot_navi/fmp4.py')
  spec = importlib.util.spec_from_file_location('owned_navi_fmp4', path)
  module = importlib.util.module_from_spec(spec)
  sys.modules[spec.name] = module
  spec.loader.exec_module(module)
  return module


def controls(video: Path, output: Path) -> tuple[MuxInput, list[MuxRow]]:
  module = source_module()
  timestamps = (1000, 1200, 1300, 1200, 0, 0, 1600, 2000)
  frames = []
  config = b''
  with av.open(str(video), format='h264') as container:
    for packet in container.demux(video=0):
      if not packet.size:
        continue
      units = module._annex_b_units(bytes(packet))
      headers = [unit for unit in units if unit[0] & 31 in (7, 8)]
      if not config and headers:
        config = b''.join(b'\x00\x00\x00\x01' + unit for unit in headers)
      raw = b''.join(b'\x00\x00\x00\x01' + unit for unit in units if unit[0] & 31 not in (7, 8))
      path = output / f'sample-{len(frames)}.h264'
      path.write_bytes(raw)
      frames.append({'path': str(path), 'sequence': len(frames) + 1, 'timestamp_ms': timestamps[len(frames)], 'keyframe': packet.is_keyframe})
  assert len(frames) == 8 and config
  path = output / 'config.h264'
  path.write_bytes(config)
  inputs = {'config': str(path), 'width': 160, 'height': 96, 'session': 'owned-map', 'frames': frames}
  save(output / 'input.json', inputs)
  muxer = module.CarrotNaviFmp4Muxer()
  muxer.configure(config, 160, 96, 'owned-map')
  rows = []
  try:
    for frame in frames:
      result = muxer.push(Path(frame['path']).read_bytes(), sequence=frame['sequence'], source_timestamp_ms=frame['timestamp_ms'], keyframe=frame['keyframe'])
      init = result.initialization
      rows.append(
        {
          'initialization': None
          if init is None
          else {'payload': base64.b64encode(init.payload).decode(), 'mime': init.mime, 'width': init.width, 'height': init.height},
          'segments': [
            {
              'payload': base64.b64encode(segment.payload).decode(),
              'sequence': segment.sequence,
              'timestamp_ms': segment.source_timestamp_ms,
              'duration_ms': segment.duration_ms,
              'keyframe': segment.keyframe,
            }
            for segment in result.segments
          ],
        }
      )
  finally:
    muxer.close()
  save(output / 'source.json', rows)
  save(
    output / 'provider.json',
    {'PyAV': av.__version__, 'FFmpeg': av.library_versions, 'source_sha256': hashlib.sha256(Path(module.__file__).read_bytes()).hexdigest()},
  )
  return inputs, rows


def metadata(rows: list[MuxRow]) -> Json:
  return [
    {
      'initialization': None if row['initialization'] is None else {k: v for k, v in row['initialization'].items() if k != 'payload'},
      'segments': [{k: v for k, v in segment.items() if k != 'payload'} for segment in row['segments']],
    }
    for row in rows
  ]


def media_payloads(rows: list[MuxRow]) -> list[bytes]:
  module = source_module()
  return [boxed[8:] for row in rows for segment in row['segments'] for name, boxed in module._boxes(base64.b64decode(segment['payload'])) if name == 'mdat']


def decoded(rows: list[MuxRow]) -> Json:
  data = bytearray()
  for row in rows:
    if row['initialization']:
      data.extend(base64.b64decode(row['initialization']['payload']))
    for segment in row['segments']:
      data.extend(base64.b64decode(segment['payload']))
  frames = []
  with av.open(io.BytesIO(data), format='mp4') as container:
    for frame in container.decode(video=0):
      digest = hashlib.sha256()
      for plane in frame.planes:
        raw = bytes(plane)
        for line in range(plane.height):
          digest.update(raw[line * plane.line_size : line * plane.line_size + plane.width])
      frames.append(
        {
          'width': frame.width,
          'height': frame.height,
          'format': frame.format.name,
          'pts': frame.pts,
          'time_base': str(frame.time_base),
          'pixel_sha256': digest.hexdigest(),
        }
      )
  return frames


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--video', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--reuse-source', type=Path)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True)
  if args.reuse_source:
    inputs = json.loads((args.reuse_source / 'input.json').read_text())
    source = json.loads((args.reuse_source / 'source.json').read_text())
    save(output / 'source-reuse.json', {'reference': str(args.reuse_source.resolve()), 'samples': len(inputs['frames'])})
  else:
    inputs, source = controls(args.video.resolve(), output)
  if args.binary:
    binary = args.binary.resolve()
    start = time.monotonic()
    run = subprocess.run([str(binary)], input=json.dumps(inputs) + '\n', text=True, capture_output=True, timeout=10)
    save(
      output / 'native-invocation.json',
      {
        'argv': [str(binary)],
        'input': inputs,
        'exit': run.returncode,
        'seconds': time.monotonic() - start,
        'stdout': run.stdout,
        'stderr': run.stderr,
        'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
      },
    )
    assert run.returncode == 0
    native = json.loads(run.stdout)
    save(output / 'native.json', native)
    assert metadata(source) == metadata(native)
    assert media_payloads(source) == media_payloads(native)
    images = {'source': decoded(source), 'native': decoded(native)}
    save(output / 'decoded.json', images)
    assert images['source'] == images['native'] and len(images['source']) == 7
    save(
      output / 'result.json',
      {
        'samples': 8,
        'metadata_equal': True,
        'mdat_payloads_equal': True,
        'decoded_frames_equal': 7,
        'limits': 'container bytes may differ across FFmpeg ABI; FFmpeg frag_every_frame retains the final pending sample until a subsequent push',
      },
    )
  print(json.dumps({'source_samples': len(inputs['frames']), 'native_compared': args.binary is not None}))


if __name__ == '__main__':
  main()
