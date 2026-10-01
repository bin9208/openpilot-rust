"""Render unchanged shared widgets with isolated hardware, time, and input boundaries."""

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

hardware = ModuleType('openpilot.system.hardware')
hardware.PC = True
hardware.TICI = scene['config']['large_viewport']
hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: 'pc')
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://ui-framework-source', swaglog_root=lambda: str(output.parent / 'source-logs'))
sys.modules[paths.__name__] = paths
import pyray as rl
from openpilot.system.ui.lib.application import gui_app, FontWeight, MouseEvent, MousePos, TextAlignment, TextAlignmentVertical
from openpilot.system.ui.lib.multilang import multilang
from openpilot.system.ui.widgets.label import Label, UnifiedLabel
from openpilot.system.ui.widgets.button import Button, ButtonStyle
from openpilot.system.ui.widgets.toggle import Toggle

multilang._language = scene.get('language', 'en')
gui_app._mouse.start = lambda: None
gui_app._mouse.stop = lambda: None
gui_app.init_window('Source shared UI')
fonts = {
  'normal': FontWeight.NORMAL,
  'medium': FontWeight.MEDIUM,
  'bold': FontWeight.BOLD,
  'semi_bold': FontWeight.SEMI_BOLD,
  'display': FontWeight.DISPLAY,
  'regular': FontWeight.DISPLAY_REGULAR,
  'pretendard': FontWeight.PRETENDARD,
  'unifont': FontWeight.UNIFONT,
}
horizontal = {'left': TextAlignment.LEFT, 'center': TextAlignment.CENTER, 'right': TextAlignment.RIGHT}
vertical = {'top': TextAlignmentVertical.TOP, 'middle': TextAlignmentVertical.MIDDLE, 'bottom': TextAlignmentVertical.BOTTOM}
widgets = []
for element in scene['elements']:
  props = element.get('props', {})
  kind = element['kind']
  if kind == 'label':
    widget = Label(
      element['text'],
      font_size=props.get('size', 60),
      font_weight=fonts[props.get('font', 'normal')],
      text_padding=props.get('padding', 0),
      elide_right=props.get('elide', False),
      text_alignment=horizontal[props.get('horizontal', 'center')],
      text_alignment_vertical=vertical[props.get('vertical', 'middle')],
    )
  elif kind == 'unified':
    widget = UnifiedLabel(
      element['text'],
      font_size=props.get('size', 60),
      font_weight=fonts[props.get('font', 'normal')],
      text_padding=props.get('padding', 0),
      elide=props.get('elide', True),
      wrap_text=props.get('wrap', True),
      scroll=props.get('scroll', False),
      shimmer=props.get('shimmer', False),
      letter_spacing=props.get('letter_spacing', 0),
      line_height=props.get('line_height', 1),
      alignment=horizontal[props.get('horizontal', 'left')],
      alignment_vertical=vertical[props.get('vertical', 'top')],
    )
  elif kind == 'button':
    styles = {
      'normal': ButtonStyle.NORMAL,
      'primary': ButtonStyle.PRIMARY,
      'danger': ButtonStyle.DANGER,
      'list': ButtonStyle.LIST_ACTION,
      'border': ButtonStyle.TRANSPARENT_WHITE_BORDER,
    }
    widget = Button(element['text'], font_size=props.get('size', 60), button_style=styles[props.get('style', 'normal')])
  elif kind == 'toggle':
    widget = Toggle(props.get('value', False))
  else:
    raise ValueError(kind)
  widget.set_rect(rl.Rectangle(*(element['rect'][key] for key in ('x', 'y', 'width', 'height'))))
  widget.set_enabled(props.get('enabled', True))
  widgets.append(widget)
loop = gui_app.render()
for frame in range(scene['frames']):
  next(loop)
  rl.get_time = lambda frame=frame: frame / 20
  rl.get_frame_time = lambda: 0.05
  for widget, element in zip(widgets, scene['elements'], strict=True):
    gui_app._mouse_events = (
      [
        MouseEvent(MousePos(event['pos']['x'], event['pos']['y']), event['slot'], event['pressed'], event['released'], event['down'], event['time'])
        for event in element.get('events', [])
      ]
      if frame == 0
      else []
    )
    widget.render()
rl.rl_draw_render_batch_active()
capture = rl.load_image_from_texture(gui_app._render_texture.texture) if gui_app._render_texture else rl.load_image_from_screen()
if gui_app._render_texture:
  rl.image_flip_vertical(capture)
assert rl.export_image(capture, str(output))
rl.unload_image(capture)
state = [
  {'width': widget.text_width, 'scroll_offset': widget._scroll_offset}
  if isinstance(widget, UnifiedLabel)
  else {'value': widget.get_state(), 'progress': widget._progress}
  if isinstance(widget, Toggle)
  else None
  for widget in widgets
]
output.with_suffix('.json').write_text(json.dumps(state, indent=2))
loop.close()
gui_app.close()
