"""Owned Wi-Fi transport boundary for the unmodified compact network widgets."""

import ast
import copy
from dataclasses import dataclass
from enum import IntEnum
from pathlib import Path
import sys
from types import ModuleType


def create(scene, ui, gui):
  module = ModuleType('openpilot.system.ui.lib.wifi_manager')
  sys.modules[module.__name__] = module
  module.dataclass = dataclass
  module.IntEnum = IntEnum
  source = ast.parse((Path(__file__).resolve().parents[3] / 'openpilot/system/ui/lib/wifi_manager.py').read_text())
  nodes = [
    node
    for node in source.body
    if isinstance(node, (ast.ClassDef, ast.FunctionDef))
    and node.name in ['SecurityType', 'MeteredType', 'ConnectStatus', 'Network', 'WifiState', 'normalize_ssid']
  ]
  exec(compile(ast.Module(body=nodes, type_ignores=[]), 'wifi_manager.py', 'exec'), module.__dict__)

  def networks(values):
    return [module.Network(n['ssid'], n['strength'], module.SecurityType[n['security_type'].upper()], n['is_tethering']) for n in values]

  class Manager:
    def __init__(self):
      self.snapshot = copy.deepcopy(scene['wifi'])
      self.events = []
      self.commands = []
      self.callbacks = {}

    @property
    def networks(self):
      return networks(self.snapshot['networks'])

    @property
    def wifi_state(self):
      state = self.snapshot['wifi_state']
      return module.WifiState(state['ssid'], module.ConnectStatus[state['status'].upper()])

    @property
    def connecting_to_ssid(self):
      return self.snapshot['connecting_to_ssid']

    @property
    def connected_ssid(self):
      return self.snapshot['connected_ssid']

    @property
    def ipv4_address(self):
      return self.snapshot['ipv4_address']

    @property
    def tethering_password(self):
      return self.snapshot['tethering_password']

    @property
    def current_network_metered(self):
      return module.MeteredType[self.snapshot['current_network_metered'].upper()]

    def is_tethering_active(self):
      return self.snapshot['tethering_active']

    def is_connection_saved(self, ssid):
      return ssid in self.snapshot['saved_ssids']

    def add_callbacks(self, **callbacks):
      for key, callback in callbacks.items():
        self.callbacks.setdefault(key, []).append(callback)

    def process_callbacks(self):
      events, self.events = self.events, []
      for event in events:
        if isinstance(event, str):
          continue
        ((kind, value),) = event.items()
        key = {'NetworksUpdated': 'networks_updated', 'NeedAuth': 'need_auth', 'Forgotten': 'forgotten'}[kind]
        for callback in self.callbacks.get(key, []):
          callback(networks(value) if kind == 'NetworksUpdated' else value)

    def set_active(self, value):
      self.commands.append({'SetActive': value})

    def set_ipv4_forward(self, value):
      self.commands.append({'SetIpv4Forward': value})

    def set_tethering_active(self, value):
      self.commands.append({'SetTetheringActive': value})

    def set_tethering_password(self, value):
      self.commands.append({'SetTetheringPassword': value})

    def set_current_network_metered(self, value):
      self.commands.append({'SetCurrentNetworkMetered': value.name.title()})

    def set_connecting(self, ssid):
      self.snapshot['wifi_state'] = {'ssid': ssid, 'status': 'Connecting'}
      self.snapshot['connecting_to_ssid'] = ssid
      self.snapshot['connected_ssid'] = None

    def connect_to_network(self, ssid, password, hidden=False):
      self.set_connecting(ssid)
      self.commands.append({'Connect': {'ssid': ssid, 'password': password, 'hidden': hidden}})

    def activate_connection(self, ssid):
      self.set_connecting(ssid)
      self.commands.append({'Activate': ssid})

    def forget_connection(self, ssid):
      self.commands.append({'Forget': ssid})

    def before(self, step):
      if 'wifi' in step:
        self.snapshot = copy.deepcopy(step['wifi'])
      self.events.extend(copy.deepcopy(step.get('wifi_events', [])))

    def state(self, widget):
      return {'commands': list(self.commands), 'forgetting': widget.any_network_forgetting if hasattr(widget, 'any_network_forgetting') else None}

  manager = Manager()
  module.WifiManager = lambda: manager
  ui.prime_state.get_type = lambda: scene['prime']
  if scene['kind'] == 'network-mici':
    from openpilot.selfdrive.ui.mici.layouts.settings.network.network_layout import NetworkLayoutMici

    widget = NetworkLayoutMici()
  else:
    from openpilot.selfdrive.ui.mici.layouts.settings.network.wifi_ui import WifiUIMici

    widget = WifiUIMici(manager)
    gui.add_nav_stack_tick(manager.process_callbacks)
  return widget, manager
