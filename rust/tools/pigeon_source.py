import ast
from datetime import datetime, UTC
from pathlib import Path
import types
import struct


def load():
  tree = ast.parse((Path(__file__).resolve().parents[2] / 'openpilot/system/ubloxd/pigeond.py').read_text())
  tree.body = [node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom)) and not isinstance(node, ast.If)]
  namespace = {'datetime': datetime, 'UTC': UTC, 'struct': struct}
  exec(compile(tree, 'source-pigeond', 'exec'), namespace)
  return namespace


def trace(request):
  source = load()
  rows = []
  now = 0.0
  responses = []
  resets = 0

  def monotonic():
    nonlocal now
    now += 0.2
    return now

  def sleep(seconds):
    nonlocal now
    rows.append(['sleep', seconds])
    now += seconds

  def send(data):
    nonlocal responses, resets
    rows.append(['send', list(data)])
    if request['failure'] == 'timeout':
      reply = b''
    elif data == b'\xb5\x62\x09\x14\x00\x00\x1d\x60':
      resets += 1
      reply = source['UBLOX_BACKUP_RESTORE_MSG'] + bytes([0, 0, 0, 2 if resets == 1 else 3])
    elif data == b'\xb5\x62\x09\x14\x04\x00\x00\x00\x00\x00\x21\xec':
      reply = source['UBLOX_SOS_NACK' if request['failure'] == 'nack' else 'UBLOX_SOS_ACK']
    elif data.startswith(b'\xb5\x62\x13'):
      reply = source['UBLOX_ASSIST_ACK']
    else:
      reply = source['UBLOX_NACK' if request['failure'] == 'nack' else 'UBLOX_ACK']
    responses = [reply]

  def receive():
    data = responses.pop(0) if responses else b''
    rows.append(['receive', list(data)])
    return data

  def valid():
    rows.append(['time'])
    return request['time_message'] is not None

  def token(key):
    assert key == 'AssistNowToken'
    rows.append(['token'])
    return request['token']

  def assist(token):
    rows.append(['assist', token])
    if request['failure'] == 'assist':
      raise OSError('synthetic HTTP failure')
    return [bytes(data) for data in request['assist']]

  source.update(
    time=types.SimpleNamespace(monotonic=monotonic, sleep=sleep),
    set_power=lambda value: rows.append(['power', value]),
    cloudlog=types.SimpleNamespace(**{level: lambda text, level=level: rows.append(['log', level, text]) for level in ('debug', 'info', 'warning', 'error')}),
    system_time_valid=valid,
    Params=lambda: types.SimpleNamespace(get=token),
    get_assistnow_messages=assist,
    datetime=types.SimpleNamespace(now=lambda tz: datetime(2026, 9, 30, 12, 34, 56, tzinfo=UTC)),
    signal=types.SimpleNamespace(SIGINT=2, signal=lambda *args: None),
  )
  pigeon = source['TTYPigeon'].__new__(source['TTYPigeon'])
  pigeon.send = send
  pigeon.receive = receive
  pigeon.set_baud = lambda value: rows.append(['baud', value])
  try:
    match request['mode']:
      case 'init':
        value = source['init'](pigeon)
      case 'initialize':
        value = source['init_pigeon'](pigeon)
      case 'reset':
        value = pigeon.reset_device()
      case 'save':
        value = source['save_almanac'](pigeon)
      case _:
        raise AssertionError('unknown mode')
    result = {'value': value}
  except TimeoutError:
    result = {'error': 'TimeoutError'}
  return {'result': result, 'trace': rows}
