# /// script
# requires-python = ">=3.12"
# dependencies = ["pillow"]
# ///
# Run with source-oracle interpreter: live.py EVIDENCE_DIR FROZEN_NATIVE CONFIG LAUNCHER.
from __future__ import annotations
import json
import os
from pathlib import Path
import queue
import signal
import subprocess
import sys
import threading
import time
from PIL import ImageGrab
from cluster_replay_tools_x11 import Client


class Steps(list):
  def __init__(self, path: Path) -> None:
    super().__init__()
    self.path = path
    self.started = time.monotonic()

  def append(self, value: dict) -> None:
    super().append(value)
    self.path.write_text(json.dumps(self, indent=2))
    print(self.path.name, round(time.monotonic() - self.started, 3), value, flush=True)


class Process:
  def __init__(self, argv: list[str], environment: dict[str, str], log: Path) -> None:
    self.error = log.open('w')
    self.process = subprocess.Popen(argv, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.error, text=True, start_new_session=True)
    self.lines: queue.Queue[dict] = queue.Queue()
    self.all = []
    threading.Thread(target=self.read, daemon=True).start()
    self.initial = self.wait(lambda value: 'started' in value, 5.0)

  def read(self) -> None:
    for line in self.process.stdout:
      value = json.loads(line)
      self.all.append(value)
      self.lines.put(value)

  def send(self, value: dict) -> None:
    self.process.stdin.write(json.dumps(value) + '\n')
    self.process.stdin.flush()

  def wait(self, predicate, timeout: float = 2.0) -> dict:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
      try:
        value = self.lines.get(timeout=0.02)
      except queue.Empty:
        continue
      if predicate(value):
        return value
    raise RuntimeError(f'missing process observable; output={self.all}, code={self.process.poll()}')

  def action(self) -> dict:
    return self.wait(lambda value: 'seek_s' in value)

  def finish(self) -> None:
    self.send({'close': True})
    self.wait(lambda value: value.get('closed') is True, 5.)
    self.process.wait(timeout=5)
    self.error.close()

  def abort(self) -> None:
    if self.process.poll() is None:
      os.killpg(self.process.pid, signal.SIGKILL)
      self.process.wait(timeout=5)
    self.error.close()


def controls(root: Path, name: str, argv: list[str], environment: dict[str, str], state: dict) -> dict:
  process = Process(argv, environment, root / f'{name}.stderr')
  x = Client(':91')
  steps = Steps(root / f'{name}-partial.json')
  try:
    window = x.wait('Carrot Cluster Replay Tools')
    x.focus(window)
    process.send(state)
    time.sleep(0.25)
    window = x.wait('Carrot Cluster Replay Tools')
    x.focus(window)
    screenshot = ImageGrab.grab(bbox=(window.x, window.y, window.x + window.width, window.y + window.height), xdisplay=':91')
    screenshot.save(root / f'{name}-initial.png')
    for label, key in [('left', 'Left'), ('right', 'Right'), ('space', 'space')]:
      x.press(key)
      steps.append({'step': label, 'action': process.action()})
    for label, px, py in [
      ('pause', 270, 85),
      ('previous', 170, 85),
      ('next', 375, 85),
      ('background', 470, 80),
      ('option-on', 30, 401),
      ('option-off', 30, 401),
    ]:
      x.click(window, px, py)
      steps.append({'step': label, 'action': process.action()})
    x.motion(window.x + 24, window.y + 485)
    x.button(True)
    steps.append({'step': 'seek-start', 'action': process.action()})
    x.motion(window.x + 550, window.y + 485)
    steps.append({'step': 'seek-move', 'action': process.action()})
    x.motion(window.x + 1120, window.y + 485)
    time.sleep(0.05)
    x.button(False)
    value = process.action()
    steps.append({'step': 'seek-end', 'action': value})
    time.sleep(0.2)
    while not process.lines.empty():
      process.lines.get_nowait()
    x.key('space', True)
    steps.append({'step': 'held-space', 'action': process.action()})
    time.sleep(0.7)
    x.key('space', False)
    time.sleep(0.1)
    repeats = []
    while not process.lines.empty():
      repeats.append(process.lines.get_nowait())
    if repeats:
      raise RuntimeError(f'repeated space differs from source default: {repeats}')
    window = x.wait('Carrot Cluster Replay Tools')
    x.resize(window, 800, 400)
    time.sleep(0.2)
    window = x.wait('Carrot Cluster Replay Tools')
    x.click(window, 30, 280)
    steps.append({'step': 'resized-option', 'action': process.action()})
    ImageGrab.grab(bbox=(window.x, window.y, window.x + window.width, window.y + window.height), xdisplay=':91').save(root / f'{name}-resized.png')
    x.wm_close(window)
    closed = process.wait(lambda value: value.get('closed') is True, 4.0)
    process.process.wait(timeout=4)
    child = process.initial['started']
    orphan = Path(f'/proc/{child}').exists()
    directory = process.initial.get('directory')
    clean = directory is None or not Path(directory).exists()
    return {
      'steps': steps,
      'closed': closed,
      'returncode': process.process.returncode,
      'child_gone': not orphan,
      'directory_clean': clean,
      'output': process.all,
    }
  finally:
    process.abort()
    x.close()


def main() -> None:
  root = Path(sys.argv[1]).resolve()
  native = Path(sys.argv[2]).resolve()
  config = Path(sys.argv[3]).resolve()
  launcher = Path(sys.argv[4]).resolve()
  state = json.loads((root / 'comparison/cases.json').read_text())[1]['state']
  state.update(playback_s=150.0, duration_s=300.0, options={})
  state['video_file'] = str(root / 'comparison/video-1.rgba')
  environment = dict(
    os.environ,
    DISPLAY=':91',
    SDL_VIDEODRIVER='x11',
    SDL_VIDEO_WINDOW_POS='20,510',
    PYGAME_HIDE_SUPPORT_PROMPT='1',
    CLUSTER_REPLAY_ORACLE_LAST_FOLDER=str(root / 'source-owned-last-folder'),
  )
  original = controls(root, 'source-live', [sys.executable, 'rust/tools/cluster_replay_tools_parent_source.py'], environment, state)
  translated = controls(root, 'native-live', [str(native), 'parent', str(config), str(launcher)], environment, state)
  report = {
    'source': original,
    'native': translated,
    'steps_equal': original['steps'] == translated['steps'],
    'all_pass': original['steps'] == translated['steps']
    and original['child_gone']
    and translated['child_gone']
    and translated['directory_clean']
    and original['returncode'] == translated['returncode'] == 0,
  }
  (root / 'live-controls.json').write_text(json.dumps(report, indent=2))
  print(json.dumps(report, indent=2))
  if not report['all_pass']:
    raise SystemExit(1)


if __name__ == '__main__':
  main()
