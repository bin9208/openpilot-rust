import json
from pathlib import Path
import sys

from original_params_binding import load
from check_wifi_policy import network
from wifi_fixture.callback_gate import ForgottenCallbackGate


def main():
  binding, root = Path(sys.argv[1]), Path(sys.argv[2])
  params_module, _ = load(binding, 'ipc://' + str(root / 'source-log'), root / 'logs')
  params = params_module.Params(str(root / 'source-params'))
  params.put('DongleId', 'test-dongle')
  from openpilot.system.ui.lib import wifi_manager as source
  source.Params = lambda: params
  start = json.loads(sys.stdin.readline())
  assert start['address'].startswith('unix:')
  manager = source.WifiManager()
  gate = ForgottenCallbackGate(manager) if start.get('defer_final_forgotten', False) else None
  events = []
  manager.add_callbacks(need_auth=lambda ssid: events.append({'NeedAuth': ssid}), activated=lambda: events.append('Activated'),
    forgotten=lambda ssid: events.append({'Forgotten': ssid}), networks_updated=lambda rows: events.append({'NetworksUpdated': [network(row) for row in rows]}),
    disconnected=lambda: events.append('Disconnected'))
  try:
    for line in sys.stdin:
      request = json.loads(line)
      match request['op']:
        case 'arm_final_forget':
          assert gate is not None
          gate.armed = True
        case 'release_final_forget':
          assert gate is not None
          gate.released.set()
        case 'stop':
          manager.stop()
          print(json.dumps({'stopped': True}), flush=True)
          return
        case 'command':
          command, value = next(iter(request['value'].items()))
          match command:
            case 'SetActive':
              manager.set_active(value)
            case 'Connect':
              manager.connect_to_network(**value)
            case 'Forget':
              manager.forget_connection(value)
            case 'Activate':
              manager.activate_connection(value)
            case 'SetTetheringPassword':
              manager.set_tethering_password(value)
            case 'SetTetheringActive':
              manager.set_tethering_active(value)
            case 'SetCurrentNetworkMetered':
              manager.set_current_network_metered(source.MeteredType[value.upper()])
            case 'SetIpv4Forward':
              manager.set_ipv4_forward(value)
            case _:
              raise AssertionError(command)
        case 'snapshot':
          pass
        case _:
          raise AssertionError(request)
      manager.process_callbacks()
      print(json.dumps({'snapshot': {
        'networks': [network(row) for row in manager.networks],
        'wifi_state': {'ssid': manager.wifi_state.ssid, 'status': manager.wifi_state.status.name.capitalize()},
        'ipv4_address': manager.ipv4_address, 'current_network_metered': manager.current_network_metered.name.capitalize(),
        'connecting_to_ssid': manager.connecting_to_ssid, 'connected_ssid': manager.connected_ssid,
        'tethering_password': manager.tethering_password, 'tethering_active': manager.is_tethering_active(), 'saved_ssids': list(manager._connections),
      }, 'events': events, **({'callback_gate': gate.snapshot()} if gate is not None else {})}), flush=True)
      events.clear()
  finally:
    if gate is not None:
      gate.released.set()
    manager.stop()


if __name__ == '__main__':
  main()
