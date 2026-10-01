"""Differential inertial scroll traces using the unchanged source class."""
import argparse
import importlib.util
import json
from pathlib import Path
import random
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

class Vector2:
  def __init__(self, x, y):
    self.x, self.y = x, y

  def __setattr__(self, key, value):
    super().__setattr__(key, struct.unpack('f', struct.pack('f', value))[0])

rl = ModuleType('pyray')
rl.Vector2 = Vector2
rl.Rectangle = SimpleNamespace
rl.check_collision_point_rec = lambda p, r: r.x <= p.x <= r.x + r.width and r.y <= p.y <= r.y + r.height
rl.get_frame_time = lambda: dt
sys.modules['pyray'] = rl
app = ModuleType('openpilot.system.ui.lib.application')
app.gui_app = SimpleNamespace(mouse_events=[])
app.MouseEvent = SimpleNamespace
sys.modules[app.__name__] = app
hardware = ModuleType('openpilot.system.hardware')
sys.modules[hardware.__name__] = hardware
rng = random.Random(125)
count = 0
for case, (horizontal, bounce, tici) in enumerate([(True, True, False), (False, True, True), (True, False, True)]):
  hardware.TICI = tici
  spec = importlib.util.spec_from_file_location('source_scroll', root / 'openpilot/system/ui/lib/scroll_panel2.py')
  source = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(source)
  panel = source.GuiScrollPanel2(horizontal, bounce)
  frames = []
  expected = []
  now = 0.0
  for _i in range(1500):
    events = []
    for _ in range(rng.randrange(4)):
      now += rng.choice([.007, .014, .0, .05])
      action = rng.randrange(3)
      events.append({'pos': {'x': rng.randrange(-80, 400), 'y': rng.randrange(-80, 400)}, 'slot': rng.randrange(2),
                     'pressed': action == 0, 'released': action == 1, 'down': action != 1, 'time': now})
    frame = {'bounds': {'x': 0, 'y': 0, 'width': 240, 'height': 240}, 'content': rng.choice([200, 500, 900]),
             'events': events, 'dt': rng.choice([.0, .02, .05, .1]), 'enabled': rng.random() > .02}
    frames.append(frame)
    app.gui_app.mouse_events = [SimpleNamespace(pos=SimpleNamespace(**e['pos']), slot=e['slot'], left_pressed=e['pressed'],
                                               left_released=e['released'], left_down=e['down'], t=e['time']) for e in events]
    dt = frame['dt']
    panel.set_enabled(frame['enabled'])
    panel.update(SimpleNamespace(**frame['bounds']), frame['content'])
    expected.append({'offset': panel.get_offset(), 'velocity': panel._velocity,
                     'state': {'STEADY': 'Steady', 'PRESSED': 'Pressed', 'MANUAL_SCROLL': 'Manual', 'AUTO_SCROLL': 'Auto'}[panel.state.name],
                     'touch_valid': panel.is_touch_valid()})
  fixture = json.dumps({'horizontal': horizontal, 'bounce': bounce, 'tici': tici, 'frames': frames})
  (args.output / f'scroll-{case}-input.json').write_text(fixture)
  (args.output / f'scroll-{case}-source.json').write_text(json.dumps(expected, indent=2))
  result = subprocess.run([str(args.binary)], input=fixture, text=True, capture_output=True, check=True)
  (args.output / f'scroll-{case}-native.json').write_text(result.stdout)
  actual = json.loads(result.stdout)
  for i, (a, e) in enumerate(zip(actual, expected, strict=True)):
    assert a['state'] == e['state'] and a['touch_valid'] == e['touch_valid'], (case, i, a, e)
    assert abs(a['offset'] - e['offset']) < 1e-5 and abs(a['velocity'] - e['velocity']) < 1e-8, (case, i, a, e)
  count += len(frames)
print(f'PASS: {count} source/native scroll frames; horizontal/vertical, bounce/snap, disabled, multi-slot and content changes')
