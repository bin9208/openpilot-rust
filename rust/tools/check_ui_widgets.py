"""Compare Rust shared widget transitions with the unmodified Python Widget."""
import argparse
import importlib.util
import json
from pathlib import Path
import random
import subprocess
import sys
from types import ModuleType, SimpleNamespace

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]

# Boundaries only: no display, device, Params or product UI state is accessed.
class Rect:
  def __init__(self, x, y, width, height):
    self.x, self.y, self.width, self.height = x, y, width, height


def intersection(a, b):
  x, y = max(a.x, b.x), max(a.y, b.y)
  w, h = min(a.x + a.width, b.x + b.width) - x, min(a.y + a.height, b.y + b.height) - y
  return Rect(x, y, w, h) if w > 0 and h > 0 else Rect(0, 0, 0, 0)

rl = ModuleType('pyray')
rl.Rectangle = Rect
rl.get_collision_rec = intersection
rl.check_collision_point_rec = lambda p, r: r.x <= p.x <= r.x + r.width and r.y <= p.y <= r.y + r.height
rl.get_time = lambda: now
sys.modules['pyray'] = rl
app = ModuleType('openpilot.system.ui.lib.application')
app.gui_app = SimpleNamespace(show_touches=False, mouse_events=[])
app.MousePos = SimpleNamespace
app.MouseEvent = SimpleNamespace
app.MAX_TOUCH_SLOTS = 2
sys.modules[app.__name__] = app
params = ModuleType('openpilot.common.params')
params.UnknownKeyName = type('UnknownKeyName', (Exception,), {})
sys.modules[params.__name__] = params
ui_state = ModuleType('openpilot.selfdrive.ui.ui_state')
ui_state.device = SimpleNamespace(awake=True)
sys.modules[ui_state.__name__] = ui_state
spec = importlib.util.spec_from_file_location('source_widget', root / 'openpilot/system/ui/widgets/__init__.py')
source = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source)

class Probe(source.Widget):
  def __init__(self):
    super().__init__()
    self.calls = []
    self._multi_touch = True
    self._click_delay = .1
    self.set_rect(Rect(10, 10, 100, 100))
    self.set_parent_rect(Rect(0, 0, 90, 90))

  def _render(self, rect):
    self.calls.append(f'paint:{str(self.is_pressed).lower()}')

  def _handle_mouse_press(self, pos):
    self.calls.append('press')

  def _handle_mouse_release(self, pos):
    self.calls.append('release')
    super()._handle_mouse_release(pos)

  def _handle_mouse_event(self, event):
    self.calls.append(f'event:{event.slot}')

rng = random.Random(125)
frames = []
# Actual valid touches, leave/re-enter, clipped parent, two slots, wake and visibility.
for i in range(800):
  events = []
  for _ in range(rng.randrange(4)):
    action = rng.randrange(3)
    events.append({'pos': {'x': rng.choice([0, 10, 40, 90, 95, 110, 120]), 'y': rng.choice([0, 10, 50, 90, 110])},
                   'slot': rng.randrange(2), 'pressed': action == 0, 'released': action == 1, 'down': action != 1, 'time': i / 20})
  frames.append({'now': i / 20, 'events': events, 'enabled': rng.random() > .08, 'visible': rng.random() > .08,
                 'awake': rng.random() > .1, 'valid': rng.random() > .1})
probe = Probe()
expected = []
for frame in frames:
  now = frame['now']
  probe.set_enabled(frame['enabled'])
  probe.set_visible(frame['visible'])
  probe.set_touch_valid_callback(lambda valid=frame['valid']: valid)
  ui_state.device.awake = frame['awake']
  app.gui_app.mouse_events = [SimpleNamespace(pos=SimpleNamespace(**e['pos']), slot=e['slot'], left_pressed=e['pressed'],
                                             left_released=e['released'], left_down=e['down'], t=e['time']) for e in frame['events']]
  probe.render()
  expected.append({'calls': probe.calls, 'pressed': probe.is_pressed})
  probe.calls = []
fixture = json.dumps({'frames': frames})
(args.output / 'widget-input.json').write_text(fixture)
(args.output / 'widget-source.json').write_text(json.dumps(expected, indent=2))
result = subprocess.run([str(args.binary)], input=fixture, text=True, capture_output=True, check=True)
(args.output / 'widget-native.json').write_text(result.stdout)
actual = json.loads(result.stdout)
assert actual == expected, next((i, a, e) for i, (a, e) in enumerate(zip(actual, expected, strict=True)) if a != e)
print(f'PASS: {len(frames)} source/native widget frames; clipping, multitouch, wake, disabled, hidden and invalid-touch transitions')
