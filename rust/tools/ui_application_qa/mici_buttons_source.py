"""Render actual product Mici buttons with the source application and assets."""

import json
import os
from pathlib import Path
import sys
import time
from types import ModuleType, SimpleNamespace

scene = json.loads(Path(sys.argv[1]).read_text())
output = Path(sys.argv[2]).resolve()
os.environ.update(BIG='0', SCALE='1', OFFSCREEN='1')
hardware = ModuleType('openpilot.system.hardware')
hardware.PC = True
hardware.TICI = False
hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: 'pc')
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://mici-buttons-source', swaglog_root=lambda: str(output.parent / 'source-logs'))
sys.modules[paths.__name__] = paths
import pyray as rl
from openpilot.system.ui.lib.application import gui_app, MouseEvent, MousePos
from openpilot.system.ui.lib.multilang import multilang
from openpilot.selfdrive.ui.mici.widgets.button import BigButton, BigCircleButton, BigCircleToggle, BigToggle, BigMultiToggle, GreyBigButton

multilang._language = scene['language']
gui_app.init_window('Source Mici buttons')
now = 0.0
original_monotonic = time.monotonic
time.monotonic = lambda: now
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
icon = gui_app.texture('icons_mici/settings/network/wifi_strength_full.png', 50, 37)
calls = []


def changed(value):
  return calls.append({'toggle': value})


def selected(value):
  return calls.append({'select': value})


kind = scene['kind']
match kind:
  case 'circle':
    widget = BigCircleButton(icon)
  case 'circle-red':
    widget = BigCircleButton(icon, True)
  case 'circle-toggle':
    widget = BigCircleToggle(icon, changed)
  case 'toggle':
    widget = BigToggle(scene['text'], scene['value'], toggle_callback=changed)
  case 'multiple':
    widget = BigMultiToggle(scene['text'], ['First', 'Second', 'Third'], changed, selected)
  case 'grey':
    widget = GreyBigButton(scene['text'], scene['value'])
  case 'button' | 'scroll':
    widget = BigButton(scene['text'], scene['value'], icon, kind == 'scroll')
  case _:
    raise ValueError(kind)
rect = rl.Rectangle((536 - widget.rect.width) / 2, 30, widget.rect.width, widget.rect.height)
widget.set_rect(rect)
widget.set_parent_rect(rect)
if kind != 'grey':
  widget.set_touch_valid_callback(lambda: True)
loop = gui_app.render()
results = []
for index in range(scene['frames']):
  now = index / 20
  next(loop)
  events = []
  for action in scene['actions']:
    if action['frame'] != index:
      continue
    events.extend(action.get('events', []))
    match action.get('operation', ''):
      case 'disable':
        widget.set_enabled(False)
      case 'enable':
        widget.set_enabled(True)
      case 'grow':
        widget.trigger_grow_animation()
      case 'shake':
        widget.trigger_shake()
      case 'rotate':
        widget.set_rotate_icon(True)
      case 'text':
        widget.set_text(action['text'])
      case 'value':
        widget.set_value(action['text'])
      case '':
        pass
      case _:
        raise ValueError(action)
  gui_app._mouse_events = [
    MouseEvent(MousePos(event['pos']['x'], event['pos']['y']), event['slot'], event['pressed'], event['released'], event['down'], event['time'])
    for event in events
  ]
  if gui_app._mouse_events:
    gui_app._last_mouse_event = gui_app._mouse_events[-1]
  rl.get_mouse_position = lambda: rl.Vector2(*gui_app.last_mouse_event.pos)
  widget.set_position(rect.x, rect.y)
  widget.render()
  results.append(
    {
      'state': {
        'checked': getattr(widget, '_checked', None),
        'value': getattr(widget, 'value', ''),
        'scale': widget._scale_filter.x,
        'pressed': widget.is_pressed,
      },
      'calls': list(calls),
    }
  )
rl.rl_draw_render_batch_active()
image = rl.load_image_from_screen()
assert rl.export_image(image, str(output))
rl.unload_image(image)
output.with_suffix('.json').write_text(json.dumps(results))
loop.close()
time.monotonic = original_monotonic
gui_app.close()
