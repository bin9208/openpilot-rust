"""Temporary original S3 lifecycle using unchanged command/encryption methods."""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import struct
import sys
from types import ModuleType, SimpleNamespace
from Crypto.Cipher import DES


def main():
  parser = argparse.ArgumentParser(); parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args(); args.output.mkdir(parents=True, exist_ok=False)
  manifest = json.loads((Path(__file__).resolve().parents[1] / 'upstream/cluster-wifi-source.json').read_text())
  prefix = 'openpilot/selfdrive/carrot/cluster/'
  def read(path):
    item = manifest['files'][prefix + path]; data = item['content'].encode()
    assert hashlib.sha256(data).hexdigest() == item['sha256']
    assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == item['git_blob']
    return data.decode()
  inputs, expected = [], []
  for fault in ['none', 'connect', 'sync_send', 'sync_result', 'sync_sequence', 'brightness_send', 'close_send', 'close_result']:
    calls = []
    time = SimpleNamespace(time=lambda: 3.21, mktime=lambda _: 0, localtime=lambda: (1970, 1, 1), sleep=lambda seconds: calls.append(['sleep_ms', int(seconds * 1000)]))
    tree = ast.parse(read('.vendor/turing-smart-screen-python-main/library/lcd/lcd_comm_turing_usb.py'))
    functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in ['build_command_packet_header','encrypt_with_des','encrypt_command_packet']]
    scope = dict(time=time, struct=struct, DES=DES)
    exec(compile(ast.Module(body=functions, type_ignores=[]), 'final TURZX helper methods', 'exec'), scope)
    usb_tree = ast.parse(read('cluster_usb_display.py'))
    base = next(node for node in usb_tree.body if isinstance(node, ast.ClassDef) and node.name == 'TuringUsbDisplay')
    command = next(node for node in base.body if isinstance(node, ast.FunctionDef) and node.name == '_send_command')
    command_scope = dict(time=time, USB_COMMAND_GAP_S=.2, USB_COMMAND_TIMEOUT_MS=2000, print=lambda text: calls.append(['print', text]))
    exec(compile(ast.Module(body=[command], type_ignores=[]), 'final _send_command', 'exec'), command_scope)
    class Lock:
      def __enter__(self): pass
      def __exit__(self, *_): pass
    class Base:
      def __init__(self, **_):
        self._usb_lock = Lock(); self._disconnected = False; self.expected_product_id = None; self.brightness = 80
        self._build_command_packet_header = scope['build_command_packet_header']
        self._encrypt_command_packet = scope['encrypt_command_packet']
      def _load_protocol_helpers(self): pass  # Static native linking replaces only this import boundary.
      _send_command = command_scope['_send_command']
    class Connection:
      def __init__(self, _):
        calls.append(['connect']); self.sequence = 0
        if fault == 'connect': raise ConnectionError('connect failed')
      def send(self, payload, *, expect_reply):
        calls.append(['send', bytes(payload).hex(), expect_reply])
        phase = ['sync', 'brightness', 'close'][self.sequence]
        if fault == phase + '_send': raise OSError('send failed')
        self.sequence += 1; return self.sequence
      def result(self):
        calls.append(['result'])
        phase = ['sync', 'brightness', 'close'][self.sequence - 1]
        if fault == phase + '_result': raise OSError('result failed')
        return self.sequence + (fault == phase + '_sequence'), bytes([self.sequence]), 123
      def close(self): calls.append(['close'])
    usb, protocol = ModuleType('cluster_usb_display'), ModuleType('cluster_wifi_protocol')
    usb.TuringUsbDisplay = Base; usb._set_cluster_hud_connected = lambda value: calls.append(['connected', value])
    protocol.Connection = Connection; protocol.load_config = lambda: None
    old = {name: sys.modules.get(name) for name in [usb.__name__, protocol.__name__]}
    sys.modules.update({usb.__name__:usb, protocol.__name__:protocol})
    module = ModuleType('final_wifi_display')
    try: exec(compile(read('cluster_wifi_display.py'), 'final cluster_wifi_display', 'exec'), module.__dict__)
    finally:
      for name, previous in old.items():
        if previous is None: sys.modules.pop(name, None)
        else: sys.modules[name] = previous
    display = module.S3WifiDisplay({})
    result = {}
    try: display.open(); result['opened'] = True
    except RuntimeError as error: result.update(error=str(error))
    display.close()
    result.update(calls=calls, pending=list(display.pending), connected=display.connection is not None, disconnected=display._disconnected)
    inputs.append(dict(fault=fault)); expected.append(result)
  (args.output / 'lifecycle.json').write_text(json.dumps(dict(source_commit=manifest['source_commit'], scope='unchanged S3 lifecycle/command/encryption; inert channel and frozen wall time; static helper import boundary omitted',input=inputs,expected=expected),indent=2)+'\n')
  print(json.dumps(dict(cases=len(inputs),scope='original S3 open/close complete command bytes/order/delays/errors')))


if __name__ == '__main__': main()
