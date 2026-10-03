"""Actual product widget render oracle with owned data boundaries."""

import json
import ast
import datetime
import os
from pathlib import Path
import sys
import time
from types import ModuleType, SimpleNamespace
from product_effects import attach_refresh, Effects
from product_params import install

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
Params = install(scene)
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
dialog_results, network, egpu = [], None, None
if scene.get('root') is not None:
  from root_source import create

  now = 0.0
  time.monotonic = lambda: now
  rl.get_time = lambda: now
  widget, camera = create(scene,output,SimpleNamespace(ui=state_module.ui_state,gui=gui_app,params=Params,effects=effects,device=state_module.device))
elif scene.get('road') is not None:
  from road_source import create

  widget, camera = create(scene, state_module.ui_state)
elif scene.get('hud') is not None:
  from hud_source import create

  widget = create(scene, state_module.ui_state)
elif scene.get('plot') is not None:
  from plot_source import create

  widget=create(scene,state_module.ui_state)
elif scene.get('exp') is not None:
  from exp_source import create

  widget=create(scene,state_module.ui_state)
elif scene.get('vision') is not None:
  from vision_source import create

  widget=create(scene,state_module.ui_state)
elif scene.get('indicator') is not None:
  from indicator_source import create

  widget = create(scene, state_module.ui_state)
elif scene.get('alert') is not None:
  from alert_source import create

  widget = create(scene, state_module.ui_state)
elif scene.get('camera') is not None:
  from camera_source import create

  if scene.get('driver') and (scene['config']['big'] or scene['driver'].get('navigation')):
    gui_app.pop_widget = lambda: effects.effects.append({'pop': True})
  widget, camera = create(scene, state_module.ui_state)
elif 'dialog' in scene or scene['kind'] == 'language':
  from product_forms_source import create

  widget = create(scene, effects, dialog_results)
elif scene['kind'] == 'regulatory':
  from device_source import regulatory

  gui_app.pop_widget = lambda: effects.effects.append({'pop': True})
  widget = regulatory(Path(__file__).resolve().parents[3], gui_app, scene['config']['big'])
elif scene['kind'] in ['network-mici', 'wifi-mici']:
  from network_source import create

  widget, network = create(scene, state_module.ui_state, gui_app)
elif scene['kind'] == 'egpu':
  from egpu_source import create

  widget, egpu = create(scene, output)
elif scene['kind'] == 'settings-root':
  from settings_source import create

  widget, network, egpu = create(scene, output, SimpleNamespace(ui=state_module.ui_state, gui=gui_app, params=Params, effects=effects))
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
  from product_forms_source import redirect_ssh

  redirect_ssh(scene)
now = 0.0
original = time.monotonic
time.monotonic = lambda: now
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
rect = rl.Rectangle(*(scene['rect'][key] for key in ['x', 'y', 'width', 'height']))
widget.set_rect(rect)
if scene.get('root') is None:
  widget.show_event()
loop = gui_app.render()
results = []
try:
  for index in range(scene['frames']):
    if scene.get("camera") is not None:
      camera.before(index)
    now = index / 20
    if scene.get('plot') is not None:
      from plot_source import step as plot_step

      now=plot_step(scene,index)['now']
    if scene.get('alert') is not None:
      from alert_source import monotonic

      now = monotonic(scene, index)
    if egpu is not None:
      egpu.before(index)
    if network is not None:
      network.before(next((step for step in scene.get('steps', []) if step['frame'] == index), {}))
    step = effects.before(index, widget)
    if scene.get('root') is not None:
      from root_source import before

      before(scene,state_module.ui_state,widget,index)
    elif scene.get('road') is not None:
      from road_source import before

      before(scene,state_module.ui_state,widget,index)
    if scene.get('alert') is not None:
      from alert_source import before

      before(scene, state_module.ui_state, index)
    if scene.get('indicator') is not None:
      from indicator_source import before

      before(scene, state_module.ui_state, widget, index)
    if scene.get('vision') is not None:
      from vision_source import before

      before(scene,state_module.ui_state,widget,index)
    if scene.get('exp') is not None:
      from exp_source import before

      before(scene,state_module.ui_state,index)
    if scene.get('hud') is not None:
      from hud_source import before

      before(scene, state_module.ui_state, widget, index)
    if scene.get('plot') is not None:
      from plot_source import before

      before(scene,state_module.ui_state,widget,index)
    if scene.get("driver") and not scene["config"]["big"] and not scene['driver'].get('navigation') and index == 20:
      widget.hide_event()
    if scene.get("driver") and not scene["config"]["big"] and not scene['driver'].get('navigation') and index == 21:
      widget.show_event()
    if step.get("show_again"):
      widget.hide_event()
      widget.show_event()
    scripted_events = [
      MouseEvent(MousePos(event['pos']['x'], event['pos']['y']), event['slot'], event['pressed'], event['released'], event['down'], event['time'])
      for event in step.get('events', [])
    ]
    if scene.get('root') is not None:
      gui_app._mouse._handle_mouse_event = lambda: None
      gui_app._mouse.get_events = lambda: scripted_events
      rl.get_mouse_position = lambda: rl.Vector2(*gui_app.last_mouse_event.pos)
      rl.is_mouse_button_down = lambda button: gui_app.last_mouse_event.left_down
    next(loop)
    if scene.get('root') is None:
      gui_app._mouse_events = scripted_events
      if scripted_events:
        gui_app._last_mouse_event = scripted_events[-1]
    if scene.get('background') is not None and scene.get('root') is None:
      rl.clear_background(rl.Color(*scene['background']))
    rl.get_mouse_position = lambda: rl.Vector2(*gui_app.last_mouse_event.pos)
    if scene['kind'] == 'settings-root':
      rl.is_mouse_button_down = lambda button: gui_app.last_mouse_event.left_down
    rl.get_mouse_wheel_move = lambda step=step: step.get('wheel', 0.0)
    rendered = widget.render() if scene.get('root') is None else None
    if scene.get("driver") and (scene["config"]["big"] or scene['driver'].get('navigation')) and index + 1 == scene["frames"]:
      widget.hide_event()
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
    if scene['kind'] == 'settings-root':
      results[-1]['settings'] = widget._current_panel.name.title() if scene['config']['big'] else None
    if scene.get('root') is not None:
      from root_source import snapshot

      results[-1]['root'] = snapshot(widget,scene['config']['big'])
    elif scene.get('camera') is not None:
      results[-1]['camera'] = camera.snapshot()
    if scene.get('alert') is not None:
      from alert_source import snapshot

      results[-1]['alert'] = snapshot(widget, state_module.ui_state, rendered)
    if scene.get('indicator') is not None:
      from indicator_source import snapshot

      results[-1]['indicator']=snapshot(scene, widget)
    if scene.get('vision') is not None:
      from vision_source import snapshot

      results[-1]['vision']=snapshot(state_module.ui_state,widget)
    if scene.get('exp') is not None:
      from exp_source import snapshot

      results[-1]['exp']=snapshot(widget)
    if scene.get('hud') is not None:
      from hud_source import snapshot

      results[-1]['hud'] = snapshot(widget)
    if scene.get('plot') is not None:
      from plot_source import snapshot

      results[-1]['plot']=snapshot(scene,state_module.ui_state,widget,index)
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
  if egpu is not None:
    output.with_suffix('.egpu.json').write_text(json.dumps(egpu.snapshot()))
    egpu.release.set()
  loop.close()
finally:
  time.monotonic = original
  gui_app.close()
