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
inactivity = []
state_module.device = SimpleNamespace(_awake=True, awake=True, override=None, brightness=65, add_interactive_timeout_callback=inactivity.append)
state_module.device.set_override_interactive_timeout = lambda value: setattr(state_module.device, 'override', value)
state_module.device.set_offroad_brightness = lambda value: setattr(state_module.device, 'brightness', 65 if value is None else value)
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
from onboarding_source import load

classes = load(Path(__file__).resolve().parents[3], gui_app, state_module.ui_state, state_module.device)
now = 0.0
time.monotonic = lambda: now
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
effects = []
widget = classes.OnboardingWindow(lambda: effects.append('completed'))
tutorial = widget._training_guide._steps[2]
from openpilot.cereal import log

state_module.ui_state.started_frame = 0
state_module.ui_state.status = 0
state_module.ui_state.sm.recv_frame = {}
state_module.ui_state.sm['selfdriveState'] = log.SelfdriveState.new_message()
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
    if 'lifecycle' in step:
      if step['lifecycle']:
        widget.show_event()
      else:
        widget.hide_event()
    if 'awake' in step:
      state_module.device.awake = step['awake']
    if step.get('timeout'):
      for callback in inactivity:
        callback()
    if step.get('close'):
      widget.close()
    if 'driver' in step:
      options = step['driver']
      drivers = log.DriverStateV2.new_message()
      for name, x in [('leftDriverData', -0.18), ('rightDriverData', 0.18)]:
        data = getattr(drivers, name)
        data.faceOrientation = options['orientation']
        data.faceOrientationStd = [options['deviation'], options['deviation'], 0.1]
        data.facePosition = [x, 0.05]
        data.leftEyeProb, data.rightEyeProb = options['eyes']
        data.sunglassesProb = options['glasses']
      dm = log.DriverMonitoringState.new_message()
      dm.isRHD = options['rhd']
      dm.activePolicy = 'vision'
      dm.visionPolicyState.faceDetected = options['detected']
      dm.visionPolicyState.awarenessPercent = 83
      state_module.ui_state.sm.update(driverStateV2=drivers, driverMonitoringState=dm)
      state_module.ui_state.sm.recv_frame.update(driverStateV2=index + 1, driverMonitoringState=index + 1)
    if 'scroll_item' in step:
      page = step.get('page', 'terms')
      target = widget._terms if page == 'terms' else widget._training_guide._steps[{'attention': 0, 'pre-dm': 1, 'record-front': 3}[page]]
      item = target._scroller._items[step['scroll_item']]
      target._scroller.scroll_to(item.rect.x + item.rect.width / 2 - 268, smooth=False)
    directory = Path(os.environ['UI_CAMERA_SYNC'])
    (directory / f'{index}.ready').write_text('ready')
    start = time.perf_counter()
    while not (directory / f'{index}.allow').exists():
      assert time.perf_counter() - start < 10
      time.sleep(0.001)
    next(loop)
    results.append(
      {
        'effects': effects.copy(),
        'depth': len(gui_app._nav_stack),
        'driver_view': Params().get_bool('IsDriverViewEnabled'),
        'record_front': Params().get_bool('RecordFront'),
        'accepted': Params().get('HasAcceptedTerms') or '',
        'trained': Params().get('CompletedTrainingVersion') or '',
        'uninstall': Params().get_bool('DoUninstall'),
        'completed': widget.completed,
        'progress': tutorial._progress.x,
        'good': tutorial._good_button.enabled,
        'frame': tutorial._dialog._camera_view.frame is not None,
        'rhd': tutorial._dialog.driver_state_renderer.is_rhd,
        'timeout': state_module.device.override,
        'brightness': state_module.device.brightness,
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
