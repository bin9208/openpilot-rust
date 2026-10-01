"""Actual-source oracle for the shared form widgets; no runtime replacement."""

import json
import os
from pathlib import Path
import sys
import time
from types import ModuleType, SimpleNamespace

scene = json.loads(Path(sys.argv[1]).read_text())
output = Path(sys.argv[2]).resolve()
os.environ['BIG'] = '1' if scene['config']['big'] else '0'
os.environ['SCALE'] = str(scene['config']['scale'])
os.environ['OFFSCREEN'] = '1'
hardware = ModuleType('openpilot.system.hardware')
hardware.PC = True
hardware.TICI = scene['config']['large_viewport']
hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: 'pc')
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://ui-forms-source', swaglog_root=lambda: str(output.parent / 'source-logs'))
sys.modules[paths.__name__] = paths
import pyray as rl
from openpilot.system.ui.lib.application import gui_app, MouseEvent, MousePos
from openpilot.system.ui.lib.multilang import multilang
from openpilot.system.ui.widgets.inputbox import InputBox
from openpilot.system.ui.widgets.html_render import HtmlRenderer, ElementType
from openpilot.system.ui.widgets.keyboard import Keyboard
from openpilot.system.ui.widgets.confirm_dialog import ConfirmDialog
from openpilot.system.ui.widgets.option_dialog import MultiOptionDialog
from openpilot.system.ui.widgets.list_view import ListItem, ToggleAction, ButtonAction, TextAction, MultipleButtonAction
from openpilot.system.ui.widgets.slider import LargerSlider
from openpilot.system.ui.widgets.mici_keyboard import MiciKeyboard

multilang._language = scene.get('language', 'en')
gui_app.init_window('Source forms')
p = scene.get('props', {})
gui_app.set_show_touches(p.get('show_touches', False))
kind = scene['kind']
text = scene.get('text', '')
now = 0.0
pops = 0
confirmed = 0
keys = []
characters = []
down = []
pressed = []
rl.get_key_pressed = lambda: keys.pop(0) if keys else 0
rl.get_char_pressed = lambda: characters.pop(0) if characters else 0
rl.is_key_down = lambda key: key in down
rl.is_key_pressed = lambda key: key in pressed
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
original_monotonic = time.monotonic
time.monotonic = lambda: now


def pop():
  global pops
  pops += 1


def confirm():
  global confirmed
  confirmed += 1


gui_app.pop_widget = pop
if kind == 'mici':
  widget = MiciKeyboard(auto_return_to_letters=p.get('auto_return', ''))
  widget.set_text(text)
elif kind == 'input':
  widget = InputBox(p.get('max', 255), p.get('password', False))
  widget.text = text
elif kind == 'html':
  widget = HtmlRenderer(text=text, text_size={ElementType.P: p.get('size', 48)}, center_text=p.get('center', False))
elif kind == 'keyboard':
  widget = Keyboard(min_text_size=8, password_mode=p.get('password', False), show_password_toggle=p.get('toggle', False))
  widget.set_title('Keyboard Input', 'Type your text below')
  widget.set_text(text)
elif kind == 'confirm':
  widget = ConfirmDialog(text, 'Continue', p.get('cancel', 'Cancel'), rich=p.get('rich', False))
elif kind == 'options':
  widget = MultiOptionDialog(text, ['First', 'Second', 'Third', 'Fourth'], 'First')
elif kind == 'list':
  action = p.get('action', 'toggle')
  if action == 'toggle':
    item = ToggleAction(True)
  elif action == 'button':
    item = ButtonAction('Edit')
    item.set_value('Current')
  elif action == 'text':
    item = TextAction('Connected', rl.Color(170, 170, 170, 255))
  elif action == 'multiple':
    item = MultipleButtonAction(['One', 'Two'], 200)
  widget = ListItem(text, description=p.get('description', ''), description_visible=p.get('description_visible', False), action_item=item)
elif kind == 'slider':
  widget = LargerSlider(text, confirm, green=p.get('green', False))
else:
  raise ValueError(kind)
rect = rl.Rectangle(*(scene['rect'][key] for key in ('x', 'y', 'width', 'height')))
widget.set_rect(rect)
widget.set_parent_rect(rect)


def snapshot():
  if kind == 'mici':
    layer = next(
      name
      for name, rows in [
        ('Lower', widget._lower_keys),
        ('Upper', widget._upper_keys),
        ('Special', widget._special_keys),
        ('SuperSpecial', widget._super_special_keys),
      ]
      if rows is widget._current_keys
    )
    return {
      'text': widget.text(),
      'candidate': widget.get_candidate_character(),
      'caps': ['Lower', 'Upper', 'Lock'][widget._caps_state],
      'layer': layer,
      'selected_at': widget._selected_key_t,
      'unselect_at': widget._unselect_key_t,
      'dragging': widget._dragging_on_keyboard,
      'keys': [
        {
          'value': key.char,
          'rect': {name: getattr(key.rect, name) for name in ('x', 'y', 'width', 'height')},
          'original': {'x': key.original_position.x, 'y': key.original_position.y},
          'size': key._size_filter.x,
          'alpha': key._alpha_filter.x,
        }
        for row in widget._current_keys
        for key in row
      ],
    }
  if kind == 'input':
    return {
      'text': widget.text,
      'cursor': widget._cursor_position,
      'offset': widget._text_offset,
      'show_cursor': widget._show_cursor,
      'display': widget._get_display_text(),
    }
  if kind == 'keyboard':
    return {
      'text': widget.text,
      'layout': widget._layout_name,
      'caps': widget._caps_lock,
      'cursor': widget._input_box._cursor_position,
      'display': widget._input_box._get_display_text(),
    }
  if kind == 'html':
    return {
      'height': widget.get_total_height(int(widget.rect.width)),
      'elements': [
        {
          'kind': element.type.value,
          'content': element.content,
          'size': element.font_size,
          'top': element.margin_top,
          'bottom': element.margin_bottom,
          'line_height': element.line_height,
          'indent': element.indent_level,
        }
        for element in widget.elements
      ],
    }
  if kind == 'options':
    return {'selection': widget.selection}
  if kind == 'list':
    right = widget.get_right_item_rect(widget.rect)
    return {
      'description_visible': widget.description_visible,
      'height': widget.rect.height,
      'right': {key: getattr(right, key) for key in ('x', 'y', 'width', 'height')},
    }
  if kind == 'slider':
    return {
      'confirmed': widget.confirmed,
      'percentage': widget.slider_percentage,
      'position': widget._scroll_x_circle_filter.x,
      'scale': widget._circle_scale_filter.x,
      'dragging': widget._is_dragging_circle,
    }
  return None


loop = gui_app.render()
results = []
for frame in range(scene['frames']):
  now = frame / 20
  next(loop)
  events = []
  keys = []
  characters = []
  down = []
  pressed = []
  for action in scene.get('actions', []):
    if action['frame'] != frame:
      continue
    if action.get('key'):
      keys.append(action['key'])
      pressed.append(action['key'])
    down.extend(action.get('down', []))
    events.extend(action.get('events', []))
    operation = action.get('operation', '')
    if operation == 'character':
      characters.extend(map(ord, action['text']))
    elif operation == 'key':
      widget.handle_key_press(action['text'])
    elif operation == 'text':
      if kind == 'input':
        widget.text = action['text']
      elif kind == 'html':
        widget.parse_html_content(action['text'])
      else:
        widget.set_text(action['text'])
    elif operation == 'cursor':
      widget.set_cursor_position(action['value'])
    elif operation == 'selection':
      widget.selection = action['text']
    elif operation == 'backspace':
      widget.backspace()
    elif operation == 'space':
      widget.space()
    elif operation == 'clear':
      widget.clear()
  gui_app._mouse_events = [
    MouseEvent(MousePos(event['pos']['x'], event['pos']['y']), event['slot'], event['pressed'], event['released'], event['down'], event['time'])
    for event in events
  ]
  if gui_app._mouse_events:
    gui_app._last_mouse_event = gui_app._mouse_events[-1]
  rl.get_mouse_position = lambda: rl.Vector2(*gui_app.last_mouse_event.pos)
  widget.render()
  results.append({'state': snapshot(), 'pops': pops, 'confirmed': confirmed})
rl.rl_draw_render_batch_active()
capture = rl.load_image_from_texture(gui_app._render_texture.texture) if gui_app._render_texture else rl.load_image_from_screen()
if gui_app._render_texture:
  rl.image_flip_vertical(capture)
assert rl.export_image(capture, str(output))
rl.unload_image(capture)
output.with_suffix('.json').write_text(json.dumps(results, indent=2))
loop.close()
time.monotonic = original_monotonic
gui_app.close()
