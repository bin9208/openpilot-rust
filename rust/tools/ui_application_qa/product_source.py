"""Actual product widget render oracle with owned data boundaries."""

import json
import ast
import datetime
import os
from pathlib import Path
import sys
import time
from types import ModuleType, SimpleNamespace
from product_effects import car_bytes, attach_refresh, Effects

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
    return json.loads(value) if key == 'ApiCache_FirehoseStats' and value else value

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
from openpilot.selfdrive.ui.widgets.prime import PrimeWidget
from openpilot.selfdrive.ui.widgets.carrot_web_dialog import CarrotWebDialog

multilang._language = scene['language']
multilang.setup()
gui_app.init_window('Source product widget')
effects = Effects(scene, state_module.ui_state, gui_app, engaged_callbacks)
effects.offroad_callbacks = offroad_callbacks
dialog_results = []
network = None
if 'dialog' in scene:
  from openpilot.selfdrive.ui.mici.widgets.dialog import BigDialog, BigConfirmationDialog, BigInputDialog

  options = scene['dialog']
  gui_app.pop_widget = lambda *args, **kwargs: None
  if scene['kind'] == 'dialog-info':
    widget = BigDialog(options['title'], options.get('description', ''))
  elif scene['kind'] == 'dialog-confirm':
    icon = gui_app.texture('icons_mici/settings/device/reboot.png', 64, 64)
    widget = BigConfirmationDialog(
      options['title'], icon, lambda: dialog_results.append('confirm'), exit_on_confirm=not options.get('stay', False), red=options.get('red', False)
    )
  else:
    widget = BigInputDialog(options['title'], options.get('text', ''), confirm_callback=lambda text: dialog_results.append(text))
elif scene['kind'] == 'language':
  from openpilot.system.ui.widgets.option_dialog import MultiOptionDialog
  from openpilot.system.ui.lib.application import FontWeight
  from openpilot.system.ui.lib.multilang import tr
  from openpilot.system.ui.widgets import DialogResult

  def select_language(result):
    if result == DialogResult.CONFIRM:
      code = multilang.languages[widget.selection]
      multilang.change_language(code)
      effects.effects.append({'language': code})

  gui_app.pop_widget = lambda: effects.effects.append({'pop': True})
  widget = MultiOptionDialog(
    tr('Select a language'), multilang.languages, multilang.codes[multilang.language], option_font_weight=FontWeight.UNIFONT, callback=select_language
  )
elif scene['kind'] == 'regulatory':
  from device_source import regulatory

  gui_app.pop_widget = lambda: effects.effects.append({'pop': True})
  widget = regulatory(Path(__file__).resolve().parents[3], gui_app, scene['config']['big'])
elif scene['kind'] in ['network-mici', 'wifi-mici']:
  from network_source import create

  widget, network = create(scene, state_module.ui_state, gui_app)
elif scene['kind'] == 'software':
  import openpilot.selfdrive.ui.layouts.settings.software as software_module

  class FixedDatetime(datetime.datetime):
    @classmethod
    def now(cls, tz=None):
      return datetime.datetime(2026, 10, 1, 12, 34, 56).astimezone().astimezone(tz)

  software_module.datetime = SimpleNamespace(datetime=FixedDatetime, UTC=datetime.UTC)
  software_module.system_time_valid = lambda: scene.get('time_valid', True)

  def updater_signal(command):
    assert command in ['pkill -SIGUSR1 -f system.updated.updated', 'pkill -SIGHUP -f system.updated.updated']
    effects.effects.append({'updater': 'Check' if '-SIGUSR1' in command else 'Download'})
    return 0

  software_module.os = SimpleNamespace(system=updater_signal)
  widget = software_module.SoftwareLayout()
elif scene['kind'] == 'developer':
  if scene['config']['big']:
    from openpilot.selfdrive.ui.layouts.settings.developer import DeveloperLayout
  else:
    import openpilot.selfdrive.ui.mici.layouts.settings.developer as developer_module

    developer_module.system_time_valid = lambda: scene.get('time_valid', True)
    DeveloperLayout = developer_module.DeveloperLayoutMici
  widget = DeveloperLayout()
elif scene['kind'] == 'device':
  from device_source import device_layout, mici_device_layout

  root = Path(__file__).resolve().parents[3]
  widget = (
    device_layout(root, Params, state_module.ui_state, gui_app)
    if scene['config']['big']
    else mici_device_layout(root, Params, state_module.ui_state, gui_app, effects, scene)
  )
elif scene['kind'] == 'toggles':
  if scene['config']['big']:
    from openpilot.selfdrive.ui.layouts.settings.toggles import TogglesLayout
  else:
    from openpilot.selfdrive.ui.mici.layouts.settings.toggles import TogglesLayoutMici as TogglesLayout
  widget = TogglesLayout()
elif scene['kind'] == 'firehose':
  if scene['config']['big']:
    from openpilot.selfdrive.ui.layouts.settings.firehose import FirehoseLayout
  else:
    from openpilot.selfdrive.ui.mici.layouts.settings.firehose import FirehoseLayout
  widget = FirehoseLayout()
elif scene['kind'] == 'ssh':
  from openpilot.selfdrive.ui.widgets.ssh_key import SshKeyAction

  widget = SshKeyAction()
elif scene['kind'] == 'setup':
  from openpilot.selfdrive.ui.widgets.setup import SetupWidget

  widget = SetupWidget()
elif scene['kind'] == 'pairing':
  if scene['config']['big']:
    from openpilot.selfdrive.ui.widgets.pairing_dialog import PairingDialog
  else:
    from openpilot.selfdrive.ui.mici.widgets.pairing_dialog import PairingDialog
  widget = PairingDialog()
  widget._get_pairing_url = lambda: 'https://connect.comma.ai/?pair=fixture'
elif scene['kind'] == 'prime':
  widget = PrimeWidget()
else:
  widget = CarrotWebDialog()
  widget._session._timestamp_factory = lambda: '12:34:56'
if scene.get('ssh_host'):
  import requests

  original_get = requests.get

  def owned_get(url, *args, **kwargs):
    assert url.startswith('https://github.com/')
    return original_get(scene['ssh_host'] + url.removeprefix('https://github.com'), *args, **kwargs)

  requests.get = owned_get
now = 0.0
original = time.monotonic
time.monotonic = lambda: now
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
rect = rl.Rectangle(*(scene['rect'][key] for key in ['x', 'y', 'width', 'height']))
widget.set_rect(rect)
widget.show_event()
loop = gui_app.render()
results = []
try:
  for index in range(scene['frames']):
    now = index / 20
    if network is not None:
      network.before(next((step for step in scene.get('steps', []) if step['frame'] == index), {}))
    step = effects.before(index, widget)
    if step.get("show_again"):
      widget.hide_event()
      widget.show_event()
    next(loop)
    gui_app._mouse_events = [
      MouseEvent(MousePos(event['pos']['x'], event['pos']['y']), event['slot'], event['pressed'], event['released'], event['down'], event['time'])
      for event in step.get('events', [])
    ]
    if gui_app._mouse_events:
      gui_app._last_mouse_event = gui_app._mouse_events[-1]
    rl.get_mouse_position = lambda: rl.Vector2(*gui_app.last_mouse_event.pos)
    rl.get_mouse_wheel_move = lambda step=step: step.get('wheel', 0.0)
    widget.render()
    results.append(
      {
        'callbacks': list(dialog_results),
        'dismissing': widget.is_dismissing,
        'text': widget._keyboard.text() if hasattr(widget, '_keyboard') else None,
        'candidate': widget._keyboard.get_candidate_character() if hasattr(widget, '_keyboard') else None,
      }
      if 'dialog' in scene
      else effects.snapshot()
      if scene.get('capture_effects')
      else {'prime': scene['prime']}
    )
    if network is not None:
      results[-1]['network'] = network.state(widget)
    if index in scene.get('capture_frames', []):
      rl.rl_draw_render_batch_active()
      capture = rl.load_image_from_screen()
      assert rl.export_image(capture, str(output.with_name(f'{output.stem}-frame-{index:04}.png')))
      rl.unload_image(capture)
  rl.rl_draw_render_batch_active()
  image = rl.load_image_from_screen()
  assert rl.export_image(image, str(output))
  rl.unload_image(image)
  output.with_suffix('.json').write_text(json.dumps(results))
  loop.close()
finally:
  time.monotonic = original
  gui_app.close()
