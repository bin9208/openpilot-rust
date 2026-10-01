"""Check source/native scroll layout, snap, item reorder and click suppression."""

import argparse
import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import sys
from types import ModuleType, SimpleNamespace

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root))


class Rect:
  def __init__(self, x, y, width, height):
    self.x, self.y, self.width, self.height = x, y, width, height

  def __setattr__(self, name, value):
    object.__setattr__(self, name, struct.unpack('f', struct.pack('f', value))[0])


class Vector2:
  def __init__(self, x, y):
    self.x, self.y = x, y

  __setattr__ = Rect.__setattr__


def intersection(a, b):
  x, y = max(a.x, b.x), max(a.y, b.y)
  w, h = min(a.x + a.width, b.x + b.width) - x, min(a.y + a.height, b.y + b.height) - y
  return Rect(x, y, w, h) if w > 0 and h > 0 else Rect(0, 0, 0, 0)


rl = ModuleType('pyray')
rl.Rectangle, rl.Vector2, rl.Color = Rect, Vector2, lambda *values: values
rl.get_collision_rec = intersection
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
rl.check_collision_point_rec = lambda p, r: r.x <= p.x <= r.x + r.width and r.y <= p.y <= r.y + r.height
rl.check_collision_recs = lambda a, b: a.x < b.x + b.width and a.x + a.width > b.x and a.y < b.y + b.height and a.y + a.height > b.y
rl.begin_scissor_mode = rl.end_scissor_mode = rl.draw_rectangle_rec = lambda *a: None
rl.GRAY = (130, 130, 130, 255)
sys.modules['pyray'] = rl
app = ModuleType('openpilot.system.ui.lib.application')
app.gui_app = SimpleNamespace(target_fps=20, show_touches=False, mouse_events=[], texture=lambda *a: SimpleNamespace(width=96, height=48))
app.MouseEvent = app.MousePos = SimpleNamespace
app.MAX_TOUCH_SLOTS = 2
sys.modules[app.__name__] = app
params = ModuleType('openpilot.common.params')
params.UnknownKeyName = type('UnknownKeyName', (Exception,), {})
sys.modules[params.__name__] = params
state = ModuleType('openpilot.selfdrive.ui.ui_state')
state.device = SimpleNamespace(awake=True)
sys.modules[state.__name__] = state
hardware = ModuleType('openpilot.system.hardware')
hardware.TICI = False
sys.modules[hardware.__name__] = hardware
logging = ModuleType('openpilot.common.swaglog')
logging.cloudlog = SimpleNamespace(warning=lambda *a: None)
sys.modules[logging.__name__] = logging


def load(name, path):
  spec = importlib.util.spec_from_file_location(name, root / path)
  module = importlib.util.module_from_spec(spec)
  sys.modules[name] = module
  spec.loader.exec_module(module)
  return module


widgets = load('openpilot.system.ui.widgets', 'openpilot/system/ui/widgets/__init__.py')
source = load('openpilot.system.ui.widgets.scroller', 'openpilot/system/ui/widgets/scroller.py')


class Item(widgets.Widget):
  def _render(self, rect):
    pass


def check(a, e, path=''):
  if isinstance(e, dict):
    assert a.keys() == e.keys(), path
    for key in e:
      check(a[key], e[key], f'{path}.{key}')
  elif isinstance(e, list):
    assert len(a) == len(e), path
    for i, (av, ev) in enumerate(zip(a, e, strict=True)):
      check(av, ev, f'{path}[{i}]')
  elif isinstance(e, float):
    assert abs(a - e) <= 1e-4, (path, a, e)
  else:
    assert a == e, (path, a, e)


for case, (horizontal, snap) in enumerate([(True, False), (True, True), (False, False)]):
  counts = [0] * 4
  items = []
  for i, width in enumerate([140, 190, 90, 240]):
    item = Item()
    item.ident = i
    item.set_rect(Rect(0, 0, width, 140))
    item.set_click_callback(lambda i=i, counts=counts: counts.__setitem__(i, counts[i] + 1))
    items.append(item)
  widget = source._Scroller(items, horizontal=horizontal, snap_items=snap, scroll_indicator=False, edge_shadows=False)
  widget.set_rect(Rect(10, 10, 516, 220))
  frames = []
  for index in range(240):
    phase = index % 40
    events = []
    if phase in range(4, 11):
      action = 'pressed' if phase == 4 else 'released' if phase == 10 else 'down'
      position = 180 - (phase - 4) * 24
      events = [
        {
          'pos': {'x': position if horizontal else 180, 'y': 90 if horizontal else position},
          'slot': 0,
          'pressed': action == 'pressed',
          'released': action == 'released',
          'down': action != 'released',
          'time': index / 20,
        }
      ]
    frames.append(
      {
        'now': index / 20,
        'events': events,
        'show': index in (0, 180),
        'enabled': not 122 <= index <= 125,
        'visible': [True, not 130 <= index <= 150, True, True],
        'scroll': [160, True, True, True] if index == 61 else [-100, False, False, False] if index == 190 else None,
        'movement': [0, 3] if horizontal and index == 60 else None,
      }
    )
  expected = []
  for frame in frames:
    now = frame['now']
    app.gui_app.mouse_events = [
      SimpleNamespace(
        pos=SimpleNamespace(**event['pos']),
        slot=event['slot'],
        left_pressed=event['pressed'],
        left_released=event['released'],
        left_down=event['down'],
        t=event['time'],
      )
      for event in frame['events']
    ]
    widget.set_enabled(frame['enabled'])
    for item, visible in zip(widget.items, frame['visible'], strict=True):
      item.set_visible(visible)
    if frame['show']:
      widget.show_event()
    if frame['movement']:
      widget.move_item(*frame['movement'])
    if frame['scroll']:
      widget.scroll_to(*frame['scroll'])
    widget.render()
    expected.append(
      {
        'offset': widget.scroll_panel.get_offset(),
        'velocity': widget.scroll_panel._velocity,
        'state': {'STEADY': 'Steady', 'PRESSED': 'Pressed', 'MANUAL_SCROLL': 'Manual', 'AUTO_SCROLL': 'Auto'}[widget.scroll_panel.state.name],
        'content': widget.content_size,
        'auto': widget.is_auto_scrolling,
        'moving': widget.moving_items,
        'items': [{'id': item.ident, 'x': item.rect.x, 'y': item.rect.y} for item in widget.items],
        'clicks': counts.copy(),
      }
    )
  fixture = json.dumps({'horizontal': horizontal, 'snap': snap, 'frames': frames})
  (args.output / f'scroller-{case}-input.json').write_text(fixture)
  (args.output / f'scroller-{case}-source.json').write_text(json.dumps(expected, indent=2))
  result = subprocess.run([str(args.binary)], input=fixture, capture_output=True, text=True, check=True)
  (args.output / f'scroller-{case}-native.json').write_text(result.stdout)
  check(json.loads(result.stdout), expected)
print('PASS: 720 source/native scroller frames; snap, reorder/lift, smooth/instant scroll, hiding and child click gates')
