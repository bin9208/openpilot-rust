from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from ui_application_qa.qa_shapes import Trace


def peer_read(peer: subprocess.Popen[str], expected: str) -> None:
  assert peer.stdout is not None
  while True:
    line = peer.stdout.readline()
    assert line, peer.poll()
    if not line.startswith(('Starting listener', 'Stopping listener')):
      assert line.strip() == expected, line
      return


def capture(root: Path, binary: Path, peer_binary: Path, scene: Path, output: Path, display: str, frames: int, lane: str) -> list[Trace]:
  with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-augmented148-', dir='/dev/shm') as namespace, tempfile.TemporaryDirectory() as sync:
    environment = dict(
      os.environ,
      DISPLAY=display,
      OFFSCREEN='1',
      UI_CAMERA_SYNC=sync,
      PYTHONPATH=os.environ['UI_MSGQ_PYTHON'] + os.pathsep + str(root),
      OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'),
    )
    with subprocess.Popen([str(peer_binary)], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True) as peer:
      peer_read(peer, 'READY')
      command = (
        [sys.executable, str(root / 'rust/tools/ui_application_qa/product_source.py'), str(scene), str(output)]
        if lane == 'source'
        else [str(binary), str(root), str(scene), str(output)]
      )
      with output.with_suffix('.log').open('w') as log:
        log.write('Invocation: ' + json.dumps(command) + '\n')
        log.flush()
        process = subprocess.Popen(command, env=environment, stdout=log, stderr=subprocess.STDOUT)
        try:
          for frame in range(frames):
            deadline = time.monotonic() + 20
            while not (Path(sync) / f'{frame}.ready').exists():
              assert process.poll() is None, output.with_suffix('.log').read_text()[-5000:]
              assert time.monotonic() < deadline, (lane, frame)
              time.sleep(0.001)
            assert peer.stdin is not None
            peer.stdin.write(f'send {frame + 32}\n')
            peer.stdin.flush()
            peer_read(peer, 'OK')
            (Path(sync) / f'{frame}.allow').write_text('allow')
          assert process.wait(timeout=20) == 0, output.with_suffix('.log').read_text()[-5000:]
        finally:
          if process.poll() is None:
            process.kill()
            process.wait()
          peer.kill()
          peer.wait()
    return json.loads(output.with_suffix('.json').read_text())
