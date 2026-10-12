# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Owned readiness providers with real C++ VisionIPC and a loopback listener."""

from __future__ import annotations

import json
import os
from pathlib import Path
import signal
import socket
import sys
import threading
from types import FrameType

from carrot_server_dashcam_upload import save


def main(role: str) -> None:
  config = json.loads(Path(os.environ['OWNED_TERMINAL_CONFIG']).read_text())
  root = Path(config['owned_root']).resolve()
  assert Path(os.environ['PARAMS_ROOT']).is_relative_to(root)
  assert os.environ['OPENPILOT_PREFIX'] == config['prefix']
  assert os.read(0, 1) == b''
  stop = threading.Event()

  def stopped(_number: int, _frame: FrameType | None) -> None:
    stop.set()

  signal.signal(signal.SIGTERM, stopped)
  signal.signal(signal.SIGINT, stopped)
  stat = Path('/proc/self/stat').read_text().rsplit(')', 1)[1].split()
  save(root / (role + '-started.json'), {'pid': os.getpid(), 'sid': os.getsid(0), 'starttime': int(stat[19]), 'stdin_eof': True, 'argv': sys.argv})
  print('[owned-vision] ' + role + ' ready', flush=True)
  if role == 'camerad':
    import msgq

    msgq.__path__.insert(0, config['vision_root'])
    from msgq.visionipc import VisionIpcServer, VisionStreamType

    server = VisionIpcServer('camerad')
    server.create_buffers(VisionStreamType.VISION_STREAM_ROAD, 2, 16, 16)
    server.start_listener()
    stop.wait(120)
    del server
  elif role == 'webrtcd':
    with socket.socket() as listener:
      listener.bind(('127.0.0.1', config['port']))
      listener.listen()
      listener.settimeout(0.2)
      while not stop.is_set():
        try:
          connection, _ = listener.accept()
        except TimeoutError:
          continue
        with connection:
          connection.shutdown(socket.SHUT_RDWR)
  else:
    stop.wait(120)


if __name__ == '__main__':
  main(sys.argv[1])
