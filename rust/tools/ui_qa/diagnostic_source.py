import os
from pathlib import Path
import sys
from types import ModuleType, SimpleNamespace

mode = sys.argv[1]
output = Path(sys.argv[2])
os.environ.update(BIG='0', SCALE='1', OFFSCREEN='1')
if mode == 'burn-in':
  os.environ['BURN_IN'] = '1'
hardware = ModuleType('openpilot.system.hardware')
hardware.PC = True
hardware.TICI = False
hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: 'pc')
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://diagnostic-source', swaglog_root=lambda: str(output.parent / 'source-logs'))
sys.modules[paths.__name__] = paths
import pyray as rl
from openpilot.system.ui.lib.application import gui_app, MouseEvent, MousePos

gui_app.init_window('Source diagnostic modes')
if gui_app._render_texture:
  rl.begin_texture_mode(gui_app._render_texture)
else:
  rl.begin_drawing()
rl.clear_background(rl.BLACK)
for x, blue in [(20, 0), (140, 128), (260, 255)]:
  rl.draw_rectangle_rec(rl.Rectangle(x, 20, 110, 140), rl.Color(0, 0, blue, 255))
rl.draw_text_ex(gui_app.font(), 'Native diagnostics', rl.Vector2(30, 175), 32, 0, rl.WHITE)
if gui_app._render_texture:
  rl.end_texture_mode()
  rl.begin_drawing()
  rl.clear_background(rl.BLACK)
  rl.begin_shader_mode(gui_app._burn_in_shader)
  rl.draw_texture_pro(gui_app._render_texture.texture, rl.Rectangle(0, 0, 536, -240), rl.Rectangle(0, 0, 536, 240), rl.Vector2(0, 0), 0, rl.WHITE)
  rl.end_shader_mode()
if mode == 'touches':
  gui_app._mouse_events = [
    MouseEvent(MousePos(70, 70), 0, True, False, True, 0),
    MouseEvent(MousePos(90, 80), 0, False, False, True, 0.1),
    MouseEvent(MousePos(120, 90), 0, False, False, True, 0.2),
  ]
  gui_app._draw_touch_points()
if mode == 'grid':
  gui_app._grid_size = 32
  gui_app._draw_grid()
rl.rl_draw_render_batch_active()
capture = rl.load_image_from_screen()
assert rl.export_image(capture, str(output))
rl.unload_image(capture)
rl.end_drawing()
gui_app.close()
