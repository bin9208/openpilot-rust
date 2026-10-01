"""Unchanged setup/teardown bodies compared with the native PTY command transcript."""
import ast
import json
from pathlib import Path
import re
import struct
import tempfile
from types import SimpleNamespace

from qcomgps_reference import SOURCE, Source


def expected_sequence():
  source = Source()
  scope = source.scope
  tree = ast.parse((SOURCE / 'modemdiag.py').read_text())
  tree.body = [node for node in tree.body if isinstance(node, (ast.Assign, ast.FunctionDef))]
  exec(compile(tree, str(SOURCE / 'modemdiag.py'), 'exec'), scope)
  # Reload the unchanged setup bodies after the publication harness substitutions.
  tree = ast.parse((SOURCE / 'qcomgpsd.py').read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in ['setup_quectel', 'teardown_quectel']]
  exec(compile(tree, str(SOURCE / 'qcomgpsd.py'), 'exec'), scope)
  commands, packets = [], []
  gps = True
  response = None
  def at(command):
    nonlocal gps
    commands.append(command)
    if command == 'AT+QGPS?':
      return f'+QGPS: {int(gps)}'
    if command == 'AT+QGPSEND':
      gps = False
    elif command == 'AT+QGPS=1':
      gps = True
    return ''
  def send(opcode, data):
    nonlocal response
    packets.append([opcode] + list(data))
    if opcode == 115:
      operation, = struct.unpack_from('<I', data, 3)
      response = struct.pack('<3xII', operation, 0)
      if operation == 1:
        response += struct.pack('<16I', 0, 0x500, 0, 9, *([0] * 12))
    else:
      response = data
    response = opcode, response
  diag = SimpleNamespace(send=send, recv=lambda: response)
  scope.update(at_cmd=at, system_time_valid=lambda: True, inject_assistance=lambda: None)
  with tempfile.TemporaryDirectory(prefix='qcomgps-source-setup-') as temporary:
    file = Path(temporary) / 'assist'
    scope['ASSIST_DATA_FILE'] = str(file)
    assert scope['setup_quectel'](diag) is False
    file.write_bytes(b'assistance')
    assert scope['setup_quectel'](diag) is True
    scope['teardown_quectel'](diag)
  return commands, packets


def compare(transcript, output):
  actual = json.loads(transcript.read_text())
  commands, packets = expected_sequence()
  normalize = lambda command: re.sub(r'AT\+QGPSXTRATIME=0,"[^\"]+"', 'AT+QGPSXTRATIME=0,"<UTC>"', command)
  assert list(map(normalize, actual['at'][1:])) == list(map(normalize, commands)), (actual['at'], commands)
  assert actual['diagnostic'] == packets
  output.write_text(json.dumps({'source_at': commands, 'source_diagnostic': packets, 'native': actual, 'pass': True}, indent=2) + '\n')
