"""Actual product widget render oracle with owned data boundaries."""

import json
import ast
import datetime
import os
from pathlib import Path
import sys
import time
from types import ModuleType, SimpleNamespace
from product_effects import car_bytes, attach_refresh

scene = json.loads(Path(sys.argv[1]).read_text())
output = Path(sys.argv[2]).resolve()
os.environ.update(BIG='1' if scene['config']['big'] else '0', SCALE=str(scene['config']['scale']), OFFSCREEN='1')
hardware = ModuleType('openpilot.system.hardware')
hardware.PC = True
hardware.TICI = scene['config']['large_viewport']
hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: 'pc')
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://product-source', swaglog_root=lambda: str(output.parent / 'logs'))
sys.modules[paths.__name__] = paths
params_module = ModuleType('openpilot.common.params')


class Params:
  def get(self, key, *args, **kwargs):
    if key in scene.get("raw_params", {}):
      return bytes(scene["raw_params"][key])
    if key == "CarParamsPersistent":
      return car_bytes(scene)
    value = scene.get('params', {}).get(key)
    if key == 'LastUpdateTime':
      try:
        return datetime.datetime.fromisoformat(value) if value else None
      except ValueError:
        return None
    if key in ['UpdaterCurrentReleaseNotes', 'UpdaterNewReleaseNotes']:
      return value.encode() if value else None
    if key in ['LongitudinalPersonality', 'UpdateFailedCount']:
      try:
        return int(value.encode()) if value else (1 if kwargs.get('return_default') else None)
      except ValueError:
        return 1 if kwargs.get('return_default') else None
    return json.loads(value) if (key == 'ApiCache_FirehoseStats' or key.startswith('Offroad_')) and value else value

  def get_bool(self, key, *args):
    return self.get(key) == '1'

  def put_bool(self, key, value):
    scene.setdefault('params', {})[key] = '1' if value else '0'

  def put(self, key, value):
    scene.setdefault('params', {})[key] = str(value)

  def put_bool_nonblocking(self, key, value):
    self.put_bool(key, value)

  def put_nonblocking(self, key, value):
    self.put(key, value)

  def remove(self, key):
    scene.setdefault('params', {}).pop(key, None)
    scene.setdefault('raw_params', {}).pop(key, None)

  def get_int(self, key, *args):
    return int(self.get(key) or 0)


params_module.Params = Params
params_module.UnknownKeyName = KeyError
sys.modules[params_module.__name__] = params_module
registration = ModuleType('openpilot.system.athena.registration')
registration.Params = Params
registration_source = ast.parse((Path(__file__).resolve().parents[3] / 'openpilot/system/athena/registration.py').read_text())
registration_nodes = [
  node
  for node in registration_source.body
  if (isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == 'UNREGISTERED_DONGLE_ID' for target in node.targets))
  or (isinstance(node, ast.FunctionDef) and node.name == 'is_registered_device')
]
exec(compile(ast.Module(body=registration_nodes, type_ignores=[]), 'registration.py', 'exec'), registration.__dict__)
sys.modules[registration.__name__] = registration

state_module = ModuleType('openpilot.selfdrive.ui.ui_state')


class Messages(dict):
  updated = {'selfdriveState': False}


engaged_callbacks = []
offroad_callbacks = []
state_module.device = SimpleNamespace(_awake=True, awake=True)
state_module.ui_state = SimpleNamespace(
  started=False,
  ignition=False,
  is_release=scene.get("params", {}).get("IsReleaseBranch") == "1",
  params=Params(),
  engaged=False,
  CP=None,
  has_longitudinal_control=False,
  personality=1,
  update_params=lambda: None,
  add_engaged_transition_callback=engaged_callbacks.append,
  add_offroad_transition_callback=offroad_callbacks.append,
  is_offroad=lambda: not state_module.ui_state.started,
  is_onroad=lambda: state_module.ui_state.started,
  sm=Messages(deviceState=SimpleNamespace(networkType=scene.get("network_type", 0), networkMetered=scene.get("network_metered", False))),
  prime_state=SimpleNamespace(is_prime=lambda: scene['prime'] > 0, is_paired=lambda: scene['prime'] > -1),
  params_memory=SimpleNamespace(get=lambda key: scene.get('address')),
)
attach_refresh(state_module.ui_state, scene)
sys.modules[state_module.__name__] = state_module
import pyray as rl
from openpilot.system.ui.lib.application import gui_app, MouseEvent, MousePos
from openpilot.system.ui.lib.multilang import multilang

multilang._language = scene['language']
multilang.setup()
gui_app.init_window('Source product widget')


sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'ui_home_qa'))
from compact_source import classes

classes = classes(Path(__file__).resolve().parents[3], gui_app, state_module.ui_state)
now = 0.0
time.monotonic = lambda: now
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
effects = []
if scene['kind'] == 'terms':
  widget = classes.TermsPage(lambda: effects.append('accept'), lambda: effects.append('decline'))
elif scene['kind'] == 'attention':
  widget = classes.TrainingGuideAttentionNotice(lambda: effects.append('next'))
elif scene['kind'] == 'pre-dm':
  widget = classes.TrainingGuidePreDMTutorial(lambda: effects.append('next'))
elif scene['kind'] == 'bad-face':
  widget = classes.DMBadFaceDetected()
  widget._scroller._items[-1].set_click_callback(lambda: effects.append('back'))
else:
  widget = classes.TrainingGuideRecordFront(lambda: effects.append('next'))
from openpilot.selfdrive.ui.mici.widgets.dialog import BigConfirmationDialog

original_push = gui_app.push_widget


def push(value):
  if isinstance(value, BigConfirmationDialog):
    effects.append('confirm')
  original_push(value)


gui_app.push_widget = push
gui_app.push_widget(widget)
events = []
gui_app._mouse._handle_mouse_event = lambda: None
gui_app._mouse.get_events = lambda: events
loop = gui_app.render()
results = []
try:
  for index in range(scene['frames']):
    now = index / 20
    step = next((step for step in scene.get('steps', []) if step['frame'] == index), {})
    events = [MouseEvent(MousePos(e['pos']['x'], e['pos']['y']), e['slot'], e['pressed'], e['released'], e['down'], e['time']) for e in step.get('events', [])]
    if 'scroll' in step:
      widget._scroller.scroll_to(step['scroll'], smooth=False, block_interrupt=False, block_widget_interaction=False)
    if 'scroll_item' in step:
      item = widget._scroller._items[step['scroll_item']]
      widget._scroller.scroll_to(item.rect.x + item.rect.width / 2 - 268, smooth=False)
    next(loop)
    results.append(
      {
        'effects': effects.copy(),
        'depth': len(gui_app._nav_stack),
        'offset': widget._scroller._scroll_offset,
        'driver_view': Params().get_bool('IsDriverViewEnabled'),
        'record_front': Params().get_bool('RecordFront'),
      }
    )
    rl.rl_draw_render_batch_active()
    image = rl.load_image_from_screen()
    assert rl.export_image(image, str(output.with_name(f'{output.stem}-{index}.png')))
    rl.unload_image(image)
  output.with_suffix('.json').write_text(json.dumps(results))
  loop.close()
finally:
  gui_app.close()
