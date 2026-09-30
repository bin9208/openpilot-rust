"""Render unchanged startup widgets with isolated hardware/network/clock/input boundaries."""
import importlib
import json
import os
from pathlib import Path
import sys
from types import ModuleType, SimpleNamespace

scene = json.loads(Path(sys.argv[1]).read_text())
output = Path(sys.argv[2]).resolve()
os.environ['BIG'] = '1' if scene['config']['big'] else '0'
os.environ['SCALE'] = str(scene['config']['scale'])
os.environ['OFFSCREEN'] = '1'
import openpilot.system
hardware = ModuleType('openpilot.system.hardware')
hardware.PC = scene['config']['pc']
hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: 'pc' if hardware.PC else ('tizi' if scene['config']['large_viewport'] else 'mici'), reboot=lambda: None)
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://startup-ui-source', swaglog_root=lambda: str(output.parent / 'source-logs'))
sys.modules[paths.__name__] = paths
import pyray as rl
from openpilot.system.ui.lib.application import gui_app, FontWeight, MouseEvent, MousePos
from openpilot.system.ui.lib.multilang import multilang
multilang._language = scene.get('language', 'en')
gui_app._mouse.start = lambda: None
gui_app._mouse.stop = lambda: None
module = importlib.import_module(f"openpilot.system.ui.{'spinner' if scene['kind'] == 'spinner' else 'text'}")
module.start_ip_monitor = lambda: None
module.label_with_port = lambda _: scene['ip']
gui_app.init_window('Source startup UI', font_weights=(FontWeight.NORMAL, FontWeight.PRETENDARD) if scene['kind'] == 'spinner' else None)
if scene['kind'] == 'spinner':
  widget = module.Spinner()
  for update in scene.get('updates', []):
    widget.set_text(update)
  widget._rotation = 45.0
  rl.get_frame_time = lambda: 0.0
else:
  widget = module.TextWindow(scene.get('text', ''))
# Drive actual shared application renderer, retaining its scale/scissor/font wrappers.
loop = gui_app.render()
for frame in range(3):
  next(loop)
  gui_app._mouse_events = [MouseEvent(MousePos(event['pos']['x'], event['pos']['y']), event['slot'], event['pressed'], event['released'], event['down'], event['time']) for event in scene.get('events', [])] if frame == 0 else []
  rl.get_mouse_wheel_move = lambda: scene.get('wheel', 0.0) if frame == 0 else 0.0
  widget.render(rl.Rectangle(0, 0, gui_app.width, gui_app.height))
rl.rl_draw_render_batch_active()
capture = rl.load_image_from_texture(gui_app._render_texture.texture) if gui_app._render_texture else rl.load_image_from_screen()
if gui_app._render_texture:
  rl.image_flip_vertical(capture)
assert rl.export_image(capture, str(output))
rl.unload_image(capture)
if scene['kind'] == 'spinner':
  state = {'rotation': widget._rotation, 'progress': widget._progress, 'status': widget._status_line, 'lines': widget._wrapped_lines}
else:
  scroll = widget._scroll_panel
  state = {'lines': widget._wrapped_lines, 'offset': scroll.offset, 'velocity': scroll._velocity_filter_y.x}
output.with_suffix('.json').write_text(json.dumps(state, indent=2, ensure_ascii=False))
loop.close()
gui_app.close()
