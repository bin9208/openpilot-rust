# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Owned camera/encoder stand-ins: actual VisionIPC and original C++msgq/Cereal."""

from __future__ import annotations

import json
import os
from pathlib import Path
import signal
import sys
import threading
import time
from types import FrameType
from urllib.request import urlopen

from carrot_server_dashcam_catalog import source_modules
from carrot_server_dashcam_upload import save


def main(role: str) -> None:
  config = json.loads(Path(os.environ['OWNED_YOUTUBE_TEST_CONFIG']).read_text())
  root = Path(config['owned_root'])
  assert Path(os.environ['PARAMS_ROOT']).is_relative_to(root)
  assert os.environ['OPENPILOT_PREFIX'] == config['prefix']
  assert os.read(0, 1) == b'', 'owned child stdin must be EOF'
  stop = threading.Event()

  def stopped(_signal: int, _frame: FrameType | None) -> None:
    stop.set()

  signal.signal(signal.SIGTERM, stopped)
  signal.signal(signal.SIGINT, stopped)
  source_modules()
  import msgq

  msgq.__path__.insert(0, config['vision_root'])
  save(root / (role + '-started.json'), {'pid': os.getpid(), 'sid': os.getsid(0), 'stdin_eof': True, 'argv': sys.argv})
  print('[owned-youtube-child] ' + role + ' ready', flush=True)
  if role == 'camerad':
    from msgq.visionipc import VisionIpcServer, VisionStreamType

    server = VisionIpcServer('camerad')
    server.create_buffers(VisionStreamType.VISION_STREAM_ROAD, 2, 16, 16)
    server.start_listener()
    stop.wait(120)
    del server
  else:
    from openpilot.cereal import messaging

    publisher = messaging.pub_sock('youtubeRoadEncodeData')
    frames = json.loads(Path(config['frames']).read_text())
    began = time.monotonic()
    index = 0
    paused = 0.0
    holds = []
    while not stop.is_set() and time.monotonic() - began < 120:
      row = frames[index % len(frames)]
      message = messaging.new_message('youtubeRoadEncodeData')
      frame = message.youtubeRoadEncodeData
      frame.header = Path(row['header']).read_bytes()
      frame.data = Path(row['path']).read_bytes()
      frame.width, frame.height = row['width'], row['height']
      frame.idx.frameId = index + 1
      frame.idx.flags = 8 if row['keyframe'] else 0
      publisher.send(message.to_bytes())
      if index >= 160 and index % 40 == 0:
        held = time.monotonic()
        deadline = held + 1
        acknowledged = False
        acknowledgement_errors = []
        while not stop.is_set() and time.monotonic() < deadline:
          try:
            with urlopen(config['status_url'], timeout=0.25) as response:
              acknowledged = json.loads(response.read()).get('last_frame_id') == index + 1
          except (OSError, ValueError) as error:
            acknowledgement_errors.append(str(error))
          if acknowledged:
            break
          stop.wait(0.01)
        duration = time.monotonic() - held
        paused += duration
        holds.append(
          {
            'frame_id': index + 1,
            'seconds': duration,
            'active_service_ack': acknowledged,
            'cli_subscriber_remains_idle': True,
            'acknowledgement_errors': acknowledgement_errors,
          }
        )
      index += 1
      stop.wait(max(0, began + paused + index / 20 - time.monotonic()))
    del publisher
    save(root / 'encoder-terminal.json', {'frames': index, 'holds': holds, 'seconds': time.monotonic() - began})
  print('[owned-youtube-child] ' + role + ' stopped', flush=True)
  save(root / (role + '-stopped.json'), {'pid': os.getpid(), 'stopped': True})


if __name__ == '__main__':
  raise SystemExit('launched only by the owned executable wrappers')
