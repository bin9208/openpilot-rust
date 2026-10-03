import argparse
import ast
import copy
from collections.abc import Callable
from hashlib import sha256
import json
import os
from pathlib import Path
import subprocess
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]


class Halt(BaseException):
  pass


class NoDevice(Exception):
  pass


class Pipe(Exception):
  pass


class Protocol(Exception):
  pass


def original(case):
  calls = []
  current = copy.deepcopy(case['rounds'][0])
  connecting = None

  def record(*call):
    calls.append(list(call))
    if case.get('fail_at') == len(calls) - 1:
      raise {'no_device': NoDevice, 'pipe': Pipe, 'protocol': Protocol}.get(case['error_kind'], OSError)('scripted failure')

  class Logger:
    def __getattr__(self, level):
      def log(message, **values):
        nonlocal current
        if level == 'event' and message == 'pandad.flash_and_connect':
          index = values['count'] - 1
          if index >= len(case['rounds']):
            raise Halt
          current = copy.deepcopy(case['rounds'][index])
        record('log', level, message, values)
      return log

  class Panda:
    def __init__(self, serial):
      nonlocal connecting
      record('connect', serial)
      self.device = copy.deepcopy(next(d for d in current['devices'] if d['serial'] == serial))
      connecting = self.device

    @property
    def bootstub(self):
      return self.device['bootstub']

    def call(self, method):
      record(method, self.device['serial'])

    def is_internal(self):
      self.call('is_internal')
      return self.device['internal']

    def get_type(self):
      self.call('get_type')
      return bytes(self.device['kind'])

    def get_usb_serial(self):
      self.call('get_usb_serial')
      return self.device['serial']

    def get_version(self):
      self.call('get_version')
      return self.device['version']

    def get_signature(self):
      self.call('get_signature')
      return bytes(self.device['signature'])

    def get_mcu_type(self):
      self.call('get_mcu_type')
      return SimpleNamespace(config=SimpleNamespace(app_fn='app.bin'))

    def flash(self):
      self.call('flash')
      self.device['bootstub'] = self.device['flash_bootstub']
      self.device['signature'] = self.device['final_signature']

    def recover(self, reset):
      record('recover', self.device['serial'], reset)
      self.device['bootstub'] = self.device['recover_bootstub']
      self.device['signature'] = self.device['final_signature']

    def health(self):
      self.call('health')
      return self.device['health']

    def reset(self, reconnect):
      record('reset', self.device['serial'], reconnect)

    def close(self):
      self.call('close')

    @staticmethod
    def get_signature_from_firmware(path):
      record('firmware_signature', path)
      return bytes(connecting['expected'])

    @staticmethod
    def list():
      record('list')
      return [d['serial'] for d in current['devices']]

  class Dfu:
    def __init__(self, serial):
      record('dfu_connect', serial)
      self.serial = serial

    def recover(self):
      record('dfu_recover', self.serial)

    @staticmethod
    def list():
      record('dfu_list')
      return current['dfu']

  class Params:
    def remove(self, key):
      record('remove', key)

    def put(self, key, value):
      record('put', key, list(value))

    def put_bool(self, key, value):
      record('put_bool', key, value)

  class Environment:
    def __setitem__(self, key, value):
      record('env', key, value)

  class Process:
    def __init__(self, argv, cwd):
      assert argv[0] == './pandad' and cwd == '/source/openpilot/selfdrive/pandad'
      record('spawn', argv[1:])

    def wait(self):
      record('wait')

  def has_internal():
    record('has_internal')
    return current['has_internal']

  namespace = {
    'Panda': Panda, 'PandaDFU': Dfu, 'PandaT': Panda, 'Callable': Callable, 'PandaProtocolMismatch': Protocol,
    'HARDWARE': SimpleNamespace(reset_internal_panda=lambda: record('reset_internal'),
                                recover_internal_panda=lambda: record('recover_internal'), has_internal_panda=has_internal),
    'usb1': SimpleNamespace(USBErrorNoDevice=NoDevice, USBErrorPipe=Pipe), 'Params': Params,
    'cloudlog': Logger(), 'os': SimpleNamespace(path=os.path, environ=Environment()),
    'time': SimpleNamespace(sleep=lambda seconds: record('sleep', seconds)),
    'signal': SimpleNamespace(SIGINT=2, signal=lambda *args: None),
    'subprocess': SimpleNamespace(Popen=Process), 'BASEDIR': '/source', 'FW_PATH': '/firmware',
  }
  for path in ('panda_helpers.py', 'pandad.py'):
    filename = ROOT / 'openpilot/selfdrive/pandad' / path
    tree = ast.parse(filename.read_text())
    functions = [node for node in tree.body if isinstance(node, ast.FunctionDef)]
    exec(compile(ast.Module(body=functions, type_ignores=[]), str(filename), 'exec'), namespace)
  try:
    operation = case['operation']
    if operation == 'main':
      namespace['main']()
    elif operation == 'all':
      namespace['flash_all_pandas']([d['serial'] for d in current['devices']])
    else:
      namespace['flash_panda'](current['devices'][0]['serial'])
  except Halt:
    pass
  except Exception:
    return {'ok': False, 'calls': calls}
  return {'ok': True, 'calls': calls}


def cases():
  fields = '''uptime voltage current safety_tx_blocked safety_rx_invalid tx_overflow rx_overflow faults ignition_line
              ignition_can controls_allowed harness_status safety_model safety_param fault_status power_save heartbeat_lost
              alternative_experience interrupt_load fan_power safety_rx_checks_invalid spi_checksum_errors fan_stall_count
              sbu1_mv sbu2_mv som_reset_triggered'''.split()
  health = dict.fromkeys(fields, 0)
  health.update(heartbeat_lost=1, som_reset_triggered=1, interrupt_load=0.125, voltage=12345)
  device = {'serial': 'internal', 'kind': [9], 'internal': True, 'bootstub': False, 'flash_bootstub': False,
            'recover_bootstub': False, 'signature': [1, 2, 3], 'expected': [1, 2, 3], 'final_signature': [1, 2, 3],
            'version': "version'with\nquote", 'health': health}

  def round_for(devices, **kwargs):
    return {'devices': devices, 'dfu': [], 'has_internal': True, **kwargs}

  external = {**device, 'serial': 'red', 'kind': [7], 'internal': False}
  flash = {**device, 'signature': [9]}
  recovery = {**device, 'bootstub': True, 'flash_bootstub': True}
  empty = round_for([])
  single = round_for([device])
  mixed = round_for([external, device, {**external, 'serial': 'blue', 'kind': [1]}])
  rounds = [[empty] * 6, [single] * 3, [mixed] * 2, [empty, round_for([external]), single, single],
            [round_for([external], has_internal=False)] * 2, [round_for([flash])],
            [round_for([recovery])], [round_for([{**recovery, 'internal': False}])],
            [round_for([{**recovery, 'recover_bootstub': True}])],
            [round_for([{**flash, 'final_signature': []}])], [round_for([device], dfu=['dfu-b', 'dfu-a'])],
            [round_for([device], dfu=[None, 'dfu-a'])],
            [round_for([{**device, 'health': dict.fromkeys(fields, 0)}])]]
  result = [{'operation': 'main', 'rounds': values, 'fail_at': None, 'error_kind': 'other'} for values in rounds]
  for operation in ('flash', 'all'):
    result += [{'operation': operation, 'rounds': [values[0]], 'fail_at': None, 'error_kind': 'other'}
               for values in rounds[1:] if values[0]['devices']]
  baseline = list(result)
  for case in baseline:
    for index in range(len(original(case)['calls'])):
      for kind in ('no_device', 'pipe', 'protocol', 'other'):
        result.append({**case, 'fail_at': index, 'error_kind': kind})
  return result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  inputs = cases()
  text = ''.join(json.dumps(value) + '\n' for value in inputs)
  (args.output / 'inputs.jsonl').write_text(text)
  run = subprocess.run([*args.runner, str(args.binary.resolve())], input=text, text=True, capture_output=True, timeout=120)
  (args.output / 'native.jsonl').write_text(run.stdout)
  (args.output / 'native.stderr').write_text(run.stderr)
  run.check_returncode()
  observed = [json.loads(line) for line in run.stdout.splitlines()]
  assert len(observed) == len(inputs)
  calls = 0
  with (args.output / 'source.jsonl').open('w') as stream:
    for index, (case, actual) in enumerate(zip(inputs, observed, strict=True)):
      expected = original(case)
      stream.write(json.dumps(expected) + '\n')
      if actual != expected:
        (args.output / 'failure.json').write_text(json.dumps({'index': index, 'case': case, 'expected': expected, 'actual': actual}, indent=2))
        raise AssertionError(f'supervisor mismatch at scenario {index}; see failure.json')
      calls += len(expected['calls'])
  sources = [ROOT / 'openpilot/selfdrive/pandad' / name for name in ('pandad.py', 'panda_helpers.py')]
  report = {'result': 'PASS', 'scenarios': len(inputs), 'calls': calls,
            'binary_sha256': sha256(args.binary.read_bytes()).hexdigest(),
            'source_sha256': {str(path): sha256(path.read_bytes()).hexdigest() for path in sources},
            'limits': 'Unchanged wrapper function bodies, recorded device/Params/log/child boundaries. No physical transport or signal delivery.'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
