# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# FLV tag and decoded frame/audio comparisons across the retained FFmpeg providers.
from __future__ import annotations

import hashlib
import io
from pathlib import Path
import struct
from typing import TypedDict, assert_never

import av
from carrot_server_dashcam_upload import save


class Tag(TypedDict):
  kind: int
  timestamp_ms: int
  payload: str


def tags(data: bytes) -> list[Tag]:
  assert data[:13] == b'FLV\x01\x05\x00\x00\x00\x09\x00\x00\x00\x00'
  rows = []
  offset = 13
  while offset < len(data):
    header = data[offset : offset + 11]
    assert len(header) == 11
    size = int.from_bytes(header[1:4], 'big')
    timestamp = int.from_bytes(bytes((header[7],)) + header[4:7], 'big')
    payload = data[offset + 11 : offset + 11 + size]
    previous = data[offset + 11 + size : offset + 15 + size]
    assert len(payload) == size and int.from_bytes(previous, 'big') == size + 11
    rows.append({'kind': header[0], 'timestamp_ms': timestamp, 'payload': payload.hex()})
    offset += size + 15
  assert offset == len(data)
  return rows


class Video(TypedDict):
  pts: int
  time_base: str
  width: int
  height: int
  sha256: str


class Audio(TypedDict):
  pts: int
  time_base: str
  rate: int
  layout: str
  samples: int
  max_abs: float


class Decoded(TypedDict):
  video: list[Video]
  audio: list[Audio]


def decoded(data: bytes) -> Decoded:
  video = []
  audio = []
  with av.open(io.BytesIO(data), format='flv') as container:
    for packet in container.demux():
      for frame in packet.decode():
        match frame:
          case av.VideoFrame():
            raw = bytearray()
            for plane in frame.planes:
              source = bytes(plane)
              for y in range(plane.height):
                raw.extend(source[y * plane.line_size : y * plane.line_size + plane.width])
            video.append(
              {'pts': frame.pts, 'time_base': str(frame.time_base), 'width': frame.width, 'height': frame.height, 'sha256': hashlib.sha256(raw).hexdigest()}
            )
          case av.AudioFrame():
            assert frame.format.name == 'fltp'
            samples = [value[0] for plane in frame.planes for value in struct.iter_unpack('<f', bytes(plane)[: frame.samples * 4])]
            peak = max(abs(value) for value in samples)
            assert peak < 1e-7
            audio.append(
              {
                'pts': frame.pts,
                'time_base': str(frame.time_base),
                'rate': frame.sample_rate,
                'layout': frame.layout.name,
                'samples': frame.samples,
                'max_abs': peak,
              }
            )
          case unexpected:
            assert_never(unexpected)
  return {'video': video, 'audio': audio}


def compare_flv(output: Path) -> None:
  source = (output / 'source.flv').read_bytes()
  native = (output / 'native.flv').read_bytes()
  source_tags, native_tags = tags(source), tags(native)
  assert [row for row in source_tags if row['kind'] == 9] == [row for row in native_tags if row['kind'] == 9]
  assert [(row['kind'], row['timestamp_ms']) for row in source_tags] == [(row['kind'], row['timestamp_ms']) for row in native_tags]
  source_decode, native_decode = decoded(source), decoded(native)
  assert source_decode['video'] == native_decode['video']
  assert source_decode['audio'] == native_decode['audio']
  save(
    output / 'comparison.json',
    {
      'video_tags_exact': True,
      'all_tag_types_timestamps_exact': True,
      'source_tags': source_tags,
      'native_tags': native_tags,
      'decoded_source': source_decode,
      'decoded_native': native_decode,
    },
  )
