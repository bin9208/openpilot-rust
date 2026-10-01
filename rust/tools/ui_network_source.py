"""Render actual network.py widgets with a fixture Wi-Fi/Params boundary."""

import json
import os
from pathlib import Path
import sys
import time
from enum import IntEnum
from types import ModuleType, SimpleNamespace

scene = json.loads(Path(sys.argv[1]).read_text())
output = Path(sys.argv[2]).resolve()
os.environ.update(BIG='1', SCALE='1', OFFSCREEN='1')
hardware = ModuleType('openpilot.system.hardware')
hardware.PC = True
hardware.TICI = True
hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: 'pc')
sys.modules[hardware.__name__] = hardware
paths = ModuleType('openpilot.system.hardware.hw')
paths.Paths = SimpleNamespace(swaglog_ipc=lambda: 'inproc://ui-network-source', swaglog_root=lambda: str(output.parent / 'source-logs'))
sys.modules[paths.__name__] = paths
values = {}


class Params:
  def get_bool(self, key):
    return values.get(key, False)

  def put_bool(self, key, value):
    values[key] = value

  def get(self, key):
    return values.get(key)

  def put(self, key, value):
    values[key] = value

  def remove(self, key):
    values.pop(key, None)


params = ModuleType('openpilot.common.params')
params.Params = Params
params.UnknownKeyName = type('UnknownKeyName', (Exception,), {})
sys.modules[params.__name__] = params
state = ModuleType('openpilot.selfdrive.ui.ui_state')
state.ui_state = SimpleNamespace(prime_state=SimpleNamespace(get_type=lambda: 0))
state.device = SimpleNamespace(awake=True)
sys.modules[state.__name__] = state
prime = ModuleType('openpilot.selfdrive.ui.lib.prime_state')
prime.PrimeType = SimpleNamespace(NONE=0, LITE=1)
sys.modules[prime.__name__] = prime


class SecurityType(IntEnum):
  OPEN = 0
  WPA = 1
  WPA2 = 2
  WPA3 = 3
  UNSUPPORTED = 4


class MeteredType(IntEnum):
  UNKNOWN = 0
  YES = 1
  NO = 2


def network(value):
  return SimpleNamespace(**{**value, 'security_type': SecurityType[value['security_type'].upper()]})


class Wifi:
  def __init__(self):
    self.callbacks = []
    self.events = []
    self.commands = []
    self.set_snapshot(scene['snapshot'])

  def set_snapshot(self, value):
    self.snapshot = value
    self.ipv4_address = value['ipv4_address']
    self.wifi_state = SimpleNamespace(**value['wifi_state'])
    self.current_network_metered = MeteredType[value['current_network_metered'].upper()]
    self.connected_ssid = value['connected_ssid']
    self.tethering_password = value['tethering_password']

  def add_callbacks(self, **callbacks):
    self.callbacks.append(callbacks)

  def process_callbacks(self):
    while self.events:
      event = self.events.pop(0)
      if isinstance(event, str):
        name, value = event, None
      else:
        name, value = next(iter(event.items()))
      callback = {
        'NeedAuth': 'need_auth',
        'Activated': 'activated',
        'Forgotten': 'forgotten',
        'NetworksUpdated': 'networks_updated',
        'Disconnected': 'disconnected',
      }[name]
      if name == 'NetworksUpdated':
        value = [network(item) for item in value]
      for callbacks in self.callbacks:
        if callback in callbacks:
          callbacks[callback](*([] if name in ('Activated', 'Disconnected') else [value]))

  def is_connection_saved(self, ssid):
    return ssid in self.snapshot['saved_ssids']

  def is_tethering_active(self):
    return self.snapshot['tethering_active']

  def set_active(self, active):
    self.commands.append({'SetActive': active})

  def connect_to_network(self, ssid, password='', hidden=False):
    self.commands.append({'Connect': {'ssid': ssid, 'password': password, 'hidden': hidden}})

  def forget_connection(self, ssid):
    self.commands.append({'Forget': ssid})

  def activate_connection(self, ssid):
    self.commands.append({'Activate': ssid})

  def set_tethering_password(self, password):
    self.commands.append({'SetTetheringPassword': password})

  def set_tethering_active(self, active):
    self.commands.append({'SetTetheringActive': active})

  def set_current_network_metered(self, value):
    self.commands.append({'SetCurrentNetworkMetered': ['Unknown', 'Yes', 'No'][value]})

  def set_ipv4_forward(self, active):
    self.commands.append({'SetIpv4Forward': active})


wifi_module = ModuleType('openpilot.system.ui.lib.wifi_manager')
wifi_module.WifiManager = Wifi
wifi_module.SecurityType = SecurityType
wifi_module.MeteredType = MeteredType
wifi_module.Network = SimpleNamespace
wifi_module.normalize_ssid = lambda ssid: ssid.replace('’', "'")
sys.modules[wifi_module.__name__] = wifi_module
import pyray as rl
from openpilot.system.ui.lib.application import gui_app, MouseEvent, MousePos
from openpilot.system.ui.widgets import DialogResult
from openpilot.system.ui.widgets.network import WifiManagerUI, NetworkUI, AdvancedNetworkSettings

gui_app.init_window('Source network widgets')
now = 0.0
rl.get_time = lambda: now
rl.get_frame_time = lambda: 0.05
original_monotonic = time.monotonic
time.monotonic = lambda: now
manager = Wifi()
widget = {'wifi': WifiManagerUI, 'network': NetworkUI, 'advanced': AdvancedNetworkSettings}[scene['kind']](manager)
gui_app.push_widget(widget)
results = []
for index, frame in enumerate(scene['frames']):
  now = index / 20
  if frame.get('snapshot'):
    manager.set_snapshot(frame['snapshot'])
  manager.events.extend(frame.get('events', []))
  gui_app._mouse_events = [
    MouseEvent(MousePos(event['pos']['x'], event['pos']['y']), event['slot'], event['pressed'], event['released'], event['down'], event['time'])
    for event in frame.get('touch', [])
  ]
  if gui_app._mouse_events:
    gui_app._last_mouse_event = gui_app._mouse_events[-1]
  operation = frame.get('operation', '')
  if operation in ('choose', 'forget'):
    selected = next(network for network in widget._networks if network.ssid == frame['ssid'])
    (widget._networks_buttons_callback if operation == 'choose' else widget._forget_networks_buttons_callback)(selected)
  elif operation == 'password':
    widget.keyboard.set_text(frame['text'])
    widget._on_password_entered(widget._state_network, DialogResult.CONFIRM)
  elif operation == 'forgot':
    widget.on_forgot_confirm_finished(widget._state_network, DialogResult.CONFIRM)
  chars = list(map(ord, frame.get("chars", "")))
  rl.get_char_pressed = lambda chars=chars: chars.pop(0) if chars else 0
  rl.get_mouse_wheel_move = lambda frame=frame: frame.get("wheel", 0.0)
  rl.begin_drawing()
  rl.clear_background(rl.BLACK)
  for current in gui_app._nav_stack[-1:]:
    current.render(rl.Rectangle(0, 0, 2160, 1080))
  state = None
  if scene['kind'] == 'wifi':
    state = {
      'phase': ['Idle', 'Connecting', 'NeedsAuth', 'ShowForgetConfirm', 'Forgetting'][widget.state],
      'network': widget._state_network.ssid if widget._state_network else None,
      'retry': widget._password_retry,
      'ip': widget.ip_address,
      'keyboard': widget.keyboard.text,
    }
  results.append(
    {
      'state': state,
      'stack': len(gui_app._nav_stack),
      'commands': list(manager.commands),
      'apn': values.get('GsmApn'),
      'roaming': values.get('GsmRoaming', False),
      'metered': values.get('GsmMetered', False),
    }
  )
  if index + 1 == len(scene['frames']):
    rl.rl_draw_render_batch_active()
    capture = rl.load_image_from_screen()
    assert rl.export_image(capture, str(output))
    rl.unload_image(capture)
  rl.end_drawing()
output.with_suffix('.json').write_text(json.dumps(results, indent=2))
time.monotonic = original_monotonic
gui_app.close()
