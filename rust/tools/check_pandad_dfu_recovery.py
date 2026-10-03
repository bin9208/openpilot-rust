import argparse
import ast
from contextlib import redirect_stdout
from hashlib import sha256
import io
import json
import os
from pathlib import Path
import runpy
import shlex
import struct
import subprocess
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]
SERIAL = '000800060004'


def original(case, firmware):
  calls, steps = [], []
  captured = io.StringIO()
  descriptor = '@Internal Flash /0x08000000/' + ('16*001Kg' if case['mcu'] == 'F4' else '08*128Kg')

  def transfer(kind, request, value, index, length, timeout, data=None, reply=None, default_code=None):
    calls.append(['control', kind, request, value, index, length, timeout, data])
    if len(calls) - 1 == case.get('fail_at'):
      steps.append({'ret': -9})
      raise OSError('owned USB failure')
    reply = [0] * length if reply is None and data is None else (reply or [])
    steps.append({'ret': default_code if default_code is not None else len(reply) if data is None else length, 'data': reply})
    return bytes(reply)

  class Handle:
    def getASCIIStringDescriptor(self, index):
      assert index == 3
      return SERIAL

    def getStringDescriptor(self, index, language):
      assert language == 0
      if index == 0:
        return None
      if index != case.get('descriptor_index', 4):
        transfer(0x80, 6, 0x300 | index, 0, 255, 1000, reply=[], default_code=-5)
        return None
      encoded = descriptor.encode('utf-16-le')
      transfer(0x80, 6, 0x300 | index, 0, 255, 1000, reply=[len(encoded) + 2, 3, *encoded])
      return descriptor

    def controlRead(self, kind, request, value, index, length, timeout=0):
      return transfer(kind | 0x80, request, value, index, length, timeout)

    def controlWrite(self, kind, request, value, index, data, timeout=0):
      return transfer(kind & 0x7F, request, value, index, len(data), timeout, data=list(data))

    def close(self):
      pass

  class Device:
    def getVendorID(self):
      return 0x0483

    def getProductID(self):
      return 0xDF11

    def open(self):
      return Handle()

  class Context:
    def open(self):
      pass

    def close(self):
      pass

    def getDeviceList(self, skip_on_error):
      assert skip_on_error
      return [Device()]

  namespace = {
    'usb1': SimpleNamespace(USBContext=Context),
    'BaseSTBootloaderHandle': object,
    'os': os,
    'struct': struct,
    'FW_PATH': str(firmware),
    'McuType': runpy.run_path(str(ROOT / 'panda/python/constants.py'))['McuType'],
  }
  for name, filename in [('STBootloaderUSBHandle', 'panda/python/usb.py'), ('PandaDFU', 'panda/python/dfu.py')]:
    tree = ast.parse((ROOT / filename).read_text())
    cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == name)
    exec(compile(ast.Module(body=[cls], type_ignores=[]), filename, 'exec'), namespace)
  ok = True
  with redirect_stdout(captured):
    try:
      with namespace['PandaDFU'](case.get('serial', SERIAL)) as dfu:
        dfu.recover()
    except Exception:
      ok = False
  return {'ok': ok, 'stdout': captured.getvalue(), 'calls': calls}, steps


def scenarios():
  rows = []
  for mcu, block in [('F4', 2048), ('H7', 1024)]:
    rows += [{'mcu': mcu, 'size': size} for size in [0, 1, block - 1, block, block + 1, 2300]]
    rows += [{'mcu': mcu, 'size': 2300, 'descriptor_index': index, 'serial': None} for index in [1, 19]]
    rows += [{'mcu': mcu, 'size': 2300, 'missing': True}]
    rows += [{'mcu': mcu, 'size': 2300, 'fail_at': index} for index in range(50)]
  return rows


def main():
  parser = argparse.ArgumentParser(description='Compare real native USB DFU discovery, files, recovery and stdout with unchanged Python.')
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--fixture', type=Path, required=True)
  parser.add_argument('--runner', default='')
  parser.add_argument('--preload', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  libraries = args.output / 'libraries'
  libraries.mkdir()
  (libraries / 'libusb-1.0.so.0').symlink_to(args.fixture.resolve())
  inputs = scenarios()
  records = []
  (args.output / 'inputs.json').write_text(json.dumps(inputs, indent=2) + '\n')
  for index, case in enumerate(inputs):
    directory = args.output / f'{index:03d}'
    directory.mkdir()
    (directory / 'owned-regular-spi').touch()
    name = 'bootstub.panda.bin' if case['mcu'] == 'F4' else 'bootstub.panda_h7.bin'
    if not case.get('missing'):
      (directory / name).write_bytes(bytes((i * 37 + 27) % 256 for i in range(case['size'])))
    expected, steps = original(case, directory)
    fixture = {'raw': True, 'devices': [{'vendor': 0x0483, 'product': 0xDF11, 'serial': list(SERIAL.encode())}], 'script': {'steps': steps}}
    trace = directory / 'usb.json'
    env = {
      **os.environ,
      'PANDA_FIRMWARE_USB_CASE': json.dumps(fixture),
      'PANDA_FIRMWARE_USB_TRACE': str(trace.resolve()),
      'PANDA_FIRMWARE_USB_LIBRARY': str(args.fixture.resolve()),
      'LD_LIBRARY_PATH': str(libraries.resolve()),
      'PANDA_DFU_FIRMWARE': str(directory.resolve()),
    }
    env.pop('PANDA_DFU_SERIAL', None)
    if args.preload:
      env['LD_PRELOAD'] = str(args.preload.resolve())
    if case.get('serial', SERIAL) is not None:
      env['PANDA_DFU_SERIAL'] = case.get('serial', SERIAL)
    result = subprocess.run([*shlex.split(args.runner), str(args.binary.resolve())], env=env, text=True, capture_output=True, timeout=15)
    (directory / 'stdout').write_text(result.stdout)
    (directory / 'stderr').write_text(result.stderr)
    (directory / 'source.json').write_text(json.dumps(expected, indent=2) + '\n')
    result.check_returncode()
    status = json.loads(next(line.removeprefix('DFU_RESULT ') for line in result.stderr.splitlines() if line.startswith('DFU_RESULT ')))
    raw = json.loads(trace.read_text())['calls']
    observed = {'ok': status['ok'], 'stdout': result.stdout, 'calls': [call for call in raw if isinstance(call, list) and call[0] == 'control']}
    if observed != expected:
      (args.output / 'failure.json').write_text(json.dumps({'index': index, 'case': case, 'source': expected, 'native': observed}, indent=2))
      raise AssertionError(f'DFU recovery mismatch in scenario {index}')
    assert raw.count('init') == raw.count('exit') == 1
    assert raw.count(['open', 0]) == raw.count(['close', 0]) == 2
    assert raw.count(['free_list', 1]) == 1
    records.append({'case': case, 'ok': status['ok'], 'transfers': len(observed['calls']), 'progress_lines': len(result.stdout.splitlines())})
  report = {
    'result': 'PASS',
    'scenarios': len(records),
    'transfers': sum(row['transfers'] for row in records),
    'progress_lines': sum(row['progress_lines'] for row in records),
    'records': records,
    'binary_sha256': sha256(args.binary.read_bytes()).hexdigest(),
    'fixture_sha256': sha256(args.fixture.read_bytes()).hexdigest(),
    'source_sha256': {
      name: sha256((ROOT / name).read_bytes()).hexdigest() for name in ('panda/python/dfu.py', 'panda/python/usb.py', 'panda/python/constants.py')
    },
    'limits': 'Owned native USB recovery on host/QEMU; no physical device, SPI DFU composition, or vehicle validation.',
  }
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({key: value for key, value in report.items() if key != 'records'}))


if __name__ == '__main__':
  main()
