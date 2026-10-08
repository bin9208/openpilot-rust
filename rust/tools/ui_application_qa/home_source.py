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
      raw = bytes(scene["raw_params"][key])
      if key in ['UpdaterCurrentReleaseNotes', 'UpdaterNewReleaseNotes', 'CarParamsPersistent']:
        return raw
      try:
        return json.loads(raw) if key.startswith('Offroad_') else raw.decode()
      except (ValueError, UnicodeDecodeError):
        return None
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
    if (key == 'ApiCache_FirehoseStats' or key.startswith('Offroad_')) and value:
      try:
        return json.loads(value)
      except ValueError:
        return None
    return value

  def get_bool(self, key, *args):
    return self.get(key) == '1'

  def put_bool(self, key, value):
    scene.setdefault('params', {})[key] = '1' if value else '0'

  def put(self, key, value):
    scene.setdefault('params', {})[key] = ('1' if value else '0') if isinstance(value, bool) else str(value)

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
state_module.ui_state.update_params()
sys.modules[state_module.__name__] = state_module
import pyray as rl
from openpilot.system.ui.lib.application import gui_app, MouseEvent, MousePos
from openpilot.system.ui.lib.multilang import multilang

multilang._language = scene['language']
multilang.setup()
gui_app.init_window('Source product widget')

from openpilot.cereal import log

alertmanager = ModuleType('openpilot.selfdrive.selfdrived.alertmanager')
alertmanager.OFFROAD_ALERTS = json.loads((Path(__file__).resolve().parents[3] / 'openpilot/selfdrive/selfdrived/alerts_offroad.json').read_text())
sys.modules[alertmanager.__name__] = alertmanager
from openpilot.selfdrive.ui.layouts.home import HomeLayout
from openpilot.selfdrive.ui.layouts.sidebar import Sidebar
from openpilot.selfdrive.ui.widgets.exp_mode_button import ExperimentalModeButton

now = 0.0
time.monotonic = lambda: now
time.monotonic_ns = lambda: int(now * 1e9)
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
effects = []
hardware.HARDWARE.reboot = lambda: effects.append('updater:Reboot')
state_module.ui_state.recording_audio = False
state_module.ui_state.panda_type = 0
state_module.ui_state.sm.updated['deviceState'] = False
initial = log.DeviceState.new_message()
state_module.ui_state.sm['deviceState'] = initial
decode_threads = []
if scene['kind'] == 'onboarding':
  import openpilot.selfdrive.ui.layouts.onboarding as onboarding
  import threading

  def thread(*args, **kwargs):
    result = threading.Thread(*args, **kwargs)
    decode_threads.append(result)
    return result

  onboarding.threading = SimpleNamespace(Thread=thread)
  widget = onboarding.OnboardingWindow()
  gui_app.pop_widget = lambda: effects.append('pop')
  gui_app.request_close = lambda: effects.append('exit')
elif scene['kind'] == 'mici-home':
  from openpilot.selfdrive.ui.mici.layouts.home import MiciHomeLayout

  widget = MiciHomeLayout()
  widget.set_callbacks(lambda: effects.append('settings'), lambda: effects.append('web'))
elif scene['kind'] == 'home':
  widget = HomeLayout()
  widget.set_settings_callback(lambda: effects.append('settings'))
elif scene['kind'] == 'sidebar':
  widget = Sidebar()
  widget.set_callbacks(lambda: effects.append('settings'), lambda: effects.append('web'), lambda: effects.append('microphone'))
else:
  widget = ExperimentalModeButton()
  widget.set_click_callback(lambda: effects.append('settings'))
widget.set_rect(rl.Rectangle(*(scene['rect'][key] for key in ['x', 'y', 'width', 'height'])))
loop = gui_app.render()
results = []


def metric(value):
  c = value.color
  if isinstance(c, tuple):
    c = SimpleNamespace(r=c[0], g=c[1], b=c[2], a=c[3])
  return {'label': value.label, 'value': value.value, 'color': int(c.r) | (int(c.g) << 8) | (int(c.b) << 16) | (int(c.a) << 24)}


try:
  for index in range(scene['frames']):
    step = next((step for step in scene.get('steps', []) if step['frame'] == index), {})
    now = step.get('now', index / 20)
    for key, value in step.get('params', {}).items():
      if value is None:
        scene.setdefault('params', {}).pop(key, None)
      else:
        scene.setdefault('params', {})[key] = value
    if 'recording' in step:
      state_module.ui_state.recording_audio = step['recording']
    if 'panda' in step:
      state_module.ui_state.panda_type = step['panda']
    state_module.ui_state.sm.updated['deviceState'] = 'device' in step
    if 'device' in step:
      d = step['device']
      state_module.ui_state.sm['deviceState'] = log.DeviceState.new_message(
        networkType=d['network'], networkStrength=d['strength'], thermalStatus=d['thermal'], lastAthenaPingTime=d['ping']
      )
    if 'scroll' in step:
      widget.offroad_alert.scroll_panel.set_offset(step['scroll'])
      widget.update_alert.scroll_panel.set_offset(step['scroll'])
    if step.get('flush_training'):
      for thread in decode_threads:
        thread.join()
    if index == 0:
      widget.show_event()
    next(loop)
    gui_app._mouse_events = [
      MouseEvent(MousePos(e['pos']['x'], e['pos']['y']), e['slot'], e['pressed'], e['released'], e['down'], e['time']) for e in step.get('events', [])
    ]
    if gui_app._mouse_events:
      gui_app._last_mouse_event = gui_app._mouse_events[-1]
    rl.get_mouse_position = lambda: rl.Vector2(*gui_app.last_mouse_event.pos)
    rl.is_mouse_button_down = lambda _: gui_app.last_mouse_event.left_down
    widget.render()
    if scene['kind'] == 'onboarding':
      state = {
        'page': ['Terms', 'Training', 'Decline'][widget._state],
        'completed': widget.completed,
        'step': widget._training_guide._step,
        'uploaded': len(widget._training_guide._textures),
      }
    elif scene['kind'] == 'home':
      state = {
        'current': ['Home', 'Update', 'Alerts'][widget.current_state],
        'update': widget.update_available,
        'alerts': widget.alert_count,
        'version': widget._version_text,
        'last_refresh': widget.last_refresh,
      }
    elif scene['kind'] == 'sidebar':
      state = {
        'network': widget._net_type,
        'strength': widget._net_strength,
        'temperature': metric(widget._temp_status),
        'panda': metric(widget._panda_status),
        'connection': metric(widget._connect_status),
        'recording': widget._recording_audio,
      }
    elif scene['kind'] == 'mici-home':
      state = {
        'version': dict(zip(['version', 'branch', 'commit', 'date'], widget._version_text, strict=True)) if widget._version_text else None,
        'experimental': widget._experimental_mode,
        'address': widget._ip_address,
        'last_refresh': widget._last_refresh,
        'did_long_press': widget._did_long_press,
      }
    else:
      state = {'experimental': widget.experimental_mode}
    results.append(
      {
        'state': state,
        'effects': effects.copy(),
        'snooze': Params().get_bool('SnoozeUpdate'),
        'excessive': scene.get('params', {}).get('Offroad_ExcessiveActuation', ''),
        'accepted': Params().get('HasAcceptedTerms') or '',
        'trained': Params().get('CompletedTrainingVersion') or '',
        'record_front': Params().get_bool('RecordFront'),
        'uninstall': Params().get_bool('DoUninstall'),
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
