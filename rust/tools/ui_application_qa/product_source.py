"""Actual product widget render oracle with owned data boundaries."""

import json
import ast
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
    if key == "CarParamsPersistent":
      return car_bytes(scene)
    value = scene.get('params', {}).get(key)
    if key == 'LongitudinalPersonality':
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

  def put_nonblocking(self, key, value):
    self.put(key, value)

  def remove(self, key):
    scene.setdefault('params', {}).pop(key, None)

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
state_module.device = SimpleNamespace(_awake=True, awake=True)
state_module.ui_state = SimpleNamespace(
  started=False,
  params=Params(),
  engaged=False,
  CP=None,
  has_longitudinal_control=False,
  personality=1,
  update_params=lambda: None,
  add_engaged_transition_callback=engaged_callbacks.append,
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
if scene['kind'] == 'toggles':
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
    step = effects.before(index, widget)
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
    results.append(effects.snapshot() if scene.get('capture_effects') else {'prime': scene['prime']})
  rl.rl_draw_render_batch_active()
  image = rl.load_image_from_screen()
  assert rl.export_image(image, str(output))
  rl.unload_image(image)
  output.with_suffix('.json').write_text(json.dumps(results))
  loop.close()
finally:
  time.monotonic = original
  gui_app.close()
