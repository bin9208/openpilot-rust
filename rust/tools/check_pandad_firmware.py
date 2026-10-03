import argparse
import ast
import binascii
from contextlib import redirect_stdout
from hashlib import sha256
from itertools import accumulate
import io
import json
from pathlib import Path
import random
import runpy
import struct
import subprocess
from types import SimpleNamespace


ROOT = Path(__file__).resolve().parents[2]


def source_classes():
  constants = runpy.run_path(str(ROOT / 'panda/python/constants.py'))
  namespace = {'McuType': constants['McuType'], 'struct': struct, 'binascii': binascii,
               'BaseSTBootloaderHandle': object, 'BaseHandle': object, 'accumulate': accumulate,
               'logger': SimpleNamespace(info=lambda *args: None)}
  definitions = [('panda/python/__init__.py', 'Panda', {'flasher_present', 'flash_static'}),
                 ('panda/python/dfu.py', 'PandaDFU', {'st_serial_to_dfu_serial', 'program_bootstub', 'reset'}),
                 ('panda/python/usb.py', 'STBootloaderUSBHandle', None)]
  for filename, name, names in definitions:
    tree = ast.parse((ROOT / filename).read_text())
    selected = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == name)
    if names is not None:
      selected.body = [node for node in selected.body if isinstance(node, ast.FunctionDef) and node.name in names]
    module = ast.Module(body=[selected], type_ignores=[])
    exec(compile(module, filename, 'exec'), namespace)
  namespace['Panda'].REQUEST_IN = 0xc0
  return namespace


class Handle:
  def __init__(self, case):
    self.case = case
    self.calls = []
    self.reads = iter(case.get('reads', []))

  def record(self, value):
    self.calls.append(value)
    if self.case.get('fail_at') == len(self.calls) - 1:
      raise OSError('scripted transport failure')

  def request(self, kind, request, value, index, timeout, disconnect=False):
    default_timeout = 15000 if self.case['operation'] == 'flash' else 0
    return {'kind': kind, 'request': request, 'value': value, 'index': index,
            'timeout_ms': default_timeout if timeout is None else timeout, 'expect_disconnect': disconnect}

  def controlRead(self, kind, request, value, index, length, timeout=None):
    self.record({'op': 'read', 'request': self.request(kind, request, value, index, timeout), 'length': length})
    default = [0, 0, 0, 0, 0xde, 0xad, 0xd0, 0x0d, 0, 0, 0, 0] if kind == 0xc0 and request == 0xb0 else [0] * length
    return bytes(next(self.reads, default))

  def controlWrite(self, kind, request, value, index, data, timeout=None, expect_disconnect=False):
    self.record({'op': 'write', 'request': self.request(kind, request, value, index, timeout, expect_disconnect), 'data': list(data)})

  def bulkWrite(self, endpoint, data, timeout=15000):
    self.record({'op': 'bulk', 'endpoint': endpoint, 'data': list(data), 'timeout_ms': timeout})


def original(namespace, case):
  handle = Handle(case)
  mcu = namespace['McuType'][case['mcu']]
  code = bytes((index * 37 + case.get('seed', 0)) % 256 for index in range(case.get('size', 0)))
  progress = []

  class Capture(io.StringIO):
    def write(self, text):
      if text.startswith('programming '):
        progress.append({'before_call': len(handle.calls), 'text': text})
        if case.get('progress_fail_at') == len(progress) - 1:
          raise OSError('scripted stdout failure')
      return super().write(text)

  captured = Capture()
  try:
    with redirect_stdout(captured):
      operation = case['operation']
      result = None
      if operation == 'serial':
        result = namespace['PandaDFU'].st_serial_to_dfu_serial(case['serial'], mcu)
      elif operation == 'flash':
        namespace['Panda'].flash_static(handle, code, mcu)
      else:
        dfu = namespace['STBootloaderUSBHandle'].__new__(namespace['STBootloaderUSBHandle'])
        dfu._mcu_type = mcu
        dfu._libusb_handle = handle
        if operation == 'clear':
          dfu.clear_status()
        elif operation == 'erase':
          dfu.erase_sector(case['sector'])
        elif operation == 'program':
          dfu.program(0x08000000, code)
        elif operation == 'jump':
          dfu.jump(0x08000000)
        elif operation == 'recover':
          wrapper = namespace['PandaDFU'].__new__(namespace['PandaDFU'])
          wrapper._mcu_type = mcu
          wrapper._handle = dfu
          wrapper.program_bootstub(code)
          wrapper.reset()
        else:
          raise ValueError(operation)
    return {'ok': True, 'value': result, 'calls': handle.calls, 'progress': progress}
  except (AssertionError, ValueError, ZeroDivisionError, IndexError, OSError) as error:
    return {'ok': False, 'error': f'{type(error).__name__}: {error}', 'calls': handle.calls, 'progress': progress}


def cases(namespace):
  result = []
  for mcu in ('F4', 'H7'):
    config = namespace['McuType'][mcu].config
    sizes = {0, 1, 15, 16, 17}
    for boundary in accumulate(config.sector_sizes[1:]):
      sizes.update((boundary - 1, boundary, boundary + 1))
    result += [{'operation': 'flash', 'mcu': mcu, 'size': size, 'seed': 19} for size in sorted(sizes)]
    result += [{'operation': 'flash', 'mcu': mcu, 'size': 33, 'fail_at': index} for index in range(9)]
    result += [{'operation': 'flash', 'mcu': mcu, 'size': 33, 'reads': [data]} for data in ([], [0] * 12, [0] * 7)]
    for size in (0, 1, config.block_size - 1, config.block_size, config.block_size + 1, config.block_size * 2 + 1):
      result.append({'operation': 'program', 'mcu': mcu, 'size': size, 'seed': 27})
      result.append({'operation': 'recover', 'mcu': mcu, 'size': size, 'seed': 27})
    for index in range(len(config.sector_sizes) + 2):
      result.append({'operation': 'erase', 'mcu': mcu, 'sector': index})
    for state in range(16):
      result.append({'operation': 'clear', 'mcu': mcu, 'reads': [[0, 0, 0, 0, state, 0]]})
    for operation in ('clear', 'program', 'jump', 'recover'):
      for failure in range(12):
        result.append({'operation': operation, 'mcu': mcu, 'size': 2300, 'fail_at': failure})
      for data in ([], [0], [0, 0], [0, 1, 0, 0, 0, 0]):
        result.append({'operation': operation, 'mcu': mcu, 'size': 31, 'reads': [data]})
    for operation in ('program', 'recover'):
      for failure in range(4):
        result.append({'operation': operation, 'mcu': mcu, 'size': 2300, 'progress_fail_at': failure})
    generator = random.Random(175)
    serials = ['', 'none', 'g0', '1 2', '00' * 11, '00' * 13, '01 00 02 00 03 00 04 00 05 00 06 00', 'ff' * 12]
    serials += [generator.randbytes(12).hex() for _ in range(1500)]
    result += [{'operation': 'serial', 'mcu': mcu, 'serial': value} for value in serials]
  return result


def main():
  parser = argparse.ArgumentParser(description='Compare native Panda flash/USB DFU operations with unchanged original Python function bodies.')
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
  progress_messages = 0
  with (args.output / 'source.jsonl').open('w') as output:
    for index, (case, observed) in enumerate(zip(inputs, actual, strict=True)):
      expected = original(namespace, case)
      output.write(json.dumps(expected) + '\n')
      expected.pop('error', None)
      observed.pop('error', None)
      assert expected == observed, (index, case, expected, observed)
      calls += len(expected['calls'])
      progress_messages += len(expected['progress'])
  sources = [ROOT / name for name in ('panda/python/__init__.py', 'panda/python/dfu.py', 'panda/python/usb.py', 'panda/python/constants.py')]
  report = {'result': 'PASS', 'scenarios': len(inputs), 'calls': calls, 'progress_messages': progress_messages,
            'binary_sha256': sha256(args.binary.read_bytes()).hexdigest(),
            'source_sha256': {str(path): sha256(path.read_bytes()).hexdigest() for path in sources},
            'limits': 'Unchanged source function bodies with recorded transport; no hardware, live flashing, or complete firmware supervisor.'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
