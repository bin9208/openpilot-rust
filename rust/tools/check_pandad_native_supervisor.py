import argparse
import ast
from collections.abc import Callable
from hashlib import sha256
import json
import math
import os
from pathlib import Path
import signal
import shlex
import subprocess
import threading
import time
from types import SimpleNamespace
import zmq
from check_pandad_client import source_classes

ROOT = Path(__file__).resolve().parents[2]


class Halt(BaseException):
  pass


class NoDevice(Exception):
  pass


class Pipe(Exception):
  pass


def original(case, firmware, child=None, live_log=None):
  namespace = source_classes()
  transfers, steps, logs, params, children = [], [], [], {}, []
  serial = case['serial']

  def cloud(entry):
    logs.append(entry)
    if live_log is not None:
      with live_log.open('a') as stream:
        stream.write(json.dumps(entry) + '\n')

  def transfer(kind, request, value, index, length, timeout, data=None):
    position = len(transfers)
    kind = kind | 0x80 if data is None else kind & 0x7f
    transfers.append(['control', kind, request, value, index, length, timeout, data])
    if case.get('fail_at') == position:
      code = case.get('error', -4)
      steps.append({'ret': code})
      raise {-4: NoDevice, -9: Pipe}.get(code, OSError)('owned transfer failure')
    defaults = {0xc1: [case['hardware']], 0xdd: [16, 4, 5], 0xd6: list(b'owned-version'),
                0xd3: list(range(64)), 0xd4: list(range(64, 128)), 0xd2: case['health']}
    reply = defaults.get(request, []) if data is None else []
    step = {'ret': len(reply), 'data': reply}
    if case.get('signal_at') == position:
      step['signal'] = signal.SIGINT
      if child is not None:
        os.kill(os.getpid(), signal.SIGINT)
        time.sleep(0.01)
    steps.append(step)
    return bytes(reply)

  class Handle:
    def controlRead(self, kind, request, value, index, length, timeout=15000):
      return transfer(kind, request, value, index, length, timeout)

    def controlWrite(self, kind, request, value, index, data, timeout=15000, expect_disconnect=False):
      transfer(kind, request, value, index, len(data), timeout, list(data))

    def close(self):
      pass

  class PandaLogger:
    def __getattr__(self, name):
      return lambda *args, **kwargs: None

  Panda = namespace['Panda']
  Panda.usb_connect = classmethod(lambda cls, wanted, claim=True, no_error=False:
                                (None, Handle(), serial, False, bytes([case['hardware']])))
  Panda.spi_connect = classmethod(lambda cls, wanted: (None, None, None, False, None))
  Panda.list = classmethod(lambda cls: [] if case.get('missing') else [serial])
  namespace.update({'logger': PandaLogger(), 'PANDA_BUS_CNT': 3, 'PandaSpiHandle': type('Spi', (), {}),
                    'FW_PATH': str(firmware), 'os': os, 'time': time})

  class CloudLog:
    def event(self, name, **values):
      if name == 'pandad.flash_and_connect' and values['count'] > case['cycles']:
        raise Halt
      cloud({'level': 'INFO', 'msg': {'event': name, **values}})

    def __getattr__(self, name):
      def emit(text):
        cloud({'level': 'ERROR' if name == 'exception' else name.upper(), 'msg': text})
      return emit

  class Params:
    def remove(self, key):
      params.pop(key, None)

    def put(self, key, value):
      params[key] = list(value)

    def put_bool(self, key, value):
      params[key] = list(b'1' if value else b'0')

  class Process:
    def __init__(self, argv, cwd):
      assert argv[0] == './pandad' and cwd == str(ROOT / 'openpilot/selfdrive/pandad')
      if case.get('spawn_failure'):
        raise FileNotFoundError('owned missing child')
      children.append(argv[1:])
      if child is not None:
        self.process = subprocess.Popen([str(child), *argv[1:]], cwd=cwd)

    def wait(self):
      if child is not None:
        self.process.wait()

    def send_signal(self, signum):
      self.process.send_signal(signum)

  wrapper = {'Panda': Panda, 'PandaT': Panda, 'Callable': Callable, 'PandaDFU': SimpleNamespace(list=list),
             'PandaProtocolMismatch': type('Mismatch', (Exception,), {}),
             'usb1': SimpleNamespace(USBErrorNoDevice=NoDevice, USBErrorPipe=Pipe), 'time': time,
             'os': SimpleNamespace(environ=os.environ if child is not None else {}, path=os.path),
             'signal': signal if child is not None else SimpleNamespace(SIGINT=signal.SIGINT, signal=lambda *args: None),
             'subprocess': SimpleNamespace(Popen=Process), 'BASEDIR': str(ROOT), 'FW_PATH': str(firmware),
             'HARDWARE': SimpleNamespace(has_internal_panda=lambda: False, reset_internal_panda=lambda: None,
                                         recover_internal_panda=lambda: None), 'Params': Params, 'cloudlog': CloudLog()}
  for name in ('panda_helpers.py', 'pandad.py'):
    filename = ROOT / 'openpilot/selfdrive/pandad' / name
    tree = ast.parse(filename.read_text())
    exec(compile(ast.Module(body=[node for node in tree.body if isinstance(node, ast.FunctionDef)], type_ignores=[]),
                 str(filename), 'exec'), wrapper)
  terminal = None
  try:
    wrapper['main']()
  except Halt:
    pass
  except Exception as error:
    terminal = type(error).__name__
  return {'transfers': transfers, 'logs': logs, 'params': params, 'children': children, 'terminal': terminal}, steps


def equal(value):
  if isinstance(value, float) and not math.isfinite(value):
    return repr(value)
  if isinstance(value, dict):
    return {key: equal(item) for key, item in value.items()}
  if isinstance(value, list):
    return [equal(item) for item in value]
  return value


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--launcher', type=Path, required=True)
  parser.add_argument('--library', type=Path, required=True)
  parser.add_argument('--preload', type=Path)
  parser.add_argument('--runner', default='')
  parser.add_argument('--child', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  library_dir = args.output / 'libraries'
  library_dir.mkdir()
  (library_dir / 'libusb-1.0.so.0').symlink_to(args.library.resolve())
  health = list(bytes(58))
  health[41] = health[57] = 1
  default = {'serial': '010002000300040005000600', 'hardware': 9, 'health': health, 'cycles': 2}
  cases = [default, {**default, 'hardware': 1}, {**default, 'health': [0] * 58},
           {**default, 'child_code': 7}, {**default, 'spawn_failure': True}]
  corpus_firmware = args.output / 'corpus-firmware'
  corpus_firmware.mkdir()
  for name in ('panda.bin.signed', 'panda_h7.bin.signed'):
    (corpus_firmware / name).write_bytes(bytes(range(128)))
  expected, _ = original(default, corpus_firmware)
  for point in range(len(expected['transfers'])):
    for error in (-4, -9, -7):
      cases.append({**default, 'fail_at': point, 'error': error})
  results = []
  for index, case in enumerate(cases):
    output = args.output / f'{index:04}'
    output.mkdir()
    firmware = output / 'firmware'
    firmware.mkdir()
    for name in ('panda.bin.signed', 'panda_h7.bin.signed'):
      (firmware / name).write_bytes(bytes(range(128)))
    source, steps = original(case, firmware)
    assert source['terminal'] == ('FileNotFoundError' if case.get('spawn_failure') else None), source
    (output / 'source.json').write_text(json.dumps(source, indent=2) + '\n')
    fixture = {'raw': True, 'devices': [{'vendor': 0x3801, 'product': 0xddcc,
                'serial': list(case['serial'].encode()), 'bcd': case['hardware'] << 8}], 'script': {'steps': steps}}
    (output / 'case.json').write_text(json.dumps({'case': case, 'fixture': fixture}) + '\n')
    prefix = f'panda-supervisor-{os.getpid()}-{index}'
    endpoint = f'ipc:///tmp/logmessage{prefix}'
    stop = threading.Event()
    collected = []
    ready = threading.Event()

    def collect(endpoint=endpoint, ready=ready, stop=stop, collected=collected):
      with zmq.Context() as context, context.socket(zmq.PULL) as receiver:
        receiver.bind(endpoint)
        ready.set()
        while not stop.is_set():
          if receiver.poll(25):
            packet = receiver.recv()
            collected.append(json.loads(packet[1:]))
    collector = threading.Thread(target=collect)
    collector.start()
    assert ready.wait(5)
    (output / 'owned-descriptor').write_bytes(b'owned regular file')
    env = {**os.environ, 'OPENPILOT_PREFIX': prefix, 'LD_LIBRARY_PATH': str(library_dir.resolve()),
           'PANDA_FIRMWARE_USB_LIBRARY': str(args.library.resolve()),
           'PANDA_FIRMWARE_USB_CASE': json.dumps(fixture), 'PANDA_FIRMWARE_USB_TRACE': str(output / 'usb.json'), 'LOGPRINT': 'debug',
           'PANDA_CHILD_TRACE': str(output / 'children.jsonl'), 'PANDA_OWNED_DESCRIPTOR': str(output / 'owned-descriptor'),
           'PANDA_CHILD_CODE': str(case.get('child_code', 0))}
    env.pop('LD_PRELOAD', None)
    if args.preload is not None:
      env['LD_PRELOAD'] = str(args.preload.resolve())
    child_path = output / 'missing-child' if case.get('spawn_failure') else args.child.resolve()
    command = [*shlex.split(args.runner), str(args.binary.resolve()), str(output / 'root'), str(ROOT), str(firmware), str(child_path),
               str(args.launcher.resolve()), str(case['cycles'])]
    try:
      run = subprocess.run(command, env=env, text=True, capture_output=True, timeout=20)
      (output / 'stdout').write_text(run.stdout)
      (output / 'stderr').write_text(run.stderr)
      assert run.returncode == int(source['terminal'] is not None), (index, run.returncode, run.stderr)
      deadline = time.monotonic() + 2
      while len(collected) < len(source['logs']) and time.monotonic() < deadline:
        stop.wait(0.01)
    finally:
      stop.set()
      collector.join(5)
      assert not collector.is_alive()
      Path('/tmp/logmessage' + prefix).unlink(missing_ok=True)
    (output / 'logs.json').write_text(json.dumps(collected, indent=2) + '\n')
    trace = json.loads((output / 'usb.json').read_text())
    native = {'transfers': [call for call in trace['calls'] if isinstance(call, list) and call[0] in ('control', 'bulk')],
              'logs': [{key: entry[key] for key in ('level', 'msg')} for entry in collected],
              'params': {path.name: list(path.read_bytes()) for path in (output / 'root/data/params/d').iterdir()}}
    expected = {key: value for key, value in source.items() if key not in ('children', 'terminal')}
    (output / 'native.json').write_text(json.dumps(native, indent=2) + '\n')
    assert equal(native) == equal(expected), (index, 'native/source mismatch')
    child_file = output / 'children.jsonl'
    child_rows = [json.loads(line) for line in child_file.read_text().splitlines()] if child_file.exists() else []
    assert [[bytes(argument).decode() for argument in row['args']] for row in child_rows] == source['children']
    assert all(row['manager'] and row['extra_fds'] == [] and bytes(row['cwd']).decode() == str(ROOT / 'openpilot/selfdrive/pandad') for row in child_rows)
    assert all(entry['filename'].endswith('.rs') for entry in collected)
    results.append({'case': index, 'transfers': len(native['transfers']), 'logs': len(collected), 'children_expected': len(source['children'])})
  report = {'result': 'PASS', 'scenarios': len(results), 'results': results,
            'native_runner': shlex.split(args.runner),
            'sha256': {str(path): sha256(path.read_bytes()).hexdigest() for path in (args.binary, args.launcher, args.library, args.child, Path(__file__))},
            'limits': 'Actual Rust environment, regular firmware/Params files, owned native USB ABI, live log IPC and native child. ' +
                      'Inherited descriptor closure checked. Unchanged Python wrapper/client bodies with scripted ' +
                      'transport/discovery/Params/child boundaries. No physical device or signal coverage.'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({key: value for key, value in report.items() if key != 'results'}))


if __name__ == '__main__':
  main()
