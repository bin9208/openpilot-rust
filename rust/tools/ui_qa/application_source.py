"""Actual GuiApplication diagnostics and lifecycle fixture."""

import json
import os
from pathlib import Path
import sys
from types import ModuleType, SimpleNamespace

mode = sys.argv[1]
output = Path(sys.argv[2]).resolve()
os.environ['BIG'] = '0'
os.environ['SCALE'] = '1'
hardware = ModuleType('openpilot.system.hardware')
hardware.PC = True
hardware.TICI = False
hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: 'pc')
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://application-source', swaglog_root=lambda: str(output.parent / 'source-logs'))
sys.modules[paths.__name__] = paths
import pyray as rl
from openpilot.system.ui.lib.application import gui_app
from openpilot.system.ui.widgets import Widget
from openpilot.system.ui.widgets.inputbox import InputBox
from openpilot.system.ui.widgets.button import Button


class Demo(Widget):
  def __init__(self):
    super().__init__()
    self.input = InputBox()
    self.button = Button('Close', gui_app.request_close, font_size=30)

  def _render(self, rect):
    self.input.render(rl.Rectangle(20, 30, 496, 100))
    self.button.render(rl.Rectangle(190, 160, 160, 60))


ticks = 0
skipped = 0
demo = None


def tick():
  global ticks
  ticks += 1


try:
  gui_app.init_window('Source framework lifecycle')
  demo = Demo()
  gui_app.push_widget(demo)
  gui_app.add_nav_stack_tick(tick)
  gui_app.add_nav_stack_tick(tick)
  if mode == 'dynamic':
    gui_app._record_dir = output.with_suffix('.videos')
    gui_app.start_recording()
    gui_app.start_recording()
  if mode == 'paused':
    gui_app.set_should_render(False)
  for rendered in gui_app.render():
    if not rendered:
      skipped += 1
      if skipped >= 2:
        gui_app.set_should_render(True)
      continue
    rl.rl_draw_render_batch_active()
    capture = rl.load_image_from_texture(gui_app._render_texture.texture) if gui_app._render_texture else rl.load_image_from_screen()
    if gui_app._render_texture:
      rl.image_flip_vertical(capture)
    assert rl.export_image(capture, str(output))
    rl.unload_image(capture)
    if mode == 'paused' and gui_app.frame >= 3:
      gui_app.request_close()
except SystemExit as error:
  assert error.code == 0
finally:
  if mode == 'dynamic':
    gui_app.stop_recording()
    gui_app.stop_recording()
  gui_app.close_ffmpeg()
  output.with_suffix('.json').write_text(json.dumps({'frames': gui_app.frame, 'ticks': ticks, 'skipped': skipped, 'text': demo.input.text if demo else ''}))
  gui_app.close()
