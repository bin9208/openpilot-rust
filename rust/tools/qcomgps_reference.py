"""Unchanged QCOM GNSS source bodies driven only by synthetic inputs."""
import ast
import datetime
import itertools
import math
import os
from pathlib import Path
import struct
from types import SimpleNamespace

from openpilot.cereal import log

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'openpilot/system/qcomgpsd'


def definitions(name, scope):
  tree = ast.parse((SOURCE / name).read_text())
  tree.body = [node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom, ast.If))]
  exec(compile(tree, str(SOURCE / name), 'exec'), scope)


def declarations():
  scope = {'unpack_from': struct.unpack_from, 'calcsize': struct.calcsize}
  definitions('structs.py', scope)
  return scope


class EndOfInput(Exception):
  pass


class Source:
  def __init__(self):
    self.output = []
    noop = lambda *args, **kwargs: None
    logger = SimpleNamespace(warning=noop, error=noop, debug=noop, info=noop)
    event = SimpleNamespace(set=noop)
    process = lambda **kwargs: SimpleNamespace(start=noop)
    self.scope = declarations() | {'__name__': 'qcomgps_reference', 'os': os, 'math': math,
      'datetime': datetime, 'time': SimpleNamespace(time=lambda: 0), 'itertools': itertools,
      'pack': struct.pack, 'unpack_from': struct.unpack_from, 'calcsize': struct.calcsize,
      'retry': lambda **kw: lambda f: f, 'ModemDiag': object, 'NoReturn': object,
      'log': log, 'cloudlog': logger, 'Event': lambda: event, 'Process': process,
      'gpio_init': noop, 'gpio_set': noop, 'GPIO': SimpleNamespace(GNSS_PWR_EN=34),
      'signal': SimpleNamespace(signal=noop, SIGINT=2, SIGTERM=15), 'DIAG_LOG_F': 16}
    definitions('qcomgpsd.py', self.scope)
    self.scope.update(wait_for_modem=noop, setup_quectel=lambda diag: True,
                      ASSIST_DATA_FILE='/nonexistent-qcomgps-oracle-assistance')
    self.scope['messaging'] = SimpleNamespace(PubMaster=lambda names: self, new_message=self.new_message)

  def new_message(self, service, valid):
    message = log.Event.new_message()
    message.logMonoTime = 123456789
    message.valid = valid
    message.init(service)
    return message

  def send(self, topic, message):
    self.output.append({'topic': topic, 'event': message.to_dict()})

  def publication(self, opcode, payload):
    pending = [(opcode, payload)]
    def receive():
      if pending:
        return pending.pop(0)
      raise EndOfInput
    self.scope['ModemDiag'] = lambda: SimpleNamespace(recv=receive)
    self.output.clear()
    try:
      self.scope['main']()
    except EndOfInput:
      return self.output[0] if self.output else {'ignored': True}
    except (AssertionError, struct.error, IndexError, ValueError, OverflowError) as error:
      return {'error': type(error).__name__}


def payload(table, replacements=None, seed=0):
  scope = declarations()
  fmt, names = scope['parse_struct'](scope[table])
  replace = replacements or {}
  values = []
  for index, (kind, name) in enumerate(zip(fmt[1:], names, strict=True)):
    value = replace.get(name)
    if value is None:
      if kind in 'fd':
        value = (index + 1) * 0.25 + seed
      elif kind in 'bh':
        value = -1 - seed
      else:
        value = (index + seed) % 64
    values.append(value)
  return struct.pack(fmt, *values)


def packet(kind, body, pending=0, timestamp=0x123456789abcdef0):
  inner = struct.pack('<HHQ', len(body) + 12, kind, timestamp) + body
  return struct.pack('<BH', pending, len(inner)) + inner
