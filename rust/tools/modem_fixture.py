"""Owned PTY and harmless native-command fixture for modem.py/native comparisons."""

import json
import os
from pathlib import Path
import pty
import select
import sys
import threading

RESPONSES = {
  'AT+CGMI': ['Quectel'],
  'AT+CGSN': ['123456789012345'],
  'AT+QCCID': ['+QCCID: 8985235123456789012F'],
  'AT+CIMI': ['450081123456789'],
  'AT+GMR': ['EG25G_TEST'],
  'AT+CREG?': ['+CREG: 2,1'],
  'AT+CGREG?': ['+CGREG: 2,1'],
  'AT+CSQ': ['+CSQ: 23,99'],
  'AT+COPS?': ['+COPS: 0,0,"Fixture",7'],
  'AT+QNWINFO': ['+QNWINFO: "FDD LTE","45008","LTE BAND 3",1300'],
  'AT+QENG="servingcell"': ['+QENG: "servingcell","NOCONN","LTE"'],
  'AT+QTEMP': ['+QTEMP: 31,255,34'],
  'AT+CGCONTRDP=1': ['+CGCONTRDP: 1,5,"apn","10.0.0.2","10.0.0.1","1.1.1.1","8.8.8.8"'],
  'AT+BAD': ['+CME ERROR: 10'],
  'AT+PLAINERROR': ['ERROR'],
  'AT+BLANK': ['', '  ', '+TEST: 1'],
}


class Fixture:
  def __init__(self, root, changes=None):
    self.root = root
    self.master, self.slave = pty.openpty()
    self.at = root / 'at'
    self.at.symlink_to(os.ttyname(self.slave))
    self.responses = RESPONSES | (changes or {})
    self.commands = []
    self.stop = threading.Event()
    self.thread = threading.Thread(target=self.serve, daemon=True)
    self.thread.start()
    for folder in ('params', 'statistics'):
      (root / folder).mkdir()
    (root / 'statistics/tx_bytes').write_text('123')
    (root / 'statistics/rx_bytes').write_text('456')
    (root / 'manager.service').touch()
    script = Path(__file__).with_name('modem_fixture_command.py').resolve()
    for name in ('sudo', 'ip'):
      path = root / name
      path.write_text(f'#!{sys.executable}\nimport runpy, sys\nsys.argv.insert(1, {str(root)!r})\nrunpy.run_path({str(script)!r}, run_name="__main__")\n')
      path.chmod(0o755)
    self.config = {
      'at_port': str(self.at),
      'ppp_port': str(root / 'absent-data-port'),
      'state': str(root / 'state'),
      'lock': str(root / 'lock'),
      'params': str(root / 'params'),
      'statistics': str(root / 'statistics'),
      'modem_manager': str(root / 'manager.service'),
      'sudo': str(root / 'sudo'),
      'ip': str(root / 'ip'),
      'serial_timeout_ms': 100,
      'state_wait_ms': 30,
      'iccid_interval': 0.1,
    }

  def serve(self):
    buffered = b''
    while not self.stop.is_set():
      if not select.select([self.master], [], [], 0.05)[0]:
        continue
      try:
        buffered += os.read(self.master, 4096)
        while b'\r' in buffered:
          raw, buffered = buffered.split(b'\r', 1)
          command = raw.decode().strip()
          if not command:
            continue
          self.commands.append(command)
          if command == 'AT+TIMEOUT':
            continue
          lines = self.responses.get(command, [])
          tail = [] if lines and (lines[-1] == 'ERROR' or lines[-1].startswith('+CME ERROR')) else ['OK']
          os.write(self.master, ('\r\n' + '\r\n'.join(lines + tail) + '\r\n').encode())
      except OSError:
        break

  def close(self):
    self.stop.set()
    self.thread.join(timeout=1)
    os.close(self.master)
    os.close(self.slave)

  def calls(self):
    path = self.root / 'calls.jsonl'
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
