# /// script
# requires-python = ">=3.12"
# dependencies = ["pygame==2.6.1"]
# ///
# Source process oracle; run with cluster modules on PYTHONPATH and owned last-folder file.
from __future__ import annotations

from dataclasses import asdict
import json
import os
from pathlib import Path
import queue
import sys
import threading
import time

import cluster_replay_tools as original
from cluster_models import RouteOverlay

original.LAST_FOLDER_FILE = Path(os.environ['CLUSTER_REPLAY_ORACLE_LAST_FOLDER'])


def read_input(lines: queue.Queue[str]) -> None:
  for line in sys.stdin:
    lines.put(line)


def main() -> None:
  window = original.RouteReplayToolsWindow()
  print(json.dumps({'started': window._process.pid}), flush=True)
  lines: queue.Queue[str] = queue.Queue()
  threading.Thread(target=read_input, args=(lines,), daemon=True).start()
  try:
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline and window.is_alive:
      while not lines.empty():
        value = json.loads(lines.get_nowait())
        if 'close' in value:
          window.close()
          print(json.dumps({'closed': True, 'exit': window._process.exitcode}), flush=True)
          return
        overlay = RouteOverlay(
          video_rgba=Path(value['video_file']).read_bytes() if value.get('video_file') else None,
          video_width=value.get('video_width', 0),
          video_height=value.get('video_height', 0),
          video_status=value.get('video_status'),
          cutin_status=value.get('cutin_status'),
          data_lines=tuple(value.get('data_lines', ())),
        )
        window.update(
          value.get('playback_s', 0),
          value.get('duration_s', 0),
          value.get('paused', False),
          value.get('status_text', ''),
          overlay,
          value.get('current_log_folder', '--'),
          value.get('options', {}),
        )
      action = window.poll()
      if action is not None:
        print(json.dumps(asdict(action)), flush=True)
      time.sleep(0.005)
    action = window.poll()
    if action is not None:
      print(json.dumps(asdict(action)), flush=True)
    window.close()
    print(json.dumps({'closed': True, 'exit': window._process.exitcode}), flush=True)
  finally:
    window.close()


if __name__ == '__main__':
  main()
