import argparse
import ast
import binascii
import copy
from functools import partial, wraps
from hashlib import sha256
import io
from itertools import accumulate
import json
import os
from pathlib import Path
import runpy
import struct
import subprocess
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]


def source_classes():
  namespace = {'struct': struct, 'wraps': wraps, 'accumulate': accumulate, 'binascii': binascii, 'BaseHandle': object,
               'usb1': SimpleNamespace(ENDPOINT_IN=0x80, ENDPOINT_OUT=0, TYPE_VENDOR=0x40, RECIPIENT_DEVICE=0),
               'McuType': runpy.run_path(str(ROOT / 'panda/python/constants.py'))['McuType']}
  filename = ROOT / 'panda/python/__init__.py'
  tree = ast.parse(filename.read_text())
  version = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'ensure_version')
  exec(compile(ast.Module(body=[version], type_ignores=[]), str(filename), 'exec'), namespace)
  namespace['ensure_health_packet_version'] = partial(namespace['ensure_version'], 'health', 'HEALTH_PACKET_VERSION', 'health_version')
  methods = {'__init__', 'close', 'connect', 'reset', 'reconnect', 'connected', 'flasher_present', 'flash_static', 'flash',
             'recover', 'wait_for_dfu', 'up_to_date', 'health', 'get_version', 'get_signature_from_firmware', 'get_signature',
             'get_type', 'get_packets_versions', 'get_mcu_type', 'is_internal', 'get_usb_serial', 'get_dfu_serial',
             'set_heartbeat_disabled', 'set_power_save', 'can_reset_communications', 'set_can_speed_kbps'}
  names = {'REQUEST_IN', 'REQUEST_OUT', 'F4_DEVICES', 'H7_DEVICES', 'INTERNAL_DEVICES', 'HEALTH_PACKET_VERSION', 'HEALTH_STRUCT'}
  cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'Panda')
  cls.body = [node for node in cls.body if
              (isinstance(node, ast.FunctionDef) and node.name in methods) or
              (isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and
               (target.id in names or target.id.startswith('HW_TYPE_')) for target in node.targets))]
  exec(compile(ast.Module(body=[cls], type_ignores=[]), str(filename), 'exec'), namespace)
  filename = ROOT / 'panda/python/dfu.py'
  tree = ast.parse(filename.read_text())
  cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'PandaDFU')
  cls.body = [node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name == 'st_serial_to_dfu_serial']
  exec(compile(ast.Module(body=[cls], type_ignores=[]), str(filename), 'exec'), namespace)
  return namespace


def original(namespace, case):
  calls, values = [], []
  clock = 0.0
  queues = copy.deepcopy({key: case[key] for key in ('usb', 'spi', 'dfu')})

  def record(*call):
    calls.append(list(call))
    if case.get('fail_at') == len(calls) - 1:
      raise OSError('scripted failure')

  def next_value(key):
    queue = queues[key]
    return copy.deepcopy(queue[0] if len(queue) == 1 else queue.pop(0))

  class Handle:
    def __init__(self, spec):
      self.spec = spec

    def request(self, kind, request, value, index, timeout, disconnect=False):
      return {'kind': kind, 'request': request, 'value': value, 'index': index, 'timeout_ms': timeout, 'expect_disconnect': disconnect}

    def controlRead(self, kind, request, value, index, length, timeout=15000):
      record('read', self.request(kind, request, value, index, timeout), length)
      reads = self.spec['reads'].get(str(request), [])
      if reads:
        return bytes(reads.pop(0))
      defaults = {0xc1: self.spec['kind'], 0xdd: self.spec['versions'], 0xd6: self.spec['version'],
                  0xd3: self.spec['signature'][:64], 0xd4: self.spec['signature'][64:], 0xd2: self.spec['health'],
                  0xb0: [0, 0, 0, 0, 0xde, 0xad, 0xd0, 0x0d, 0, 0, 0, 0]}
      return bytes(defaults.get(request, [0] * length))

    def controlWrite(self, kind, request, value, index, data, timeout=15000, expect_disconnect=False):
      record('write', self.request(kind, request, value, index, timeout, expect_disconnect), list(data))

    def bulkWrite(self, endpoint, data, timeout=15000):
      record('bulk', endpoint, list(data), timeout)

    def close(self):
      record('close')

  class Spi(Handle):
    pass

  def connect(key, serial, claim=True, no_error=False):
    if key == 'usb':
      record('usb_connect', serial, claim, no_error)
    else:
      record('spi_connect', serial)
    spec = next_value(key)
    if spec is None:
      return None, None, None, False, None
    handle = (Spi if spec['spi'] else Handle)(spec)
    return None, handle, spec['serial'], spec['bootstub'], bytes(spec['bcd']) if spec['bcd'] is not None else None

  class Logger:
    def __getattr__(self, level):
      return lambda text, *args: record('log', level, text % args if args else text)

  class File(io.BytesIO):
    def __init__(self, path):
      super().__init__(bytes(case['file']))
      self.path = path
      self.tail = False

    def seek(self, offset, whence=0):
      assert offset == -128 and whence == 2
      record('file', self.path, 128)
      self.tail = True
      if len(case['file']) < 128:
        raise OSError('negative file position')
      return super().seek(offset, whence)

    def read(self, size=-1):
      if not self.tail:
        record('file', self.path, None)
      return super().read(size)

  def isfile(path):
    record('isfile', path)
    return case['file_exists']

  def sleep(seconds):
    nonlocal clock
    record('sleep', seconds)
    clock += seconds

  def monotonic():
    record('clock')
    return clock

  def dfu_list():
    record('dfu_list')
    return next_value('dfu')

  dfu_serial_function = namespace['PandaDFU'].st_serial_to_dfu_serial

  class Dfu:
    st_serial_to_dfu_serial = staticmethod(dfu_serial_function)
    list = staticmethod(dfu_list)

    def __init__(self, serial):
      record('dfu_connect', serial)

    def recover(self):
      record('dfu_recover')

  namespace.update({'logger': Logger(), 'PANDA_BUS_CNT': 3, 'PandaSpiHandle': Spi, 'PandaDFU': Dfu,
                    'FW_PATH': '/firmware', 'os': SimpleNamespace(path=SimpleNamespace(join=os.path.join, isfile=isfile)),
                    'time': SimpleNamespace(sleep=sleep, monotonic=monotonic), 'open': lambda path, mode: File(path)})
  Panda = namespace['Panda']
  Panda.usb_connect = classmethod(lambda cls, serial, claim=True, no_error=False: connect('usb', serial, claim, no_error))
  Panda.spi_connect = classmethod(lambda cls, serial: connect('spi', serial))
  health_names = {'tx_buffer_overflow': 'tx_overflow', 'rx_buffer_overflow': 'rx_overflow', 'car_harness_status': 'harness_status',
                  'safety_mode': 'safety_model', 'power_save_enabled': 'power_save', 'spi_checksum_error_count': 'spi_checksum_errors',
                  'sbu1_voltage_mV': 'sbu1_mv', 'sbu2_voltage_mV': 'sbu2_mv'}
  try:
    panda = Panda('010002000300040005000600', cli=False)
    for op in case['operations']:
      name = op['name']
      value = None
      if name == 'type':
        value = list(panda.get_type())
      elif name == 'mcu':
        value = panda.get_mcu_type().name
      elif name == 'internal':
        value = panda.is_internal()
      elif name == 'signature':
        value = list(panda.get_signature())
      elif name == 'version':
        value = panda.get_version()
      elif name == 'health':
        value = {health_names.get(key, key): item for key, item in panda.health().items()}
      elif name == 'up_to_date':
        value = panda.up_to_date(fn=op.get('path'))
      elif name == 'flash':
        panda.flash(fn=op.get('path'), code=bytes(op['code']) if op.get('code') is not None else None, reconnect=op.get('reconnect', True))
      elif name == 'reset':
        panda.reset(enter_bootstub=op.get('bootstub', False), enter_bootloader=op.get('bootloader', False), reconnect=op.get('reconnect', True))
      elif name == 'reconnect':
        panda.reconnect()
      elif name == 'recover':
        value = panda.recover(timeout=op.get('timeout'), reset=op.get('reset', True))
      elif name == 'wait':
        value = panda.wait_for_dfu(op.get('serial'), timeout=op.get('timeout'))
      elif name == 'close':
        panda.close()
      else:
        raise ValueError(name)
      values.append({'value': value, 'connected': panda.connected, 'bootstub': panda.bootstub})
  except Exception:
    return {'ok': False, 'values': values, 'calls': calls}
  return {'ok': True, 'values': values, 'calls': calls}


def cases(namespace):
  signature = list(range(128))
  device = {'serial': '010002000300040005000600', 'bootstub': False, 'bcd': None, 'spi': False,
            'kind': [9], 'versions': [16, 4, 5], 'signature': signature, 'version': list(b'test-version'),
            'health': [0] * 58, 'reads': {}}
  default = {'usb': [device], 'spi': [None], 'operations': [], 'file': signature, 'file_exists': True, 'dfu': [[]], 'fail_at': None}
  operations = [{'name': name} for name in ('type', 'mcu', 'internal', 'signature', 'version', 'health', 'up_to_date', 'close', 'close')]
  result = [{**default, 'operations': operations}]
  for kind in range(12):
    for spi in (False, True):
      spec = {**device, 'kind': [kind], 'spi': spi}
      result.append({**default, 'usb': [None if spi else spec], 'spi': [spec if spi else None], 'operations': operations})
  magic = [0xff, 0, 0xc1, 0x3e, 0xde, 0xad, 0xd0, 0x0d]
  for bcd in (None, [0], [4], [6], [9]):
    for kind in (magic, [9], [], [1, 2]):
      result.append({**default, 'usb': [{**device, 'bootstub': True, 'bcd': bcd, 'kind': kind}], 'operations': operations})
  for version in ([], [16], [16, 4], [16, 4, 5, 6], [0, 4, 5], [15, 4, 5]):
    result.append({**default, 'usb': [{**device, 'versions': version}], 'operations': [{'name': 'health'}]})
  for size in (0, 1, 57, 58, 59):
    result.append({**default, 'usb': [{**device, 'health': [0] * size}], 'operations': [{'name': 'health'}]})
  for version in ([0xff], list('한글'.encode()), [], [0, 10, 97]):
    result.append({**default, 'usb': [{**device, 'version': version}], 'operations': [{'name': 'version'}]})
  boot = {**device, 'bootstub': True, 'signature': [0] * 128}
  stale = {**device, 'signature': [0] * 128}
  for start in (stale, boot):
    for reconnect in (False, True):
      for code in (None, [], [1], list(range(33))):
        result.append({**default, 'usb': [start, boot, device] if not start['bootstub'] else [start, device],
                       'operations': [{'name': 'flash', 'code': code, 'reconnect': reconnect}]})
  for size in (0, 127, 128, 129):
    result.append({**default, 'file': [0] * size, 'operations': [{'name': 'up_to_date'}]})
  result += [{**default, 'usb': [stale, boot, device], 'file_exists': False, 'operations': [{'name': 'flash'}]},
             {**default, 'usb': [stale], 'operations': [{'name': 'flash'}]},
             {**default, 'usb': [boot, device], 'operations': [{'name': 'flash', 'path': '/custom.bin'}]},
             {**default, 'usb': [device], 'operations': [{'name': 'flash'}]},
             {**default, 'usb': [None]}, {**default, 'usb': [device, {**device, 'kind': [0]}], 'operations': [{'name': 'reconnect'}]}]
  for spi in (False, True):
    for bootstub in (False, True):
      for bootloader in (False, True):
        for reconnect in (False, True):
          spec = {**device, 'spi': spi}
          result.append({**default, 'usb': [None if spi else spec], 'spi': [spec if spi else None],
                         'operations': [{'name': 'reset', 'bootstub': bootstub, 'bootloader': bootloader, 'reconnect': reconnect}]})
  serial = '000800060004'
  for requested in (None, serial):
    for timeout in (None, 0, 0.1, 0.2, 1):
      result.append({**default, 'dfu': [[], ['other'], [serial]], 'operations': [{'name': 'wait', 'serial': requested, 'timeout': timeout}]})
  for reset in (False, True):
    result.append({**default, 'usb': [device, boot, boot, device] if reset else [device, boot, device],
                   'dfu': [[], [serial]], 'operations': [{'name': 'recover', 'reset': reset, 'timeout': 1}]})
  result += [{**default, 'dfu': [[None]], 'operations': [{'name': 'wait', 'timeout': 0}]},
             {**default, 'dfu': [[None], [serial]], 'operations': [{'name': 'wait', 'serial': serial, 'timeout': 1}]}]
  baselines = list(result)
  for case in baselines:
    count = len(original(namespace, case)['calls'])
    if count >= 250:
      continue
    for index in range(count):
      result.append({**case, 'fail_at': index})
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
  actual = [json.loads(line) for line in process.stdout.splitlines()]
  assert len(actual) == len(inputs)
  calls = 0
  with (args.output / 'source.jsonl').open('w') as output:
    for index, (case, observed) in enumerate(zip(inputs, actual, strict=True)):
      expected = original(namespace, case)
      output.write(json.dumps(expected) + '\n')
      if expected != observed:
        (args.output / 'failure.json').write_text(json.dumps({'index': index, 'case': case, 'expected': expected, 'actual': observed}, indent=2))
        raise AssertionError(f'client mismatch at scenario {index}; see failure.json')
      calls += len(expected['calls'])
  sources = [ROOT / name for name in ('panda/python/__init__.py', 'panda/python/dfu.py', 'panda/python/constants.py')]
  report = {'result': 'PASS', 'scenarios': len(inputs), 'calls': calls, 'binary_sha256': sha256(args.binary.read_bytes()).hexdigest(),
            'source_sha256': {str(path): sha256(path.read_bytes()).hexdigest() for path in sources},
            'limits': 'Unchanged Panda runtime-client methods with recorded USB/SPI connector, transport, filesystem, clock and DFU boundaries; no hardware.'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
