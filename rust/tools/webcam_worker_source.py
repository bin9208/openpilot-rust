"""Run unchanged Camerad workers against owned file captures and real IPC."""

from __future__ import annotations

import json
import os
from pathlib import Path
import sys
import threading
import time

from openpilot.tools.webcam import camerad
from msgq.visionipc import VisionStreamType


def main(folder: Path, specification: Path) -> None:
  owned = folder.resolve(strict=True)
  assert os.environ['OPENPILOT_PREFIX'] and Path(os.environ['WEBCAM_OWNED_ROOT']).resolve(strict=True) == owned
  specs = json.loads(specification.read_text())
  kinds = {
    'road': ('roadCameraState', VisionStreamType.VISION_STREAM_ROAD),
    'wide': ('wideRoadCameraState', VisionStreamType.VISION_STREAM_WIDE_ROAD),
  }
  cameras = []
  for row in specs:
    path = Path(row['input']).resolve()
    assert path.is_relative_to(owned)
    message, stream = kinds[row['kind']]
    cameras.append(camerad.CameraType(message, stream, str(path)))
  previous = camerad.CAMERAS, camerad.platform.system, camerad.messaging.PubMaster, threading.excepthook
  messages, failures = [], []
  original_master = camerad.messaging.PubMaster

  class ObservedMaster(original_master):
    def send(self, service, data):
      result = super().send(service, data)
      raw = data.to_bytes()
      target = owned / f'{service}-{data.__getattr__(service).frameId}.bin'
      target.write_bytes(raw)
      messages.append({'service': service, 'frame_id': data.__getattr__(service).frameId, 'monotonic': time.monotonic(), 'raw': str(target)})
      return result

  def failure(args):
    failures.append({'thread': args.thread.name, 'type': args.exc_type.__name__, 'error': str(args.exc_value)})
    previous[3](args)

  daemon = None
  try:
    camerad.CAMERAS = cameras
    # The source supports literal file paths through its Darwin selection input;
    # capture, constructor, worker and publication bodies remain unchanged.
    camerad.platform.system = lambda: 'Darwin'
    camerad.messaging.PubMaster = ObservedMaster
    threading.excepthook = failure
    daemon = camerad.Camerad()
    camerad.platform.system = previous[1]
    print('READY', flush=True)
    assert sys.stdin.readline().strip() == 'RUN'
    daemon.run()
    result = {
      'messages': messages,
      'failures': failures,
      'cameras_before_caller_cleanup': [
        {'service': camera.cam_type_state, 'width': camera.W, 'height': camera.H, 'frame_id': camera.cur_frame_id, 'opened': camera.cap.isOpened()}
        for camera in daemon.cameras
      ],
      'source_scope': 'unchanged Camerad constructor/workers/publication and original IPC; Darwin file-path selection input on Linux host',
    }
    (owned / 'worker-result.json').write_text(json.dumps(result, indent=2) + '\n')
  finally:
    camerad.CAMERAS, camerad.platform.system, camerad.messaging.PubMaster, threading.excepthook = previous
    del daemon


if __name__ == '__main__':
  main(*(Path(value) for value in sys.argv[1:]))
