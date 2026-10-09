# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Small actual Cereal inputs; H264 samples come from the already compared remux corpus.
from __future__ import annotations

import json
from pathlib import Path

from carrot_server_live_cases import Frame, event


def state(output: Path) -> Frame:
  return event(
    output,
    'carrotNavi',
    {
      'schemaVersion': 1,
      'generation': 21,
      'sessionId': 'owned-map',
      'connected': True,
      'publishMonoTimeNanos': 1_000_000_003,
      'vehicle': {'roadName': 'owned 한글 road', 'latitude': 37.5, 'longitude': 127.0, 'speedKph': 35.5},
      'guidanceCurrent': {'mainText': 'owned left', 'distanceM': 42, 'turnType': 2},
      'laneCurrent': {'count': 2, 'turnInfo': [1, 2]},
      'navigationStatus': {'guidanceActive': True, 'routePresent': True},
      'route': {'polyline': [{'latitude': 37.5, 'longitude': 127.0}]},
    },
    'state',
  )


def media(
  output: Path,
  tag: str,
  kind: str,
  message_type: int,
  raw: bytes,
  sequence: int = 1,
  timestamp: int = 1000,
  keyframe: bool = False,
  session: str = 'owned-map',
  name: str = 'map_main',
  present: bool = True,
) -> Frame:
  return event(
    output,
    'carrotNaviMedia',
    {
      'schemaVersion': 1,
      'sessionId': session,
      'kind': kind,
      'name': name,
      'sequence': sequence,
      'sourceTimestampMillis': timestamp,
      'receivedMonoTimeNanos': 1_000_000_004,
      'present': present,
      'messageType': message_type,
      'formatOrReason': 1,
      'flags': int(keyframe),
      'width': 160,
      'height': 96,
      'reason': 'owned clear' if not present else '',
      'payload': raw,
    },
    tag,
  )


def map_frames(output: Path, retained: Path) -> tuple[Frame, list[Frame]]:
  config = json.loads((retained / 'input.json').read_text())
  header = media(output, 'map-config', 'web_render', 2, Path(config['config']).read_bytes())
  samples = [
    media(output, f'map-frame-{index}', 'web_render', 3, Path(row['path']).read_bytes(), row['sequence'], row['timestamp_ms'], row['keyframe'])
    for index, row in enumerate(config['frames'])
  ]
  return header, samples
