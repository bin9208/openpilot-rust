#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Caller supplies original PyAV/dependencies and retained synthetic H264 input; no keys or recipients are used.
from __future__ import annotations

import argparse
import base64
from datetime import datetime
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid
from collections.abc import Callable
from typing import TypedDict

import av
from carrot_server_dashcam_upload import save
from carrot_server_dashcam_sync_probe import Json
from carrot_server_youtube_decode import compare_flv


class Captured(TypedDict):
  bytes: str | None
  error: str | None


class AccessCaptured(Captured):
  nal_types: list[int] | None


class Frame(TypedDict):
  path: str
  keyframe: bool
  timestamp_ms: int | None


def captured(callback: Callable[[], bytes]) -> Captured:
  try:
    return {'bytes': base64.b64encode(callback()).decode(), 'error': None}
  except (ValueError, OverflowError) as error:
    return {'bytes': None, 'error': str(error)}


def pure(output: Path, header: bytes) -> tuple[dict[str, Json], dict[str, Json]]:
  from openpilot.selfdrive.carrot.server.services import youtube_h264 as h264
  from openpilot.selfdrive.carrot.server.services.youtube_live_captions import Cea608TimestampInjector
  from openpilot.selfdrive.carrot.server.services.youtube_profiles import youtube_profile

  start = b'\x00\x00\x00\x01'
  idr = start + b'\x65\x88\x84'
  predicted = start + b'\x41\x9a\x22'
  configs = [
    header,
    h264.avc_decoder_configuration(header),
    b'',
    start + b'\x67\x64\x00\x1f',
    b'\x01',
    b'\x01\x64\x00\x1f\xfc\xe1\x00',
    b'\x01\x64\x00\x1f\xff\xe0\x00',
  ]
  access = [
    idr,
    predicted,
    start + b'\x65\x88\x00\x00',
    b'\x00\x00\x00\x03\x65\x88\x84',
    b'',
    b'not-h264',
    b'\x00\x00\x00',
    b'\x00\x00\x00\x00',
    start + b'\x00\x00',
  ]
  starts = [(header, idr), (header, predicted), (b'', idr), (header, b'bad')]
  qualities = [-1, 0, 1, 2, 3, 4, 999]
  text = '2026-10-09 12:00:00'
  captions = [
    {'payload': base64.b64encode(idr).decode(), 'enabled': enabled, 'text': stamp, 'reset': reset}
    for enabled, stamp, reset in [
      (False, text, False),
      (True, text, False),
      (True, text, False),
      (True, '2026-10-09 12:00:01', False),
      (False, text, False),
      (True, text, True),
    ]
  ]
  selected = []
  for quality in qualities:
    profile = youtube_profile(quality)
    selected.append(
      {
        'quality': profile.quality,
        'label': profile.label,
        'source': profile.source,
        'process': profile.process_name,
        'encoder_flag': profile.encoder_flag,
        'target': profile.target,
      }
    )
  normalized = []
  for payload in access:
    row = captured(lambda payload=payload: h264.normalize_access_unit(payload).avcc)
    normalized.append(AccessCaptured(**row, nal_types=list(h264.nal_unit_types(payload)) if row['error'] is None else None))
  start_results = []
  for config, payload in starts:
    try:
      h264.validate_stream_start(config, payload)
      start_results.append(None)
    except (ValueError, OverflowError) as error:
      start_results.append(str(error))
  injector = Cea608TimestampInjector()
  injected = []
  for row in captions:
    if row['reset']:
      injector.reset()
    raw = injector.inject(base64.b64decode(row['payload']), enabled=row['enabled'], now=datetime.fromisoformat(row['text']))
    injected.append({'bytes': base64.b64encode(raw).decode(), 'packets': injector.packets_injected})
  input_value = {
    'mode': 'pure',
    'configs': [base64.b64encode(row).decode() for row in configs],
    'access': [base64.b64encode(row).decode() for row in access],
    'starts': [{'header': base64.b64encode(config).decode(), 'payload': base64.b64encode(payload).decode()} for config, payload in starts],
    'qualities': qualities,
    'captions': captions,
  }
  expected = {
    'configs': [captured(lambda config=config: h264.avc_decoder_configuration(config)) for config in configs],
    'access': normalized,
    'starts': start_results,
    'profiles': selected,
    'captions': injected,
  }
  save(output / 'pure-input.json', input_value)
  save(output / 'pure-source.json', expected)
  return input_value, expected


def source_mux(output: Path, retained: Path, custom: bool) -> dict[str, Json]:
  from openpilot.selfdrive.carrot.server.services.youtube_live_muxer import H264FlvMuxer

  input_value = json.loads((retained / 'input.json').read_text())
  sink = io.BytesIO()
  muxer = H264FlvMuxer(sink, codec_header=Path(input_value['config']).read_bytes(), fps=20, width=160, height=96)
  timestamps = [None, 0, 50, 20, 300, None, 500, 501] if custom else [None] * 8
  frames: list[Frame] = []
  try:
    for row, timestamp in zip(input_value['frames'], timestamps, strict=True):
      raw = Path(row['path']).read_bytes()
      muxer.mux(raw, keyframe=row['keyframe'], timestamp_ms=timestamp)
      frames.append({'path': row['path'], 'keyframe': row['keyframe'], 'timestamp_ms': timestamp})
  finally:
    muxer.close()
  (output / 'source.flv').write_bytes(sink.getvalue())
  result = {'mode': 'mux', 'header': input_value['config'], 'fps': 20, 'frames': frames, 'output': str(output / 'native.flv')}
  save(output / 'mux-input.json', result)
  return result


def native(binary: Path, input_value: dict[str, Json], output: Path) -> Json:
  start = time.monotonic()
  result = subprocess.run([str(binary)], input=(json.dumps(input_value) + '\n').encode(), capture_output=True, timeout=10)
  save(
    output / 'native-invocation.json',
    {
      'argv': [str(binary)],
      'input': input_value,
      'exit': result.returncode,
      'seconds': time.monotonic() - start,
      'stdout': result.stdout.decode(),
      'stderr': result.stderr.decode(),
    },
  )
  assert result.returncode == 0
  return json.loads(result.stdout)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--retained-mux', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True)
  params = output / 'owned-params'
  namespace = 'youtube-media-' + uuid.uuid4().hex
  (params / namespace).mkdir(parents=True)
  os.environ.update(PARAMS_ROOT=str(params), OPENPILOT_PREFIX=namespace, CARROT_DATA_DIR=str(output / 'owned-data'))
  assert Path(os.environ['PARAMS_ROOT']).resolve().is_relative_to(output)
  input_value = json.loads((args.retained_mux / 'input.json').read_text())
  save(
    output / 'invocation.json',
    {
      'argv': [sys.executable, '-P', *sys.argv],
      'PYTHONPATH': os.environ.get('PYTHONPATH', ''),
      'providers': {'PyAV': av.__version__, 'FFmpeg': av.library_versions},
      'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest() if args.binary else None,
      'owned_params_root': str(params),
      'network_recipients': [],
    },
  )
  control, expected = pure(output, Path(input_value['config']).read_bytes())
  if args.binary:
    actual = native(args.binary.resolve(), control, output)
    save(output / 'pure-native.json', actual)
    assert actual == expected
  for name, custom in [('cfr', False), ('explicit-timestamps', True)]:
    directory = output / name
    directory.mkdir()
    input_value = source_mux(directory, args.retained_mux.resolve(), custom)
    if args.binary:
      native(args.binary.resolve(), input_value, directory)
      compare_flv(directory)
  save(output / 'result.json', {'pure_controls': 33, 'flv_cases': 2, 'native_compared': bool(args.binary)})


if __name__ == '__main__':
  main()
