import argparse
import ast
from hashlib import sha256
import json
import os
from pathlib import Path
import runpy
import subprocess
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]


def original(case):
  logs = []

  class Logger:
    def debug(self, text, *args):
      logs.append(['debug', text % args])

    def warning(self, text, *args):
      logs.append(['warning', text])

    def exception(self, text):
      logs.append(['exception', text])

  def serial(spec, index):
    if index == 0 or spec.get('serial_error') == -5:
      return None
    if spec.get('serial_error', 0) < 0:
      raise OSError('serial error')
    return bytes(spec['serial'][:255]).decode('ascii')

  class Handle:
    def __init__(self, spec):
      self.spec = spec

    def getASCIIStringDescriptor(self, index):
      return serial(self.spec, index)

    def getStringDescriptor(self, index, language):
      assert language == 0
      value = case.get('strings', {}).get(str(index))
      if isinstance(value, int):
        raise OSError('string descriptor error')
      return value

    def setAutoDetachKernelDriver(self, enabled):
      assert enabled
      if case.get('auto_detach', 0) < 0:
        raise OSError('detach error')

    def claimInterface(self, interface):
      assert interface == 0
      if case.get('claim_code', 0) < 0:
        raise OSError('claim error')

    def close(self):
      pass

  class Device:
    def __init__(self, spec):
      self.spec = spec

    def getVendorID(self):
      return self.spec['vendor']

    def getProductID(self):
      return self.spec['product']

    def getSerialNumber(self):
      self.open()
      return serial(self.spec, self.spec.get('serial_index', 3))

    def getbcdDevice(self):
      return self.spec.get('bcd', 0)

    def open(self):
      if self.spec.get('open', 0) < 0:
        raise OSError('open error')
      return Handle(self.spec)

  class Context:
    def open(self):
      if case.get('init', 0) < 0:
        raise OSError('init error')

    def close(self):
      pass

    def getDeviceList(self, skip_on_error):
      assert skip_on_error
      if case.get('list_error', 0) < 0:
        raise OSError('list error')
      return [Device(spec) for spec in case['devices'] if spec.get('descriptor_error', 0) >= 0]

    def __enter__(self):
      self.open()
      return self

    def __exit__(self, *args):
      self.close()

  namespace = {'usb1': SimpleNamespace(USBContext=Context), 'sys': SimpleNamespace(platform='linux'),
               'logger': Logger(), 'BaseHandle': object, 'BaseSTBootloaderHandle': object, 'TIMEOUT': 15000,
               'McuType': runpy.run_path(str(ROOT / 'panda/python/constants.py'))['McuType']}
  definitions = [('panda/python/usb.py', 'PandaUsbHandle', None),
                 ('panda/python/usb.py', 'STBootloaderUSBHandle', {'__init__', 'get_mcu_type', 'close'}),
                 ('panda/python/__init__.py', 'Panda', {'usb_connect', 'usb_list'}),
                 ('panda/python/dfu.py', 'PandaDFU', {'usb_connect', 'usb_list'})]
  for filename, name, methods in definitions:
    tree = ast.parse((ROOT / filename).read_text())
    cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == name)
    if methods is not None:
      cls.body = [node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name in methods]
    exec(compile(ast.Module(body=[cls], type_ignores=[]), filename, 'exec'), namespace)
  namespace['Panda'].USB_VIDS = (0xbbaa, 0x3801)
  namespace['Panda'].USB_PIDS = (0xddee, 0xddcc)
  try:
    mode = case['mode']
    if mode == 'list':
      value = namespace['Panda'].usb_list()
    elif mode == 'dfu_list':
      value = namespace['PandaDFU'].usb_list()
    elif mode == 'dfu_connect':
      _, handle = namespace['PandaDFU'].usb_connect(case.get('serial'))
      value = None if handle is None else handle.get_mcu_type().name
      if handle is not None:
        handle.close()
    else:
      _, handle, found_serial, bootstub, bcd = namespace['Panda'].usb_connect(
        case['serial'], claim=case.get('claim', True), no_error=case.get('no_error', False))
      value = None if handle is None else {'serial': found_serial, 'bootstub': bootstub, 'bcd': list(bcd) if bcd is not None else None, 'spi': False}
      if handle is not None:
        handle.close()
    return {'ok': True, 'value': value, 'logs': logs}
  except Exception:
    return {'ok': False, 'logs': logs}


def cases():
  serial = '012345678901234567890123'
  device = {'vendor': 0x3801, 'product': 0xddcc, 'serial': list(serial.encode()), 'bcd': 0x0900}
  base = {'raw': True, 'devices': [device], 'serial': serial, 'script': {'steps': []}, 'mode': 'connect'}
  result = []
  for mode in ('connect', 'list', 'dfu_connect', 'dfu_list'):
    row = {**base, 'mode': mode}
    if mode.startswith('dfu'):
      row['devices'] = [{**device, 'vendor': 0x0483, 'product': 0xdf11}]
      row['strings'] = {'4': '@Internal Flash /0x08000000/08*128Kg'}
    result += [row, {**row, 'devices': []}, {**row, 'serial': 'different'},
               {**row, 'devices': [{**row['devices'][0], 'serial_error': -5}]}]
    for field in ('init', 'list_error', 'auto_detach', 'claim_code'):
      for code in (-3, -4, -9):
        result.append({**row, field: code})
    for field in ('descriptor_error', 'open', 'serial_error'):
      for code in (-3, -4, -5, -9):
        result.append({**row, 'devices': [{**row['devices'][0], field: code}]})
  for vendor in (0, 0xbbaa, 0x3801, 0x0483):
    for product in (0, 0xddee, 0xddcc, 0xdf11):
      for mode in ('connect', 'list'):
        result.append({**base, 'mode': mode, 'devices': [{**device, 'vendor': vendor, 'product': product}]})
  for bcd in (0, 0x2300, 0x0600, 0x09ff, 0xffff):
    for claim in (False, True):
      result.append({**base, 'devices': [{**device, 'bcd': bcd}], 'claim': claim})
  for data in ([], list(b'short'), list(b'x' * 25), [0xff]):
    for mode in ('connect', 'list'):
      result.append({**base, 'mode': mode, 'devices': [{**device, 'serial': data}]})
  result += [{**base, 'devices': [{**device, 'serial_error': -9}, device], 'no_error': quiet} for quiet in (False, True)]
  dfu = {**base, 'mode': 'dfu_connect', 'serial': None, 'devices': [{**device, 'vendor': 0x0483, 'product': 0xdf11}]}
  for strings in ({}, {'1': 'unrelated', '19': '@Internal Flash /0/16*001Kg'},
                  {'1': '@Internal Flash /0/04*016Kg,01*064Kg,011*128Kg'}, {'1': '@Internal Flash /0/07*128Kg'},
                  {'1': '@Internal Flash /0/bad'}, {'1': '@Internal Flash /0/  +08 *128Kg'}, {'1': -9}):
    result.append({**dfu, 'strings': strings})
  for row in result:
    row['script'] = {'steps': []}
    if row['mode'] == 'dfu_connect':
      for index in range(1, 20):
        value = row.get('strings', {}).get(str(index))
        if value is None:
          step = {'ret': -5}
        elif isinstance(value, int):
          step = {'ret': value}
        else:
          data = list(value.encode('utf-16-le'))
          data = [len(data) + 2, 3, *data]
          step = {'ret': len(data), 'data': data}
        row['script']['steps'].append(step)
        if isinstance(value, int) or (isinstance(value, str) and value.startswith('@Internal Flash')):
          break
  return result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--fixture', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--native-preload', type=Path)
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  link = args.output / 'libusb-1.0.so.0'
  link.symlink_to(args.fixture.resolve())
  inputs = cases()
  (args.output / 'inputs.jsonl').write_text(''.join(json.dumps(case) + '\n' for case in inputs))
  with (args.output / 'native.jsonl').open('w') as native, (args.output / 'source.jsonl').open('w') as source:
    for index, case in enumerate(inputs):
      expected = original(case)
      trace = args.output / f'{index:04d}-usb.json'
      env = {**os.environ, 'PANDA_FIRMWARE_USB_CASE': json.dumps(case), 'PANDA_FIRMWARE_USB_TRACE': str(trace.resolve()),
             'PANDA_FIRMWARE_USB_LIBRARY': str(args.fixture.resolve()), 'LD_LIBRARY_PATH': str(args.output.resolve())}
      if args.native_preload:
        env['LD_PRELOAD'] = str(args.native_preload.resolve())
      result = subprocess.run([*args.runner, str(args.binary.resolve())], env=env, text=True, capture_output=True, timeout=10)
      (args.output / f'{index:04d}.stderr').write_text(result.stderr)
      result.check_returncode()
      assert trace.exists(), 'owned libusb fixture did not execute'
      actual = json.loads(result.stdout)
      actual.pop('error', None)
      native.write(json.dumps(actual) + '\n')
      source.write(json.dumps(expected) + '\n')
      if actual != expected:
        (args.output / 'failure.json').write_text(json.dumps({'index': index, 'case': case, 'expected': expected, 'actual': actual}, indent=2))
        raise AssertionError(f'USB policy mismatch at scenario {index}; see failure.json')
  report = {'result': 'PASS', 'scenarios': len(inputs), 'binary_sha256': sha256(args.binary.read_bytes()).hexdigest(),
            'fixture_sha256': sha256(args.fixture.read_bytes()).hexdigest(),
            'source_sha256': {name: sha256((ROOT / name).read_bytes()).hexdigest() for name in
                              ('panda/python/__init__.py', 'panda/python/dfu.py', 'panda/python/usb.py')},
            'limits': 'Unchanged USB connector and MCU detection bodies; native owned libusb ABI. ' +
                      'Outcome and declared log payload parity; no physical USB or traceback equivalence.'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
