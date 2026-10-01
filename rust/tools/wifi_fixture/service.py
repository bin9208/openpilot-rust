import copy
from queue import Empty, Queue
import threading

from jeepney import DBusAddress, new_error, new_method_return, new_signal
from jeepney.bus_messages import MatchRule, message_bus
from jeepney.io.blocking import open_dbus_connection

from openpilot.system.ui.lib.networkmanager import (NM, NM_PATH, NM_IFACE, NM_SETTINGS_PATH, NM_SETTINGS_IFACE,
  NM_ACTIVE_CONNECTION_IFACE, NM_WIRELESS_IFACE, NM_DEVICE_IFACE, NM_ACCESS_POINT_IFACE,
  NM_IP4_CONFIG_IFACE, NM_PROPERTIES_IFACE)


def plain(value):
  if isinstance(value, dict):
    return {key: ['s', 'UUID'] if key == 'uuid' else plain(item) for key, item in value.items()}
  if isinstance(value, (list, tuple, bytes)):
    return [plain(item) for item in value]
  return value


def settings(ssid, password='fixture-password'):
  return {'connection': {'type': ('s', '802-11-wireless'), 'id': ('s', ssid), 'metered': ('i', 2), 'fixture': ('s', 'preserve')},
          '802-11-wireless': {'ssid': ('ay', ssid.encode()), 'mode': ('s', 'infrastructure')},
          '802-11-wireless-security': {'psk': ('s', password), 'key-mgmt': ('s', 'wpa-psk')}}


class Service:
  def __init__(self, address, hotspot):
    self.address = address
    self.settings = {'/settings/A': settings('A')}
    if hotspot:
      self.settings['/settings/hotspot'] = settings('weedle-test')
    self.active = None
    self.device_state = 30
    self.calls = []
    self.scans = 0
    self.control = Queue()
    self.ready = threading.Event()
    self.stop = threading.Event()
    self.error = None
    self.thread = threading.Thread(target=self.run)

  def start(self):
    self.thread.start()
    assert self.ready.wait(5), self.error
    assert self.error is None, self.error

  def close(self):
    self.stop.set()
    self.thread.join(timeout=3)
    assert not self.thread.is_alive()
    assert self.error is None, self.error

  def signal(self, path, interface, member, signature, body):
    self.conn.send(new_signal(DBusAddress(path, interface=interface), member, signature, body))

  def state(self, current, previous, reason=0):
    self.device_state = current
    self.signal('/wifi', NM_DEVICE_IFACE, 'StateChanged', 'uuu', (current, previous, reason))

  def activate(self, path):
    self.active = path
    self.state(40, self.device_state)
    self.state(50, 40)
    self.state(100, 90)

  def properties(self, path, interface):
    match (path, interface):
      case (p, i) if p == NM_PATH and i == NM_IFACE:
        return {'ActiveConnections': ('ao', ['/active'] if self.active else [])}
      case ('/modem', i) if i == NM_DEVICE_IFACE:
        return {'DeviceType': ('u', 8)}
      case ('/wifi', i) if i == NM_DEVICE_IFACE:
        return {'DeviceType': ('u', 2), 'State': ('u', self.device_state)}
      case ('/wifi', i) if i == NM_WIRELESS_IFACE:
        return {'AccessPoints': ('ao', ['/ap/Aweak', '/ap/A', '/ap/B', '/ap/hotspot', '/ap/empty', '/ap/gone', '/ap/broken']), 'LastScan': ('x', self.scans)}
      case ('/active', i) if i == NM_ACTIVE_CONNECTION_IFACE:
        if not self.active:
          return None
        name = self.active.rsplit('/', 1)[1]
        return {'Connection': ('o', self.active), 'Type': ('s', '802-11-wireless'), 'Ip4Config': ('o', '/ip4'), 'SpecificObject': ('o', '/ap/' + name)}
      case ('/ip4', i) if i == NM_IP4_CONFIG_IFACE:
        return {'AddressData': ('aa{sv}', [{'address': ('s', '10.42.0.7'), 'prefix': ('u', 24)}])}
      case (p, i) if p.startswith('/ap/') and i == NM_ACCESS_POINT_IFACE:
        name = p.rsplit('/', 1)[1]
        if name == 'gone':
          return None
        ssid = {'Aweak': 'A', 'hotspot': 'weedle-test', 'empty': ''}.get(name, name)
        result = {'Ssid': ('ay', ssid.encode()), 'HwAddress': ('s', '00:00:00:00:00:01'),
                  'Strength': ('y', 20 if name == 'Aweak' else 70), 'Flags': ('u', 1), 'WpaFlags': ('u', 256), 'RsnFlags': ('u', 0)}
        if name == 'broken':
          result.pop('HwAddress')
        return result
      case _:
        return None

  def handle(self, message):
    path = str(message.header.fields[1])
    interface = str(message.header.fields[2])
    method = str(message.header.fields[3])
    body = message.body
    if interface == NM_PROPERTIES_IFACE:
      props = self.properties(path, body[0])
      if props is None or (method == 'Get' and body[1] not in props):
        self.conn.send(new_error(message, 'org.freedesktop.DBus.Error.UnknownObject', 's', ('fixture object gone',)))
      elif method == 'Get':
        self.conn.send(new_method_return(message, 'v', (props[body[1]],)))
      else:
        self.conn.send(new_method_return(message, 'a{sv}', (props,)))
      return
    self.calls.append({'path': path, 'interface': interface, 'method': method, 'body': plain(body)})
    match method:
      case 'GetDevices':
        self.conn.send(new_method_return(message, 'ao', (['/modem', '/wifi'],)))
      case 'ListConnections':
        self.conn.send(new_method_return(message, 'ao', (list(self.settings),)))
      case 'GetSettings' | 'GetSecrets':
        self.conn.send(new_method_return(message, 'a{sa{sv}}', (self.settings[path],)))
      case 'RequestScan':
        self.scans += 1
        self.conn.send(new_method_return(message))
        self.signal('/wifi', NM_PROPERTIES_IFACE, 'PropertiesChanged', 'sa{sv}as', (NM_WIRELESS_IFACE, {'LastScan': ('x', self.scans)}, []))
      case 'AddConnection':
        self.settings['/settings/hotspot'] = copy.deepcopy(body[0])
        self.conn.send(new_method_return(message, 'o', ('/settings/hotspot',)))
        self.signal(NM_SETTINGS_PATH, NM_SETTINGS_IFACE, 'NewConnection', 'o', ('/settings/hotspot',))
      case 'AddAndActivateConnection2':
        ssid = bytes(body[0]['802-11-wireless']['ssid'][1]).decode()
        path = '/settings/' + ssid
        self.settings[path] = copy.deepcopy(body[0])
        self.conn.send(new_method_return(message, 'ooa{sv}', (path, '/active', {})))
        self.signal(NM_SETTINGS_PATH, NM_SETTINGS_IFACE, 'NewConnection', 'o', (path,))
        self.activate(path)
      case 'ActivateConnection':
        self.conn.send(new_method_return(message, 'o', ('/active',)))
        self.activate(body[0])
      case 'Save':
        self.conn.send(new_method_return(message))
      case 'Update':
        self.settings[path] = copy.deepcopy(body[0])
        self.conn.send(new_method_return(message))
      case 'Delete':
        self.settings.pop(path, None)
        self.conn.send(new_method_return(message))
        self.signal(NM_SETTINGS_PATH, NM_SETTINGS_IFACE, 'ConnectionRemoved', 'o', (path,))
      case 'DeactivateConnection':
        self.active = None
        self.conn.send(new_method_return(message))
        self.state(30, 110, 39)
      case _:
        self.conn.send(new_error(message, 'org.freedesktop.DBus.Error.UnknownMethod'))

  def run(self):
    try:
      with open_dbus_connection(bus=self.address) as self.conn:
        self.conn.send_and_get_reply(message_bus.RequestName(NM))
        with self.conn.filter(MatchRule(type='method_call'), bufsize=512) as requests:
          self.ready.set()
          while not self.stop.is_set():
            try:
              action = self.control.get_nowait()
            except Empty:
              action = None
            if action == 'wrong-password':
              self.state(60, 50, 8)
            try:
              self.conn.recv_messages(timeout=.05)
            except TimeoutError:
              continue
            while requests:
              self.handle(requests.popleft())
    except (OSError, RuntimeError, ValueError, KeyError, TypeError) as error:
      self.error = repr(error)
      self.ready.set()
