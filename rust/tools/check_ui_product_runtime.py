#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pillow", "pycapnp", "python-xlib"]
# ///
# ─── How to run ───
# 1. Install uv (if not installed):
#      curl -LsSf https://astral.sh/uv/install.sh | sh
# 2. Run with repository native msgq modules on PYTHONPATH:
#      uv run check_ui_product_runtime.py [ARGS]
# 3. Or use the existing startup-ui Python environment without installing dependencies.
# ──────────────────

from __future__ import annotations

import ast
from dataclasses import dataclass
import json
import os
from pathlib import Path
from queue import Queue
import subprocess
import shutil
import sys
import threading
import time
from collections.abc import Callable
from typing import Final, Unpack

from Xlib import X, display
from Xlib.ext import xtest
from openpilot.cereal import car
from ui_application_qa.runtime_peer import Publisher
from ui_application_qa.qa_shapes import Command, Item

ROOT: Final = Path(__file__).resolve().parents[2]


@dataclass(frozen=True, slots=True)
class Frame:
  draw: int
  message: int
  rendered: bool
  mode: str | None
  sidebar: bool
  scroll: float | None
  plot: bool
  recording: bool
  stack: int
  started: bool
  driver_view: bool
  navigation_y: float | None
  active_kind: str
  active_scroll: float | None
  callback_value: str
  callback_confirmed: bool
  settings_panel: str | None
  active_items: list[Item]

  @classmethod
  def parse(cls, text: str) -> Frame:
    value = json.loads(text)
    root = value['root']
    return cls(
      int(value['draw_frame']),
      int(value['message_frame']),
      bool(value['rendered']),
      root['mode'],
      bool(root['sidebar']),
      root['scroll'],
      bool(root['in_plot_mode']),
      bool(root['recording']),
      int(value['stack']),
      bool(value['started']),
      bool(value['driver_view']),
      value['navigation_y'],
      value['active']['kind'],
      value['active']['scroll'],
      value['callback_value'],
      value['callback_confirmed'],
      root['settings_panel'],
      value['active']['items'],
    )


class Driver:
  def __init__(self, process: subprocess.Popen[str], output: Path) -> None:
    self.process = process
    self.output = output
    self.frames: Queue[Frame] = Queue()
    self.trace: list[Frame] = []
    self.reader = threading.Thread(target=self.read, name='native-frame-reader')
    self.reader.start()

  def read(self) -> None:
    assert self.process.stdout is not None
    with (self.output / 'native.log').open('w') as capture:
      for line in self.process.stdout:
        capture.write(line)
        capture.flush()
        if line.startswith('RUNTIME_FRAME '):
          frame = Frame.parse(line.removeprefix('RUNTIME_FRAME '))
          self.trace.append(frame)
          self.frames.put(frame)

  def wait(self, predicate: Callable[[Frame], bool]) -> Frame:
    deadline = time.monotonic() + 20.0
    while time.monotonic() < deadline:
      frame = self.frames.get(timeout=max(0.01, deadline - time.monotonic()))
      if predicate(frame):
        return frame
    raise TimeoutError('native product UI did not reach the requested observable state')

  def send(self, **command: Unpack[Command]) -> None:
    assert self.process.stdin is not None
    self.process.stdin.write(json.dumps(command) + '\n')
    self.process.stdin.flush()

  def capture(self, name: str) -> None:
    start = self.wait(lambda frame: frame.rendered)
    self.send(capture=name)
    path = self.output / f'{name}.png'
    self.wait(lambda frame: frame.draw > start.draw + 1 and path.is_file())
    assert path.stat().st_size > 0


def seed(path: Path, prefix: str) -> None:
  namespace = path / prefix
  namespace.mkdir(parents=True)
  version = ast.parse((ROOT / 'openpilot/system/version.py').read_text())
  constants = {
    node.target.id: ast.literal_eval(node.value)
    for node in version.body
    if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name) and node.target.id in ['terms_version', 'training_version']
  }
  params = {
    'LanguageSetting': 'en',
    'HasAcceptedTerms': constants['terms_version'],
    'CompletedTrainingVersion': constants['training_version'],
    'IsMetric': '1',
    'PrimeType': '0',
    'GitBranch': 'dev',
    'ShowCustomBrightness': '100',
    'IsOnroad': '0',
  }
  for key, value in params.items():
    (namespace / key).write_text(value)
  car_params = car.CarParams.new_message(maxLateralAccel=3.0, openpilotLongitudinalControl=True)
  (namespace / 'CarParamsPersistent').write_bytes(car_params.to_bytes())


def scenario(binary: Path, output: Path, display_name: str, big: bool, failure: bool, vision_peer: Path) -> None:
  output.mkdir(parents=True, exist_ok=True)
  prefix = f'rust-probe-runtime148-{os.getpid()}-{int(big)}-{int(failure)}'
  namespace = Path('/dev/shm') / f'msgq_{prefix}'
  namespace.mkdir()
  seed(output / 'params', prefix)
  env = dict(os.environ, DISPLAY=display_name, BIG=str(int(big)), SCALE='1', PARAMS_ROOT=str(output / 'params'), OPENPILOT_PREFIX=prefix)
  previous = os.environ.get('OPENPILOT_PREFIX')
  os.environ['OPENPILOT_PREFIX'] = prefix
  publisher = Publisher(vision_peer, env)
  publisher.thread.start()
  assert publisher.started.wait(10), 'native IPC publisher did not initialize'
  with subprocess.Popen(
    [str(binary), str(ROOT), str(output)], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1
  ) as process:
    driver = Driver(process, output)
    connection = display.Display(display_name)
    try:
      driver.wait(lambda frame: frame.draw >= 4 and frame.stack == 1)
      windows = [window for window in connection.screen().root.query_tree().children if window.get_wm_name() == 'Native product UI fixture']
      assert len(windows) == 1
      window = windows[0]
      window.set_input_focus(X.RevertToParent, X.CurrentTime)
      connection.sync()
      maps = Path(f'/proc/{process.pid}/maps').read_text()
      assert 'libpython' not in maps.lower()
      assert Path(f'/proc/{process.pid}/exe').resolve() == binary.resolve()
      (output / 'native-process.json').write_text(
        json.dumps({'pid': process.pid, 'executable': str(binary.resolve()), 'python_library_mapped': False}, indent=2)
      )
      driver.capture('home')
      position = connection.screen().root.translate_coords(window, 150 if big else 32, 90 if big else 214)
      xtest.fake_input(connection, X.MotionNotify, x=position.x, y=position.y)
      connection.sync()
      start = driver.wait(lambda frame: frame.rendered)
      xtest.fake_input(connection, X.ButtonPress, 1)
      connection.sync()
      driver.wait(lambda frame: frame.draw > start.draw + 2)
      xtest.fake_input(connection, X.ButtonRelease, 1)
      connection.sync()
      driver.wait(lambda frame: frame.mode == 'Settings' if big else frame.stack == 2)
      driver.capture('settings-mid-real-click')
      driver.wait(lambda frame: big or frame.navigation_y is not None and abs(frame.navigation_y) < 1.0)
      driver.capture('settings-settled-real-click')
      driver.send(bookmark=True)
      assert publisher.bookmark_received.wait(10), 'native bookmark publication did not arrive'
      start = driver.wait(lambda frame: frame.rendered)
      driver.send(asleep=True)
      paused = driver.wait(lambda frame: not frame.rendered)
      later = driver.wait(lambda frame: not frame.rendered and frame.message > paused.message + 3)
      assert later.draw == paused.draw and later.message > start.message
      driver.send(asleep=False)
      driver.wait(lambda frame: frame.rendered and frame.draw > later.draw)
      publisher.set_onroad(True)
      driver.wait(lambda frame: frame.started)
      driver.wait(lambda frame: frame.mode == 'Onroad' if big else frame.stack == 1 and frame.scroll is not None and frame.scroll < -1000)
      driver.capture('onroad')
      driver.send(params={'ScreenRecord': '1'})
      recording = driver.wait(lambda frame: frame.recording)
      driver.wait(lambda frame: frame.draw >= recording.draw + 9)
      driver.capture('recording')
      if failure:
        driver.send(fail=True)
        process.stdin.close()
        assert process.wait(timeout=15) == 1
      else:
        publisher.record_command('STOP')
        driver.wait(lambda frame: not frame.recording)
        publisher.set_onroad(False)
        driver.wait(lambda frame: not frame.started)
        driver.wait(lambda frame: frame.mode == 'Home' if big else frame.scroll is not None and -550 < frame.scroll < -520)
        driver.capture('returned-home')
        driver.send(close=True)
        process.stdin.close()
        assert process.wait(timeout=15) == 0
      driver.reader.join(timeout=5)
      result = json.loads((output / 'result.json').read_text())
      assert not result['recording'] and result['child'] is None and result['close_error'] is None, result
      assert ('injected runtime failure' in result['error']) if failure else result['error'] is None, result
      videos = list((output / 'videos').glob('*.mp4'))
      assert videos
      encoded_frames = 0
      for video in videos:
        probe = subprocess.run(
          ['ffprobe', '-v', 'error', '-show_entries', 'stream=width,height,nb_frames', '-of', 'json', str(video)], capture_output=True, text=True, check=True
        )
        video.with_suffix('.ffprobe.json').write_text(probe.stdout)
        streams = json.loads(probe.stdout)['streams']
        encoded_frames += sum(int(stream['nb_frames']) for stream in streams)
      assert encoded_frames > 0, 'recording session produced no encoded frames'
      (output / 'paused-proof.json').write_text(
        json.dumps({'before': start.message, 'paused_message': paused.message, 'after_message': later.message, 'draw_frame': paused.draw}, indent=2)
      )
    finally:
      connection.close()
      if process.poll() is None:
        process.kill()
        process.wait()
      publisher.stop.set()
      publisher.thread.join(timeout=10)
      assert not publisher.thread.is_alive() and publisher.completed, 'native IPC publisher failed or did not finish'
      (output / 'ui-debug.json').write_text('[' + ','.join(publisher.debug_records) + ']')
      assert publisher.debug_records, 'native uiDebug publication did not arrive'
      shutil.rmtree(namespace)
      if previous is None:
        os.environ.pop('OPENPILOT_PREFIX', None)
      else:
        os.environ['OPENPILOT_PREFIX'] = previous


def main() -> None:
  binary_text, output_text, display_name, peer_text = sys.argv[1:]
  binary, output = Path(binary_text).resolve(), Path(output_text).resolve()
  vision_peer = Path(peer_text).resolve()
  for big in [False, True]:
    for failure in [False, True]:
      scenario(binary, output / f'{"big" if big else "compact"}-{"failure" if failure else "normal"}', display_name, big, failure, vision_peer)
  (output / 'result.json').write_text(
    json.dumps(
      {'verdict': 'PASS', 'scenarios': 4, 'surface': 'native X11 window, actual cereal IPC, real button press/release, recording and error cleanup'}, indent=2
    )
  )
  print('PASS four native product runtime desktop scenarios')


if __name__ == '__main__':
  main()
