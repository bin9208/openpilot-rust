"""Unchanged compact tutorial state and ring arithmetic oracle; no camera/device I/O."""

import argparse
import ast
from pathlib import Path
import json
import math
import subprocess
from types import SimpleNamespace
import numpy as np
import pyray as rl
from openpilot.common.filter_simple import FirstOrderFilter

parser = argparse.ArgumentParser()
parser.add_argument('binary', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]


class NavWidget:
  def _update_state(self):
    pass


ui_state = SimpleNamespace(params=SimpleNamespace(get_bool=lambda _: True))
device = SimpleNamespace(awake=True)
gui_app = SimpleNamespace(target_fps=20, get_active_widget=lambda: None)
from collections.abc import Callable

namespace = {"NavWidget": NavWidget, "Callable": Callable, "ui_state": ui_state, "device": device, "gui_app": gui_app, "math": math, "np": np, "rl": rl}
tree = ast.parse((root / 'openpilot/selfdrive/ui/mici/layouts/onboarding.py').read_text())
node = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'TrainingGuideDMTutorial')
exec(compile(ast.Module(body=[node], type_ignores=[]), 'onboarding.py', 'exec'), namespace)
widget = namespace['TrainingGuideDMTutorial'].__new__(namespace['TrainingGuideDMTutorial'])
widget._progress = FirstOrderFilter(0.0, 0.5, 1 / 20)
widget._bad_face_page = object()
widget._rect = rl.Rectangle(0, 0, 536, 240)
widget._good_button = SimpleNamespace(set_enabled=lambda value: setattr(widget, 'good', value))
widget.good = False
widget._dialog = SimpleNamespace(render=lambda _: None, _camera_view=SimpleNamespace(frame=False), driver_state_renderer=SimpleNamespace(_is_rhd=False))
ring = []
rl.draw_ring = lambda center, inner, outer, start, end, segments, color: ring.append(
  {
    'ring': [float(center.x), float(center.y), float(inner), float(outer), float(start), float(np.float32(end))],
    'segments': segments,
    'color': color.r | (color.g << 8) | (color.b << 16) | (color.a << 24),
  }
)
rl.draw_rectangle_gradient_v = lambda *args: None
rl.begin_scissor_mode = lambda *args: None
rl.end_scissor_mode = lambda: None
rl.draw_rectangle_rounded_lines_ex = lambda *args: None
steps = []


def add(orientation=None, **values):
  step = {
    'input': {
      'received': True,
      'face_detected': True,
      'orientation': orientation if orientation is not None else [0.0, 0.0, 0.0],
      'bad_face_page': False,
      'fps': 20.0,
    },
    'right_hand_drive': bool(len(steps) % 2),
  }
  for key, value in values.items():
    if key in step['input']:
      step['input'][key] = value
    else:
      step[key] = value
  steps.append(step)


for _ in range(130):
  add()
for _ in range(5):
  add(face_detected=False)
for _ in range(25):
  add(bad_face_page=True)
add(show=True, received=False)
for angle in [math.radians(30), np.nextafter(math.radians(30), np.inf), math.radians(-30), math.radians(29.99), math.radians(30.01)]:
  for axis in [0, 1]:
    orient = [0.0] * 3
    orient[axis] = float(angle)
    add(orient, value=0.3)
for size in [0, 1, 2, 4]:
  add([0.0] * size, value=0.4)
for progress in [0.0, 0.249999999999, 0.25, 0.99, 0.990000000001, 0.998, 0.999, 1.0]:
  for face in [False, True]:
    for bad in [False, True]:
      add(value=progress, face_detected=face, bad_face_page=bad)
for x in [0.0001, 31.333333, -71.234567]:
  for rhd in [False, True]:
    add(value=0.42, right_hand_drive=rhd, rect={'x': x, 'y': x, 'width': 535.55555, 'height': 240.33333})
source = []
for step in steps:
  data = step['input']
  rect = step.get('rect', {'x': 0, 'y': 0, 'width': 536, 'height': 240})
  widget._rect = rl.Rectangle(*(rect[key] for key in ['x', 'y', 'width', 'height']))
  if step.get('show'):
    widget._progress.x = 0.0
  if 'value' in step:
    widget._progress.x = step['value']
  sm = {'driverMonitoringState': SimpleNamespace(visionPolicyState=SimpleNamespace(faceDetected=data['face_detected']))}

  class Messages(dict):
    pass

  ui_state.sm = Messages(sm)
  ui_state.sm.recv_frame = {'driverMonitoringState': int(data['received'])}
  gui_app.get_active_widget = lambda data=data: widget._bad_face_page if data['bad_face_page'] else None
  widget._dialog.driver_state_renderer.get_driver_data = lambda data=data: SimpleNamespace(faceOrientation=data['orientation'])
  widget._dialog.driver_state_renderer._is_rhd = step['right_hand_drive']
  widget._update_state()
  widget._render(None)
  source.append({'progress': widget._progress.x, 'enabled': widget.good, **ring[-1]})
(args.output / 'input.json').write_text(json.dumps(steps))
(args.output / 'source.json').write_text(json.dumps(source))
process = subprocess.run([str(args.binary)], input=json.dumps(steps), text=True, capture_output=True, check=True)
(args.output / 'native.json').write_text(process.stdout)
native = json.loads(process.stdout)
for index, (a, b) in enumerate(zip(source, native, strict=True)):
  assert math.isclose(a['progress'], b['progress'], rel_tol=1e-11, abs_tol=1e-11), (index, a, b)
  assert {k: v for k, v in a.items() if k != 'progress'} == {k: v for k, v in b.items() if k != 'progress'}, (index, a, b)
(args.output / 'results.json').write_text(
  json.dumps({'passed': True, 'samples': len(steps), 'float64_bound': 1e-11, 'ring_geometry_colors_decisions': 'exact'})
)
print(f'PASS {len(steps)} source/native progress and ring samples')
