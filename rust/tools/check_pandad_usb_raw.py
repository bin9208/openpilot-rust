import argparse
import ast
import ctypes
from hashlib import sha256
import importlib
import json
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace


def source_class(path):
  sys.path.insert(0, str(path.parent.parent))
  module = importlib.import_module('usb1')
  assert Path(module.__file__).resolve() == path.resolve()
  namespace = dict(vars(module))
  tree = ast.parse(path.read_text())
  cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'USBDeviceHandle')
  cls.bases = []
  methods = {'getStringDescriptor', 'getASCIIStringDescriptor', '_controlTransfer', 'controlRead', 'controlWrite', '_bulkTransfer', 'bulkWrite'}
  cls.body = [node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name in methods]
  exec(compile(ast.Module(body=[cls], type_ignores=[]), str(path), 'exec'), namespace)
  return namespace


def original(namespace, case):
  calls = []
  steps = iter(case['script']['steps'])
  current = 0

  def control(_handle, kind, request, value, index, data, length, timeout):
    calls.append(['control', kind, request, value, index, length, timeout,
                  None if kind & 0x80 else list(ctypes.string_at(data, length))])
    step = next(steps)
    payload = bytes(step.get('data', []))
    assert len(payload) <= length
    ctypes.memmove(data, payload, len(payload))
    return step.get('ret', 0)

  def bulk(_handle, endpoint, data, length, transferred, timeout):
    calls.append(['bulk', endpoint, length, timeout, None if endpoint & 0x80 else list(ctypes.string_at(data, length))])
    step = next(steps)
    payload = bytes(step.get('data', []))
    assert len(payload) <= length
    ctypes.memmove(data, payload, len(payload))
    ctypes.cast(transferred, ctypes.POINTER(ctypes.c_int))[0] = step.get('transferred', 0)
    return step.get('ret', 0)

  def ascii_string(_handle, index, data, length):
    calls.append(['serial', current, index, length])
    device = case['devices'][current]
    if 'serial_error' in device:
      return device['serial_error']
    payload = bytes(device['serial'])[:length]
    ctypes.memmove(data, payload, len(payload))
    return len(payload)

  namespace['libusb1'] = SimpleNamespace(libusb_control_transfer=control, libusb_bulk_transfer=bulk,
                                        libusb_get_string_descriptor_ascii=ascii_string,
                                        libusb_get_string_descriptor=lambda h, i, lang, data, length:
                                        control(h, 0x80, 6, 0x300 | i, lang, data, length, 1000))
  handle = namespace['USBDeviceHandle']()
  handle._USBDeviceHandle__handle = None
  owner = listed = opened = False
  values = []
  try:
    calls.append('init')
    if case.get('init', 0) != 0:
      raise OSError
    owner = True
    calls.append('list')
    if 'list_error' in case:
      raise OSError
    listed = True
    for current, device in enumerate(case['devices']):
      calls.append(['descriptor', current])
      if device.get('descriptor_error', 0) < 0:
        raise OSError
      calls.append(['open', current])
      if device.get('open', 0) < 0:
        raise OSError
      opened = True
      serial = handle.getASCIIStringDescriptor(device.get('serial_index', 3))
      if case.get('claim', False):
        calls.append(['auto_detach', 1])
        if case.get('auto_detach', 0) < 0:
          raise OSError
        calls.append(['claim', 0])
        if case.get('claim_code', 0) < 0:
          raise OSError
      results = []
      for op in case['operations']:
        kind, request, value, index, timeout = [op.get(key, 0) for key in ('kind', 'request', 'value', 'index', 'timeout')]
        if op['op'] == 'read':
          result = list(handle.controlRead(kind, request, value, index, op.get('length', 0), timeout))
        elif op['op'] == 'write':
          result = handle.controlWrite(kind, request, value, index, bytes(op.get('data', [])), timeout)
        elif op['op'] == 'bulk':
          result = handle.bulkWrite(request, bytes(op.get('data', [])), timeout)
        else:
          result = handle.getStringDescriptor(request, index)
        results.append(result)
      values.append({'serial': serial, 'vendor': device['vendor'], 'product': device['product'], 'bcd': device.get('bcd', 0), 'values': results})
      calls.append(['close', current])
      opened = False
    success = True
  except Exception:
    success = False
  finally:
    if opened:
      calls.append(['close', current])
    if listed:
      calls.append(['free_list', 1])
    if owner:
      calls.append('exit')
  result = {'ok': success, 'trace': {'calls': calls, 'logs': []}}
  if success:
    result['values'] = values
  return result


def cases():
  device = {'vendor': 0x3801, 'product': 0xddcc, 'serial': list(b'012345678901234567890123'), 'bcd': 0x0900}
  base = {'raw': True, 'devices': [device], 'operations': [], 'script': {'steps': []}, 'claim': False}
  result = [base, {**base, 'claim': True}, {**base, 'devices': []}]
  for property_name in ('descriptor_error', 'open', 'serial_error'):
    for error in (-1, -3, -4, -5, -7, -9, -99):
      result.append({**base, 'devices': [{**device, property_name: error}]})
  for error in (-1, -3, -4, -5, -7, -9, -99):
    result += [{**base, 'init': error}, {**base, 'list_error': error}, {**base, 'claim': True, 'auto_detach': error},
               {**base, 'claim': True, 'claim_code': error}]
  for data in ([], [0], [127], [128], list(range(255))):
    result.append({**base, 'devices': [{**device, 'serial': data}]})
  result.append({**base, 'devices': [{**device, 'serial_index': 0}]})
  for kind in (0, 0x21, 0x40, 0x80, 0xa1, 0xc0, 0xff):
    for operation in ('read', 'write', 'bulk'):
      for size in (0, 1, 7, 64, 255, 1024):
        data = [index % 256 for index in range(size)]
        op = {'op': operation, 'kind': kind, 'request': 0x82, 'value': 1, 'index': 2, 'timeout': 15000, 'length': size, 'data': data}
        for status in (0, -4, -7, -9):
          step = {'ret': status if status < 0 or operation == 'bulk' else size, 'transferred': size,
                  'data': data if operation == 'read' else []}
          result.append({**base, 'operations': [op], 'script': {'steps': [step]}})
  for text in ('', '@Internal Flash  /0x08000000/08*128Kg', '한글', '\U0001f642'):
    raw = list(text.encode('utf-16-le'))
    raw = [len(raw) + 2, 3, *raw]
    result.append({**base, 'operations': [{'op': 'string', 'request': 4}], 'script': {'steps': [{'ret': len(raw), 'data': raw}]}})
  for raw in ([], [2], [2, 2], [3, 3, 0], [4, 3, 0, 0xd8], [0, 3, 65, 0], [2, 3, 65, 0], [8, 3, 65, 0]):
    result.append({**base, 'operations': [{'op': 'string', 'request': 4}], 'script': {'steps': [{'ret': len(raw), 'data': raw}]}})
  for error in (-4, -5, -9):
    result.append({**base, 'operations': [{'op': 'string', 'request': 4}], 'script': {'steps': [{'ret': error}]}})
  result.append({**base, 'operations': [{'op': 'string', 'request': 0}]})
  return result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--fixture', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--native-preload', type=Path)
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  namespace = source_class(args.binding)
  inputs = cases()
  text = ''.join(json.dumps(case) + '\n' for case in inputs)
  (args.output / 'inputs.jsonl').write_text(text)
  command = [*args.runner, str(args.binary.resolve()), str(args.fixture.resolve())]
  if args.native_preload:
    command = ['env', f'LD_PRELOAD={args.native_preload.resolve()}', *command]
  process = subprocess.run(command, input=text, text=True, capture_output=True, timeout=120)
  (args.output / 'native.jsonl').write_text(process.stdout)
  (args.output / 'native.stderr').write_text(process.stderr)
  process.check_returncode()
  observed = [json.loads(line) for line in process.stdout.splitlines()]
  assert len(observed) == len(inputs)
  with (args.output / 'source.jsonl').open('w') as output:
    for index, (case, actual) in enumerate(zip(inputs, observed, strict=True)):
      expected = original(namespace, case)
      output.write(json.dumps(expected) + '\n')
      actual.pop('error', None)
      if actual != expected:
        (args.output / 'failure.json').write_text(json.dumps({'index': index, 'case': case, 'expected': expected, 'actual': actual}, indent=2))
        raise AssertionError(f'raw USB mismatch at scenario {index}; see failure.json')
  report = {'result': 'PASS', 'scenarios': len(inputs), 'binary_sha256': sha256(args.binary.read_bytes()).hexdigest(),
            'fixture_sha256': sha256(args.fixture.read_bytes()).hexdigest(), 'binding_sha256': sha256(args.binding.read_bytes()).hexdigest(),
            'limits': 'Unchanged python-libusb1 transfer/string methods with an owned libusb ABI; no physical USB.'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
