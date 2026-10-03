#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pillow", "pycapnp", "python-xlib"]
# ///
# ─── How to run ───
# 1. Install uv (if not installed):
#      curl -LsSf https://astral.sh/uv/install.sh | sh
# 2. Run with repository native msgq modules on PYTHONPATH:
#      uv run check_ui_runtime_pages.py [ARGS]
# 3. Or use the existing startup-ui Python environment without installing dependencies.
# ──────────────────

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from Xlib import X, display
from check_ui_product_runtime import Driver, ROOT, seed
from ui_application_qa.runtime_input import Input
from ui_application_qa.runtime_peer import Publisher
from ui_application_qa.qa_shapes import Pages


def pages(driver: Driver, mouse: Input, big: bool) -> list[str]:
  captured = []
  driver.send(params={'GitBranch': 'carrot-egpu'}, page='device')
  driver.wait(lambda frame: frame.mode == 'Settings' if big else frame.active_kind == 'settings')
  if not big:
    driver.wait(lambda frame: frame.navigation_y is not None and abs(frame.navigation_y) < 1)
  for index, name in enumerate(
    ['device', 'network', 'toggles', 'software', 'firehose', 'developer', 'egpu']
    if big
    else ['toggles', 'network', 'device', 'pairing', 'firehose', 'egpu', 'developer']
  ):
    if big:
      mouse.click(250, 355 + 110 * index)
      driver.wait(lambda frame, name=name: frame.settings_panel == name.title() if name != 'egpu' else frame.settings_panel == 'Egpu')
    else:
      if name == 'pairing':
        continue
      current = mouse.frames()
      for _ in range(12):
        item = current.active_items[index]
        center = float(item['x']) + float(item['width']) / 2
        if 75 < center < 461:
          break
        current = mouse.drag((470, 200), (70, 200)) if center >= 461 else mouse.drag((70, 200), (470, 200))
      else:
        raise AssertionError(('settings card did not enter viewport', name, current))
      mouse.click(round(center), 120)
      driver.wait(lambda frame, name=name: frame.active_kind == name and frame.stack == 3)
      driver.wait(lambda frame: frame.navigation_y is not None and abs(frame.navigation_y) < 1)
    driver.capture(f'page-{name}')
    captured.append(name)
    if not big:
      mouse.dismiss(2)
  if big:
    mouse.click(250, 160)
    driver.wait(lambda frame: frame.mode == 'Home')
  else:
    mouse.dismiss(1)
  return captured


def overlays(driver: Driver, mouse: Input, big: bool) -> list[str]:
  captured = []
  for name in ['web', 'pairing', 'driver', 'regulatory', 'terms', 'training', 'language']:
    if name == 'terms' and big:
      continue
    driver.send(prime=-1 if name == 'pairing' else 0, memory={'NetworkAddress': '192.0.2.148'}, page=name)
    driver.wait(lambda frame: frame.stack >= 2)
    if name == 'driver':
      driver.wait(lambda frame: frame.driver_view)
    mouse.frames(18)
    driver.capture(f'overlay-{name}')
    captured.append(name)
    if name == 'web':
      mouse.click(300 if not big else 1500, 120 if not big else 500)
    elif name == 'pairing' and big:
      mouse.click(110, 110)
    elif name == 'regulatory' and big:
      mouse.click(1750, 950)
    elif name == 'language':
      mouse.click(650 if big else 170, 920 if big else 60)
    elif name == 'training' and big:
      positions = [
        (400, 900),
        (2000, 500),
        (2000, 500),
        (1700, 700),
        (1750, 550),
        (2000, 500),
        (1900, 700),
        (1750, 500),
        (1750, 550),
        (1000, 900),
        (1750, 350),
        (2000, 500),
        (1750, 500),
        (2000, 500),
        (1750, 450),
        (1550, 550),
        (2000, 500),
        (2000, 500),
        (1000, 900),
      ]
      for step, (x, y) in enumerate(positions):
        mouse.click(x, y)
        if step < 18:
          driver.capture(f'training-step-{step + 1:02}')
    elif name == 'driver' and big:
      mouse.click(1000, 500)
    else:
      mouse.dismiss(1)
    driver.wait(lambda frame: frame.stack == 1 and not frame.driver_view)
  return captured


def dialogs(driver: Driver, mouse: Input, big: bool) -> list[str]:
  captured = []
  driver.send(alert='Native alert lifecycle')
  driver.wait(lambda frame: frame.stack == 2)
  mouse.frames(18)
  driver.capture('dialog-alert')
  if big:
    mouse.key('Return')
  else:
    mouse.dismiss(1)
  driver.wait(lambda frame: frame.stack == 1)
  for name in ['confirm', 'select'] if big else ['input', 'slider']:
    driver.send(dialog=name)
    driver.wait(lambda frame, name=name: frame.stack == 2 and frame.active_kind == name)
    mouse.frames(18)
    driver.capture(f'dialog-{name}')
    if name == 'confirm':
      mouse.key('Return')
      driver.wait(lambda frame: frame.stack == 1 and frame.callback_confirmed)
    elif name == 'select':
      mouse.click(1100, 430)
      mouse.click(1650, 920)
      driver.wait(lambda frame: frame.stack == 1 and frame.callback_value == 'beta')
    elif name == 'input':
      mouse.click(50, 40)
      driver.wait(lambda frame: frame.stack == 1 and frame.callback_value == 'a')
    else:
      mouse.drag((480, 120), (35, 120))
      driver.wait(lambda frame: frame.stack == 1 and frame.callback_confirmed)
    captured.append(name)
  driver.send(page='device')
  driver.wait(lambda frame: frame.mode == 'Settings' if big else frame.stack == 2)
  driver.send(timeout=1)
  driver.wait(lambda frame: frame.mode == 'Home' if big else frame.stack == 1)
  driver.capture('interactive-timeout-home')
  return ['alert', *captured, 'interactive-timeout']


def scenario(binary: Path, peer: Path, output: Path, display_name: str, big: bool) -> Pages:
  output.mkdir(parents=True, exist_ok=True)
  prefix = f'rust-probe-runtime-pages148-{os.getpid()}-{int(big)}'
  namespace = Path('/dev/shm') / f'msgq_{prefix}'
  namespace.mkdir()
  seed(output / 'params', prefix)
  previous = os.environ.get('OPENPILOT_PREFIX')
  os.environ['OPENPILOT_PREFIX'] = prefix
  environment = dict(os.environ, DISPLAY=display_name, BIG=str(int(big)), SCALE='1', PARAMS_ROOT=str(output / 'params'))
  publisher = Publisher(peer, environment)
  publisher.thread.start()
  assert publisher.started.wait(10)
  connection = display.Display(display_name)
  with subprocess.Popen(
    [str(binary), str(ROOT), str(output)], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1
  ) as process:
    driver = Driver(process, output)
    try:
      driver.wait(lambda frame: frame.draw >= 4 and frame.stack == 1)
      windows = [window for window in connection.screen().root.query_tree().children if window.get_wm_name() == 'Native product UI fixture']
      assert len(windows) == 1
      window = windows[0]
      window.set_input_focus(X.RevertToParent, X.CurrentTime)
      connection.sync()
      mouse = Input(connection, window, driver)
      result = {'settings': pages(driver, mouse, big), 'overlays': overlays(driver, mouse, big), 'dialogs': dialogs(driver, mouse, big)}
      driver.send(close=True)
      assert process.stdin is not None
      process.stdin.close()
      assert process.wait(timeout=15) == 0
      driver.reader.join(timeout=5)
      final = json.loads((output / 'result.json').read_text())
      assert final['error'] is None and final['close_error'] is None and final['child'] is None, final
      result['frames'] = len(driver.trace)
      return result
    finally:
      connection.close()
      if process.poll() is None:
        process.kill()
        process.wait()
      publisher.stop.set()
      publisher.thread.join(timeout=10)
      assert not publisher.thread.is_alive() and publisher.completed, 'native IPC publisher failed or did not finish'
      shutil.rmtree(namespace)
      if previous is None:
        os.environ.pop('OPENPILOT_PREFIX', None)
      else:
        os.environ['OPENPILOT_PREFIX'] = previous


def main() -> None:
  binary_text, output_text, display_name, peer_text = sys.argv[1:]
  binary, output, peer = Path(binary_text).resolve(), Path(output_text).resolve(), Path(peer_text).resolve()
  result = {label: scenario(binary, peer, output / label, display_name, big) for label, big in [('compact', False), ('big', True)]}
  result['verdict'] = 'PASS'
  result['binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
  (output / 'pages-result.json').write_text(json.dumps(result, indent=2))
  print('PASS native root pages, overlays, dialogs, callbacks and interactive timeout through real XTest input')


if __name__ == '__main__':
  main()
