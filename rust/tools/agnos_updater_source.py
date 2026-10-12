"""Unchanged source updater widgets with owned command, Params and Wi-Fi boundaries."""
import json
import os
from pathlib import Path
import sys
import threading
import time
from types import ModuleType, SimpleNamespace

root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / 'rust/tools/ui_application_qa'))
scene = json.loads(Path(sys.argv[1]).read_text())
output = Path(sys.argv[2]).resolve()
os.environ.update(BIG='1' if scene['config']['big'] else '0', SCALE='1', OFFSCREEN='1')
hardware = ModuleType('openpilot.system.hardware')
hardware.__path__ = [str(root / 'openpilot/system/hardware')]
hardware.PC = True
hardware.TICI = scene['config']['large_viewport']
reboots = []
hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: 'pc', get_os_version=lambda: 'owned-fixture', reboot=lambda: reboots.append(True), get_network_type=lambda: 0)
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://owned-updater', swaglog_root=lambda: str(output.parent / 'logs'))
sys.modules[paths.__name__] = paths
from product_params import install
scene.update(params={}, prime=0)
Params = install(scene)
ui_module = ModuleType('openpilot.selfdrive.ui.ui_state')
ui_module.ui_state = SimpleNamespace(prime_state=SimpleNamespace(get_type=lambda: 0), params=Params())
sys.modules[ui_module.__name__] = ui_module
from network_source import prepare
manager = prepare(scene, ui_module.ui_state)
import pyray as rl
from openpilot.system.ui.lib.application import gui_app, MouseEvent, MousePos
from openpilot.system.ui.lib.multilang import multilang
multilang._language = 'en'
multilang.setup()
gui_app.init_window('Source AGNOS updater')
class Monitor:
  def __init__(self, *a, **k):
    self.network_connected = threading.Event()
    self.wifi_connected = threading.Event()
    self.recheck_event = threading.Event()
  def start(self): pass
  def stop(self): pass
  def invalidate(self):
    self.recheck_event.set()
    self.network_connected.clear()
  def reset(self): self.network_connected.clear()
now = 0.0
time.monotonic = lambda: now
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
if scene['state']['layout'] == 'mici':
  import openpilot.system.ui.mici_setup as setup
  setup.NetworkConnectivityMonitor = Monitor
  import openpilot.system.ui.mici_updater as updater_module
else:
  import openpilot.system.ui.tici_updater as updater_module
updater = updater_module.Updater(str(output.parent / 'owned-updater'), str(output.parent / 'owned-manifest.json'))
if scene['state']['layout'] == 'mici':
  gui_app.push_widget(updater)
  page = {'root': None, 'network': updater._network_setup_page, 'failed': None, 'wifi': updater._network_setup_page._wifi_ui}.get(scene.get('page'), updater._progress_page)
  if page is not None: gui_app.push_widget(page)
else:
  if scene.get('page') == 'network': updater.current_screen = updater_module.Screen.WIFI
  gui_app.push_widget(updater)
loop = gui_app.render()
results = []
connected = False
for index in range(scene['frames']):
  now = index / 20
  step = next((s for s in scene.get('steps', []) if s['frame'] == index), {})
  manager.before(step)
  state = step.get('state', scene['state'])
  updater.progress_text = state['text']
  updater.progress_value = state['progress']
  if scene['state']['layout'] == 'tici':
    updater.failure_detail = state.get('failure') or ''
    updater.show_reboot_button = state.get('failure') is not None
  else:
    updater._failed_page.set_reason(state.get('failure') or '')
    if state.get('failure') and index == 0:
      updater._failure_reason = state['failure']
    if 'connected' in step: connected = step['connected']
    if connected: updater._network_monitor.network_connected.set()
    else: updater._network_monitor.network_connected.clear()
  active = gui_app.get_active_widget()
  if 'scroll' in step and hasattr(active, '_scroller'):
    active._scroller.scroll_to(step['scroll'], smooth=False)
  events = [MouseEvent(MousePos(e['pos']['x'], e['pos']['y']), e['slot'], e['pressed'], e['released'], e['down'], e['time']) for e in step.get('events', [])]
  gui_app._mouse._handle_mouse_event = lambda: None
  gui_app._mouse.get_events = lambda: events
  rl.get_mouse_position = lambda: rl.Vector2(*gui_app.last_mouse_event.pos)
  rl.is_mouse_button_down = lambda button: gui_app.last_mouse_event.left_down
  next(loop)
  results.append({'commands': list(manager.commands), 'reboots': len(reboots), 'stack': len(gui_app._nav_stack), 'text': updater.progress_text, 'progress': updater.progress_value})
  if index in scene.get('capture_frames', []):
    rl.rl_draw_render_batch_active()
    image = rl.load_image_from_screen()
    assert rl.export_image(image, str(output.with_name(f'{output.stem}-frame-{index:04}.png')))
    rl.unload_image(image)
rl.rl_draw_render_batch_active()
image = rl.load_image_from_screen()
assert rl.export_image(image, str(output))
rl.unload_image(image)
output.with_suffix('.json').write_text(json.dumps(results))
loop.close()
from PIL import ImageGrab
position = rl.get_window_position()
ImageGrab.grab(xdisplay=os.environ['DISPLAY']).crop((round(position.x), round(position.y), round(position.x) + rl.get_screen_width(), round(position.y) + rl.get_screen_height())).save(output.with_name(output.stem + '-composited.png'))
updater.close()
gui_app.close()
