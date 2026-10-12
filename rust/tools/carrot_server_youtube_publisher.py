# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Actual C++msgq/Cereal producer in one caller-owned namespace."""

from __future__ import annotations

import json
import os
from pathlib import Path
import sys
import time


def main() -> None:
  config = json.loads(sys.stdin.readline())
  root = Path(config['owned_root'])
  assert Path(os.environ['PARAMS_ROOT']).is_relative_to(root)
  from carrot_server_dashcam_catalog import source_modules

  source_modules()
  from openpilot.cereal import messaging

  rows = json.loads(Path(config['input']).read_text())
  publisher = messaging.pub_sock('youtubeRoadEncodeData')
  print(json.dumps({'pid': os.getpid(), 'namespace': os.environ['OPENPILOT_PREFIX']}), flush=True)
  sys.stdin.readline()
  started = time.monotonic()
  sent = 0
  fresh_idr_ack = None
  paused = 0.0
  while time.monotonic() - started < config['seconds']:
    row = rows[sent % len(rows)]
    message = messaging.new_message('youtubeRoadEncodeData')
    frame = message.youtubeRoadEncodeData
    frame.header = Path(row['header']).read_bytes()
    frame.data = Path(row['path']).read_bytes()
    frame.width = row['width']
    frame.height = row['height']
    frame.idx.frameId = sent + 1
    frame.idx.flags = 8 if row['keyframe'] else 0
    publisher.send(message.to_bytes())
    if sent == 160:
      assert row['keyframe'] and frame.header
      deadline = time.monotonic() + 1
      held = time.monotonic()
      while not publisher.all_readers_updated() and time.monotonic() < deadline:
        time.sleep(0.005)
      fresh_idr_ack = publisher.all_readers_updated()
      paused = time.monotonic() - held
      assert fresh_idr_ack
    sent += 1
    time.sleep(max(0, started + paused + sent / 20 - time.monotonic()))
  print(
    json.dumps(
      {
        'sent': sent,
        'seconds': time.monotonic() - started,
        'readers_updated': publisher.all_readers_updated(),
        'fresh_idr_reader_ack': fresh_idr_ack,
        'fresh_idr_hold_seconds': paused,
      }
    ),
    flush=True,
  )
  del publisher


if __name__ == '__main__':
  main()
