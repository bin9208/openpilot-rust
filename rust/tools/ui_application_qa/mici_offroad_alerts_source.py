# /// script
# dependencies = ["raylib"]
# ///
# How to run: DISPLAY=:127 <existing-ui-python> mici_offroad_alerts_source.py SCENE OUTPUT
"""Render unchanged MiciOffroadAlerts with isolated Params, clock and reboot effects."""
import json
import os
from dataclasses import asdict
from pathlib import Path
import sys
import time
from types import ModuleType, SimpleNamespace

scene = json.loads(Path(sys.argv[1]).read_text())
output = Path(sys.argv[2]).resolve()
root = Path(__file__).resolve().parents[3]
os.environ.update(BIG='0', SCALE=str(scene['config']['scale']), OFFSCREEN='1')
effects: list[str] = []
hardware = ModuleType('openpilot.system.hardware')
hardware.PC = True
hardware.TICI = False
hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: 'pc', reboot=lambda: effects.append('reboot'))
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://mici-offroad-alerts-source', swaglog_root=lambda: str(output.parent / 'logs'))
sys.modules[paths.__name__] = paths
params_module = ModuleType('openpilot.common.params')


class Params:
  def get(self, key: str):
    value = scene.get('params', {}).get(key)
    if key.startswith('Offroad_') and value:
      try:
        return json.loads(value)
      except json.JSONDecodeError:
        return None
    return value

  def get_bool(self, key: str) -> bool:
    return self.get(key) == '1'


params_module.Params = Params
sys.modules[params_module.__name__] = params_module
manager = ModuleType('openpilot.selfdrive.selfdrived.alertmanager')
manager.OFFROAD_ALERTS = json.loads((root / 'openpilot/selfdrive/selfdrived/alerts_offroad.json').read_text())
sys.modules[manager.__name__] = manager
import pyray as rl
from openpilot.system.ui.lib.application import gui_app, MouseEvent, MousePos
from openpilot.system.ui.lib.multilang import multilang
from openpilot.selfdrive.ui.mici.layouts.offroad_alerts import MiciOffroadAlerts

multilang._language = scene['language']
multilang.setup()
gui_app.init_window('Source Mici offroad alerts')
now = 0.0
time.monotonic = lambda: now
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
widget = MiciOffroadAlerts()
gui_app.push_widget(widget)
events = []
gui_app._mouse._handle_mouse_event = lambda: None
gui_app._mouse.get_events = lambda: events
loop = gui_app.render()
results = []
try:
  for index in range(scene['frames']):
    step = next((step for step in scene['steps'] if step['frame'] == index), {})
    now = step.get('now', 0.0 if index == 0 else now + 0.05)
    scene['params'].update(step.get('params', {}))
    for key in step.get('remove', []):
      scene['params'].pop(key, None)
    events = [MouseEvent(MousePos(e['pos']['x'], e['pos']['y']), e['slot'], e['pressed'], e['released'], e['down'], e['time']) for e in step.get('events', [])]
    if 'offset' in step:
      widget._scroller.scroll_panel.set_offset(step['offset'])
    if step.get('refresh', False):
      widget.refresh()
    if step.get('show', False):
      widget.hide_event()
      widget.show_event()
    next(loop)
    items = []
    for item in widget.alert_items:
      r = item.rect
      items.append({'data': asdict(item.alert_data), 'rect': {'x': r.x, 'y': r.y, 'width': r.width, 'height': r.height}, 'size': int(item._alert_size), 'title': item._title_text, 'body': item._body_text, 'pressed': item.is_pressed, 'visible': item.is_visible})
    results.append({'active': widget.active_alerts(), 'scrolling': widget.scrolling(), 'offset': widget._scroller._scroll_offset, 'content': widget._scroller._content_size, 'effects': effects.copy(), 'items': items})
    if index in scene['capture_frames']:
      rl.rl_draw_render_batch_active()
      image = rl.load_image_from_screen()
      assert rl.export_image(image, str(output.with_name(f'{output.stem}-{index}.png')))
      rl.unload_image(image)
  output.with_suffix('.json').write_text(json.dumps(results))
  loop.close()
finally:
  gui_app.close()
