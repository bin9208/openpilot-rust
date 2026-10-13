# /// script
# requires-python = ">=3.12"
# dependencies = ["pillow"]
# ///
# Owned helper/file-dialog lifecycle scenarios, using the real native/source GUI.
from __future__ import annotations
import json
import os
from pathlib import Path
import signal
import sys
import time
from PIL import ImageGrab
from cluster_replay_tools_live import Process
from cluster_replay_tools_x11 import Client


def dialog(root: Path, name: str, argv: list[str], environment: dict[str, str], saved: Path) -> dict:
  process = Process(argv, environment, root / f'{name}.stderr')
  x = Client(':91')
  log = root / 'owned-log/rlog.zst'
  log.parent.mkdir(exist_ok=True)
  log.write_bytes(b'owned chooser recipient; no replay parsing in this scenario')
  try:
    window = x.wait('Carrot Cluster Replay Tools')
    time.sleep(0.3)
    window = x.wait('Carrot Cluster Replay Tools')
    x.click(window, 50, 85)
    chooser = x.wait('Open cluster replay log')
    x.focus(chooser)
    ImageGrab.grab(xdisplay=':91').save(root / f'{name}-chooser.png')
    x.key('Alt_L', True)
    x.press('n')
    x.key('Alt_L', False)
    x.key('Control_L', True)
    x.press('a')
    x.key('Control_L', False)
    x.type_path(str(log))
    x.press('Return')
    action = process.wait(lambda value: value.get('open_path') is not None, 5.0)
    if action['open_path'] != str(log):
      raise RuntimeError(f'wrong selected path: {action}')
    persisted = saved.read_text()
    window = x.wait('Carrot Cluster Replay Tools')
    x.click(window, 50, 85)
    chooser = x.wait('Open cluster replay log')
    x.focus(chooser)
    ImageGrab.grab(xdisplay=':91').save(root / f'{name}-reopened.png')
    x.press('Escape')
    time.sleep(0.1)
    process.finish()
    return {
      'action': action,
      'saved': persisted,
      'child_gone': not Path(f'/proc/{process.initial["started"]}').exists(),
      'returncode': process.process.returncode,
    }
  finally:
    process.abort()
    x.close()


def stopped_close(root: Path, argv: list[str], environment: dict[str, str]) -> dict:
  process = Process(argv, environment, root / 'stopped-close.stderr')
  x = Client(':91')
  try:
    x.wait('Carrot Cluster Replay Tools')
    child = process.initial['started']
    os.kill(child, signal.SIGSTOP)
    start = time.monotonic()
    process.finish()
    elapsed = time.monotonic() - start
    return {
      'elapsed': elapsed,
      'child_gone': not Path(f'/proc/{child}').exists(),
      'directory_clean': not Path(process.initial['directory']).exists(),
      'returncode': process.process.returncode,
      'output': process.all,
    }
  finally:
    process.abort()
    x.close()


def missing(root: Path, native: Path, config: Path, launcher: Path, environment: dict[str, str]) -> dict:
  value = json.loads(config.read_text())
  value['sdl_library'] = str(root / 'unavailable-sdl.so')
  path = root / 'missing-gui.json'
  path.write_text(json.dumps(value))
  process = Process([str(native), 'parent', str(path), str(launcher)], environment, root / 'missing-provider.stderr')
  try:
    closed = process.wait(lambda value: value.get('closed') is True, 4.0)
    process.process.wait(timeout=4)
    return {
      'closed': closed,
      'child_gone': not Path(f'/proc/{process.initial["started"]}').exists(),
      'directory_clean': not Path(process.initial['directory']).exists(),
      'returncode': process.process.returncode,
      'output': process.all,
    }
  finally:
    process.abort()


def main() -> None:
  root = Path(sys.argv[1]).resolve()
  native = Path(sys.argv[2]).resolve()
  config = Path(sys.argv[3]).resolve()
  launcher = Path(sys.argv[4]).resolve()
  environment = dict(
    os.environ,
    DISPLAY=':91',
    SDL_VIDEODRIVER='x11',
    SDL_VIDEO_WINDOW_POS='20,510',
    PYGAME_HIDE_SUPPORT_PROMPT='1',
    CLUSTER_REPLAY_ORACLE_LAST_FOLDER=str(root / 'source-dialog-last-folder'),
  )
  source = dialog(
    root,
    'source-dialog',
    [sys.executable, 'rust/tools/cluster_replay_tools_parent_source.py'],
    environment,
    Path(environment['CLUSTER_REPLAY_ORACLE_LAST_FOLDER']),
  )
  translated = dialog(
    root, 'native-dialog', [str(native), 'parent', str(config), str(launcher)], environment, Path(json.loads(config.read_text())['last_folder_file'])
  )
  stopped = stopped_close(root, [str(native), 'parent', str(config), str(launcher)], environment)
  unavailable = missing(root, native, config, launcher, environment)
  report = {
    'source_dialog': source,
    'native_dialog': translated,
    'stopped_helper_close': stopped,
    'missing_provider': unavailable,
    'all_pass': source == translated
    and stopped['child_gone']
    and stopped['directory_clean']
    and 2.9 <= stopped['elapsed'] < 3.6
    and unavailable['child_gone']
    and unavailable['directory_clean']
    and unavailable['returncode'] == 0,
  }
  (root / 'lifecycle.json').write_text(json.dumps(report, indent=2))
  print(json.dumps(report, indent=2))
  if not report['all_pass']:
    raise SystemExit(1)


if __name__ == '__main__':
  main()
