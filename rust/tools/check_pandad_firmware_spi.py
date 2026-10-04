import argparse
import ast
import binascii
from collections.abc import Callable
from contextlib import contextmanager
import copy
import ctypes
from functools import reduce
from hashlib import sha256
import json
import math
from pathlib import Path
import runpy
import struct
import subprocess
import sys
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]


def source_classes():
  constants = runpy.run_path(str(ROOT / 'panda/python/constants.py'))
  namespace = {'binascii': binascii, 'ctypes': ctypes, 'math': math, 'struct': struct, 'reduce': reduce, 'Callable': Callable,
               'BaseHandle': object, 'BaseSTBootloaderHandle': object, 'TIMEOUT': 15000, 'USBPACKET_MAX_SIZE': 64,
               'McuType': constants['McuType'], 'MCU_TYPE_BY_IDCODE': constants['MCU_TYPE_BY_IDCODE']}
  filename = ROOT / 'panda/python/spi.py'
  tree = ast.parse(filename.read_text())
  names = {'SYNC', 'HACK', 'DACK', 'NACK', 'CHECKSUM_START', 'MIN_ACK_TIMEOUT_MS', 'MAX_XFER_RETRY_COUNT', 'XFER_SIZE'}
  nodes = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'crc8' or
           isinstance(node, ast.ClassDef) and node.name != 'SpiDevice' or
           isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id in names for target in node.targets)]
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(filename), 'exec'), namespace)
  filename = ROOT / 'panda/python/__init__.py'
  tree = ast.parse(filename.read_text())
  cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'Panda')
  cls.body = [node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name in {'spi_connect', 'flasher_present'}]
  exec(compile(ast.Module(body=[cls], type_ignores=[]), str(filename), 'exec'), namespace)
  namespace['Panda'].REQUEST_IN = 0xc0
  filename = ROOT / 'panda/python/dfu.py'
  tree = ast.parse(filename.read_text())
  cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'PandaDFU')
  cls.body = [node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name in {'program_bootstub', 'reset'}]
  exec(compile(ast.Module(body=[cls], type_ignores=[]), str(filename), 'exec'), namespace)
  return namespace


def original(namespace, case):
  calls = []
  clock = 0.0
  stage = None
  wire = case['wire']
  errors = {'nack': namespace['PandaSpiNackResponse'], 'missing_ack': namespace['PandaSpiMissingAck'],
            'checksum': namespace['PandaSpiBadChecksum'], 'protocol': namespace['PandaSpiException'], 'io': OSError}

  def record(*call):
    calls.append(list(call))
    if len(calls) - 1 == case['fail_at']:
      raise errors[case['fail_kind']]('scripted failure')

  class Spi:
    def xfer2(self, tx):
      tx = list(tx)
      record('xfer2', tx)
      if tx == [0x11]:
        return wire['hack']
      if tx == [0x13] * 68:
        return wire['dack']
      return [0] * len(tx)

    def xfer(self, tx):
      tx = list(tx)
      record('xfer', tx)
      if tx == [0]:
        return wire['dfu_ack']
      if len(tx) > 1 and not any(tx):
        return (wire['dfu_reply'] + [0] * len(tx))[:len(tx)]
      return [0] * len(tx)

    def readbytes(self, length):
      nonlocal stage
      record('read', length)
      if stage is False:
        if bytes(wire['version_echo']).startswith(b'VERSION'):
          stage = True
        return wire['version_echo']
      if stage is True:
        stage = None
        return wire['version_reply']
      return (wire['tail'] + [0] * length)[:length]

    def writebytes(self, tx):
      nonlocal stage
      record('write', list(tx))
      if bytes(tx) == b'VERSION':
        stage = False

    def fileno(self):
      return 17

  actor = Spi()

  class Device:
    def __init__(self, *args, **kwargs):
      self._spidev = actor

    @contextmanager
    def acquire(self):
      record('lock')
      try:
        yield actor
      finally:
        record('unlock')

    def close(self):
      pass

  class Logger:
    def debug(self, text, *args, **kwargs):
      record('log', text % args if args else text, kwargs.get('exc_info', False))

  def monotonic():
    nonlocal clock
    record('clock')
    clock += case['tick']
    return clock

  def sleep(seconds):
    nonlocal clock
    record('sleep', seconds)
    clock += seconds

  def ioctl(fd, command, transfer):
    assert fd == 17 and command == 0x80016b02
    assert transfer.tx_length <= 1024
    record('kernel', transfer.endpoint, list(ctypes.string_at(transfer.tx_buf, transfer.tx_length)),
           transfer.rx_length_max, bool(transfer.expect_disconnect), transfer.timeout)
    data = bytes(wire['kernel_reply'])
    assert len(data) <= 1024
    ctypes.memmove(transfer.rx_buf, data, len(data))
    return len(data)

  namespace.update({'SpiDevice': Device, 'logger': Logger(), 'time': SimpleNamespace(monotonic=monotonic, sleep=sleep),
                    'os': SimpleNamespace(environ={'KERN': '1'} if case['kernel'] else {}), 'fcntl': SimpleNamespace(ioctl=ioctl)})
  sys.modules['spidev2'] = SimpleNamespace(SPI_IOC_RD_LSB_FIRST=0x80016b02)
  try:
    operation = case['operation']
    value = None
    if operation == 'dfu_probe':
      value = namespace['STBootloaderSPIHandle']().get_mcu_type().name
    elif operation.startswith('dfu_'):
      dfu = namespace['STBootloaderSPIHandle'].__new__(namespace['STBootloaderSPIHandle'])
      dfu.dev, dfu._mcu_type = Device(), namespace['McuType'][case['mcu']]
      parts = None if case['parts'] is None else list(map(bytes, case['parts']))
      predata = None if case['predata'] is None else bytes(case['predata'])
      if operation == 'dfu_ack':
        dfu._get_ack(actor, case['ack_timeout'])
      elif operation in ('dfu_command', 'dfu_once'):
        value = (dfu._cmd if operation == 'dfu_command' else dfu._cmd_no_retry)(
          case['command'], parts, case['length'], predata)
      elif operation == 'dfu_read':
        value = dfu.read(case['address'], case['length'])
      elif operation == 'dfu_chip':
        value = dfu.get_chip_id()
      elif operation == 'dfu_uid':
        value = dfu.get_uid()
      elif operation == 'dfu_erase':
        dfu.erase_sector(case['sector'])
      elif operation == 'dfu_program':
        dfu.program(case['address'], bytes(case['data']))
      elif operation == 'dfu_jump':
        dfu.jump(case['address'])
      elif operation == 'dfu_recover':
        wrapper = namespace['PandaDFU'].__new__(namespace['PandaDFU'])
        wrapper._handle, wrapper._mcu_type = dfu, dfu._mcu_type
        wrapper.program_bootstub(bytes(case['data']))
        wrapper.reset()
    elif operation == 'identify':
      _, handle, serial, bootstub, _ = namespace['Panda'].spi_connect(case['serial'], case['ignore_version'])
      value = None if handle is None else {'serial': serial, 'bootstub': bootstub}
    else:
      spi = namespace['PandaSpiHandle']()
      if operation == 'ack':
        value = spi._wait_for_ack(actor, case['ack'], case['timeout'], case['tx'], case['length'])
      elif operation == 'once':
        value = spi._transfer_spidev(actor, case['endpoint'], bytes(case['data']), case['timeout'], case['maximum'], case['disconnect'])
      elif operation == 'transfer':
        value = spi._transfer(case['endpoint'], bytes(case['data']), case['timeout'], case['maximum'], case['disconnect'])
      elif operation == 'protocol':
        value = spi.get_protocol_version()
      elif operation == 'read':
        r = case['request']
        value = spi.controlRead(r['kind'], r['request'], r['value'], r['index'], case['length'], r['timeout_ms'])
      elif operation == 'write':
        r = case['request']
        value = spi.controlWrite(r['kind'], r['request'], r['value'], r['index'], bytes(case['data']), r['timeout_ms'], r['expect_disconnect'])
      elif operation == 'bulk_read':
        value = spi.bulkRead(case['endpoint'], case['length'], case['timeout'])
      elif operation == 'bulk_write':
        value = spi.bulkWrite(case['endpoint'], bytes(case['data']), case['timeout'])
    if isinstance(value, bytes):
      value = list(value)
    return {'ok': True, 'value': value, 'calls': calls}
  except Exception as error:
    kind = next((kind for kind, cls in errors.items() if type(error) is cls), None)
    if isinstance(error, namespace['PandaProtocolMismatch']):
      kind = 'version'
    elif kind is None and isinstance(error, (ValueError, IndexError, KeyError, TypeError, struct.error)):
      kind = 'invalid'
    return {'ok': False, 'error_kind': kind or type(error).__name__, 'calls': calls}


def cases(namespace):
  def frame(payload):
    header = [0x85, len(payload) & 255, len(payload) >> 8, *payload]
    checksum = 0xab
    for byte in header:
      checksum ^= byte
    full = [*header, checksum]
    return (full + [0] * 68)[:68], full[68:]

  def version(payload):
    echo = [*b'VERSION', len(payload) & 255, len(payload) >> 8]
    return echo, [*payload, namespace['crc8'](bytes(echo + payload))]

  dack, tail = frame([7, 8, 9])
  echo, reply = version([*range(12), 0xdd, 0xcc, 2])
  wire = {'hack': [0x79], 'dack': dack, 'tail': tail, 'version_echo': echo, 'version_reply': reply,
          'dfu_ack': [0x79], 'dfu_reply': [0, 1, 4, 0x83], 'kernel_reply': [7, 8, 9]}
  base = {'operation': 'transfer', 'wire': wire, 'fail_at': None, 'fail_kind': 'io', 'tick': 1 / 1024, 'kernel': False,
          'data': [1, 2], 'endpoint': 2, 'timeout': 30, 'maximum': 1000, 'disconnect': False,
          'request': {'kind': 0xc0, 'request': 0xc1, 'value': 1, 'index': 2, 'timeout_ms': 30, 'expect_disconnect': False},
          'serial': None, 'ignore_version': False, 'mcu': 'H7', 'address': 0x08000000, 'length': 3, 'sector': 2,
          'command': 0x31, 'parts': [[1, 2, 3], [4, 5]], 'predata': None, 'ack': 0x79, 'tx': 0x11, 'ack_timeout': 0.01}
  result = [{**base, 'operation': op} for op in ('transfer', 'once', 'read', 'write', 'bulk_read', 'bulk_write', 'protocol', 'identify',
             'dfu_probe', 'dfu_command', 'dfu_once', 'dfu_read', 'dfu_chip', 'dfu_uid', 'dfu_erase', 'dfu_program', 'dfu_jump', 'dfu_recover', 'dfu_ack')]
  result.append({**base, 'operation': 'ack', 'length': 1})
  for size in (0, 1, 63, 64, 65, 999, 1000, 1001, 1984):
    payload = [index % 256 for index in range(size)]
    dack, tail = frame(payload)
    for maximum in (0, 64, 1000, 1984):
      result.append({**base, 'maximum': maximum, 'wire': {**wire, 'dack': dack, 'tail': tail}})
  for size in (0, 1, 1983, 1984, 1985, 3970):
    result += [{**base, 'operation': 'bulk_write', 'data': [23] * size}, {**base, 'operation': 'bulk_read', 'length': size}]
  for disconnect in (False, True):
    for timeout in (0, 1, 30, 100):
      result.append({**base, 'disconnect': disconnect, 'timeout': timeout})
  for field in ('hack', 'dack'):
    for byte in (0, 0x1f):
      result.append({**base, 'wire': {**wire, field: [byte] + [0] * (len(wire[field]) - 1)}})
  bad = copy.deepcopy(wire)
  bad['dack'][6] ^= 1
  result.append({**base, 'wire': bad})
  for pid in (0, 0xcc, 0xee):
    for proto in (0, 1, 2, 3):
      echo, reply = version([*range(12), 0xdd, pid, proto])
      for ignore in (False, True):
        result.append({**base, 'operation': 'identify', 'ignore_version': ignore,
                       'wire': {**wire, 'version_echo': echo, 'version_reply': reply}})
  for field, value in (('version_echo', [0] * 9), ('version_echo', [*b'VERSION', 0xff, 0xff]),
                       ('version_reply', wire['version_reply'][:-1] + [wire['version_reply'][-1] ^ 1])):
    result.append({**base, 'operation': 'protocol', 'wire': {**wire, field: value}})
    result.append({**base, 'operation': 'identify', 'wire': {**wire, field: value}})
  result.append({**base, 'operation': 'identify', 'serial': 'different'})
  for size in (0, 1, 255, 256, 257, 513):
    for mcu in ('F4', 'H7'):
      result.append({**base, 'operation': 'dfu_program', 'data': [13] * size, 'mcu': mcu})
  for length in (0, 1, 12, 255, 256, 257):
    result.append({**base, 'operation': 'dfu_read', 'length': length})
  for parts in (None, [], [[7]], [[1, 2], [3]], [[]]):
    for predata in (None, [], [0, 0]):
      result.append({**base, 'operation': 'dfu_command', 'parts': parts, 'predata': predata})
  for ack in (0, 0x1f):
    for operation in ('dfu_probe', 'dfu_command', 'dfu_ack'):
      result.append({**base, 'operation': operation, 'wire': {**wire, 'dfu_ack': [ack]}})
  for chip in ([0, 1, 4, 0x63], [0, 1, 0, 1], [0, 2, 4, 0x83]):
    result.append({**base, 'operation': 'dfu_probe', 'wire': {**wire, 'dfu_reply': chip}})
  for operation in ('transfer', 'read', 'write', 'bulk_read', 'bulk_write'):
    result.append({**base, 'operation': operation, 'kernel': True})
  for case in list(result):
    calls = original(namespace, case)['calls']
    if len(calls) > 180:
      continue
    for index in range(len(calls)):
      for kind in ('io', 'protocol'):
        result.append({**case, 'fail_at': index, 'fail_kind': kind})
  return result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  namespace = source_classes()
  inputs = cases(namespace)
  text = ''.join(json.dumps(case) + '\n' for case in inputs)
  (args.output / 'inputs.jsonl').write_text(text)
  process = subprocess.run([*args.runner, str(args.binary.resolve())], input=text, text=True, capture_output=True, timeout=120)
  (args.output / 'native.jsonl').write_text(process.stdout)
  (args.output / 'native.stderr').write_text(process.stderr)
  process.check_returncode()
  observed = [json.loads(line) for line in process.stdout.splitlines()]
  assert len(observed) == len(inputs)
  calls = 0
  with (args.output / 'source.jsonl').open('w') as stream:
    for index, (case, actual) in enumerate(zip(inputs, observed, strict=True)):
      expected = original(namespace, case)
      stream.write(json.dumps(expected) + '\n')
      if expected != actual:
        (args.output / 'failure.json').write_text(json.dumps({'index': index, 'case': case, 'expected': expected, 'actual': actual}, indent=2))
        raise AssertionError(f'SPI mismatch at scenario {index}; see failure.json')
      calls += len(expected['calls'])
  report = {'result': 'PASS', 'scenarios': len(inputs), 'calls': calls, 'binary_sha256': sha256(args.binary.read_bytes()).hexdigest(),
            'source_sha256': {name: sha256((ROOT / name).read_bytes()).hexdigest() for name in
                              ('panda/python/spi.py', 'panda/python/__init__.py', 'panda/python/dfu.py', 'panda/python/constants.py')},
            'limits': 'Unchanged Python SPI and STM32 bootloader methods with owned I/O, clocks and kernel-ioctl boundary; no physical SPI.'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
