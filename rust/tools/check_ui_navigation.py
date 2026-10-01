"""Compare complete navigation animation/input ordering with unchanged source."""

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
    super().__setattr__(name, struct.unpack('f', struct.pack('f', value))[0])


rl = ModuleType('pyray')
rl.Rectangle = Rect
rl.Color = lambda *values: values
rl.get_time = lambda: now
rl.check_collision_point_rec = lambda p, r: r.x <= p.x <= r.x + r.width and r.y <= p.y <= r.y + r.height
rl.draw_rectangle_rounded = rl.draw_rectangle_rounded_lines_ex = rl.draw_rectangle_rec = lambda *args: None
rl.BLACK = (0, 0, 0, 255)
sys.modules['pyray'] = rl
app = ModuleType('openpilot.system.ui.lib.application')
app.gui_app = SimpleNamespace(target_fps=20, height=240, show_touches=False, mouse_events=[])
app.MouseEvent = app.MousePos = SimpleNamespace
app.MAX_TOUCH_SLOTS = 2
sys.modules[app.__name__] = app
params = ModuleType('openpilot.common.params')
params.UnknownKeyName = type('UnknownKeyName', (Exception,), {})
sys.modules[params.__name__] = params
state = ModuleType('openpilot.selfdrive.ui.ui_state')
state.device = SimpleNamespace(awake=True)
sys.modules[state.__name__] = state


def load(name, path):
  spec = importlib.util.spec_from_file_location(name, root / path)
  module = importlib.util.module_from_spec(spec)
  sys.modules[name] = module
  spec.loader.exec_module(module)
  return module


load('openpilot.system.ui.widgets', 'openpilot/system/ui/widgets/__init__.py')
source = load('openpilot.system.ui.widgets.nav_widget', 'openpilot/system/ui/widgets/nav_widget.py')
counts = {'pops': 0, 'back': 0, 'shown': 0, 'dismissed': 0}


def count(key):
  counts[key] += 1


class Probe(source.NavWidget):
  def _render(self, rect):
    pass

  def _back_enabled(self):
    return frame['back']


widget = Probe()
widget.set_rect(Rect(0, 0, 536, 240))
widget.set_back_callback(lambda: count('back'))
app.gui_app.pop_widget = lambda: count('pops')
frames = []
for index in range(480):
  phase = index % 80
  cycle = index // 80
  events = []
  positions = {18: (60, 30, 'pressed'), 19: (60, 75, 'down'), 20: (60, 125, 'down'), 21: (60, 160, 'released')}
  if cycle == 1:
    positions = {18: (60, 30, 'pressed'), 19: (140, 35, 'down'), 20: (140, 130, 'down'), 21: (140, 150, 'released')}
  if cycle == 2:
    positions = {18: (60, 220, 'pressed'), 19: (60, 230, 'down'), 20: (60, 235, 'released')}
  if phase in positions:
    x, y, action = positions[phase]
    events = [
      {'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': action == 'pressed', 'released': action == 'released', 'down': action != 'released', 'time': index / 20}
    ]
  frames.append(
    {
      'now': index / 20,
      'events': events,
      'show': phase == 0,
      'dismiss': phase == 40 and cycle in (1, 2, 3),
      'enabled': not (cycle == 4 and 19 <= phase <= 21),
      'back': cycle != 5,
    }
  )
expected = []
app.gui_app.last_mouse_event = SimpleNamespace(pos=SimpleNamespace(x=0, y=0))
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
  if app.gui_app.mouse_events:
    app.gui_app.last_mouse_event = app.gui_app.mouse_events[-1]
  widget.set_enabled(frame['enabled'])
  if frame['show']:
    widget.set_shown_callback(lambda: count('shown'))
    widget.show_event()
  if frame['dismiss']:
    widget.dismiss(lambda: count('dismissed'))
  widget.render()
  expected.append(
    {
      'y': widget.rect.y,
      'position': widget._y_pos_filter.x,
      'velocity': widget._y_pos_filter.velocity.x,
      'bar_y': widget._nav_bar_y_filter.x,
      'bar_alpha': widget._nav_bar._alpha_filter.x,
      'dragging': widget._dragging_down,
      'playing': widget._playing_dismiss_animation,
      **counts,
    }
  )
fixture = json.dumps({'frames': frames})
(args.output / 'navigation-input.json').write_text(fixture)
(args.output / 'navigation-source.json').write_text(json.dumps(expected, indent=2))
result = subprocess.run([str(args.binary)], input=fixture, capture_output=True, text=True, check=True)
(args.output / 'navigation-native.json').write_text(result.stdout)
actual = json.loads(result.stdout)
for index, (a, e) in enumerate(zip(actual, expected, strict=True)):
  for key in e:
    assert abs(a[key] - e[key]) < 1e-6, (index, key, a, e)
print(f'PASS: {len(frames)} source/native navigation frames; show/swipe/cancel/blocked/programmatic dismiss and callback counts')
