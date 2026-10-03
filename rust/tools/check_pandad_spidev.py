import argparse
import ast
from contextlib import contextmanager
import ctypes
import fcntl
from hashlib import sha256
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import shlex
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]


def original():
  library = os.environ['PANDA_SPI_LIBRARY']
  assert library in Path('/proc/self/maps').read_text()
  assert ctypes.CDLL(library).panda_spi_fixture_marker() == 175
  import spidev
  assert Path(spidev.__file__).parent == Path(library).parent
  namespace = {'os': os, 'spidev': spidev, 'ctypes': ctypes, 'fcntl': fcntl,
               'DEV_PATH': '/dev/spidev0.0', 'SPI_LOCK': threading.Lock(), 'SPI_DEVICES': {},
               'contextmanager': contextmanager, 'BaseHandle': object, 'Callable': object}
  tree = ast.parse((ROOT / 'panda/python/spi.py').read_text())
  names = {'PandaSpiException', 'PandaSpiUnavailable', 'PandaSpiTransfer', 'SpiDevice', 'PandaSpiHandle'}
  nodes = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name in names]
  for node in nodes:
    if node.name == 'PandaSpiHandle':
      node.body = [method for method in node.body if isinstance(method, ast.FunctionDef)
                   and method.name == '_transfer_kernel_driver']
  exec(compile(ast.Module(body=nodes, type_ignores=[]), 'panda/python/spi.py', 'exec'), namespace)
  sys.modules['spidev2'] = SimpleNamespace(SPI_IOC_RD_LSB_FIRST=0x80016b02)
  results = []
  for operation in json.loads(os.environ['PANDA_SPI_CASE'])['operations']:
    try:
      device = namespace['SpiDevice'](operation.get('speed', 50_000_000))
      if operation['kind'] == 'open':
        value = None
      else:
        with device.acquire() as spi:
          kind, data, length = operation['kind'], operation.get('data', []), operation.get('length', 1)
          if kind in ('xfer', 'xfer2'):
            value = getattr(spi, kind)(data)
          elif kind == 'write':
            value = spi.writebytes(data)
          elif kind == 'read':
            value = spi.readbytes(length)
          else:
            handle = namespace['PandaSpiHandle']()
            handle.tx_buf, handle.rx_buf = bytearray(1024), bytearray(4096)
            handle.ioctl_data = namespace['PandaSpiTransfer']()
            handle.ioctl_data.tx_buf = ctypes.addressof(ctypes.c_char.from_buffer(handle.tx_buf))
            handle.ioctl_data.rx_buf = ctypes.addressof(ctypes.c_char.from_buffer(handle.rx_buf))
            handle.fileno = spi.fileno()
            value = list(handle._transfer_kernel_driver(spi, operation.get('endpoint', 0), bytes(data),
                                                       15000, length, operation.get('disconnect', False)))
      results.append({'ok': True, 'value': value})
    except namespace['PandaSpiUnavailable']:
      results.append({'ok': True, 'value': {'missing': True}})
    except ValueError as error:
      results.append({'ok': False, 'error': 'io' if str(error).startswith('file descriptor cannot be a negative integer') else 'invalid'})
    except (AssertionError, OverflowError, TypeError):
      results.append({'ok': False, 'error': 'invalid'})
    except (OSError, SystemError):
      results.append({'ok': False, 'error': 'io'})
    except namespace['PandaSpiException']:
      results.append({'ok': False, 'error': 'protocol'})
  print(json.dumps(results))


def corpus():
  operations = [{'kind': 'open'}, {'kind': 'open', 'speed': 1_000_000}, {'kind': 'open'},
                {'kind': 'open', 'speed': 1_000_000}]
  for speed in (1_000_000, 50_000_000):
    for kind in ('xfer', 'xfer2', 'read', 'write', 'kernel'):
      for length in (0, 1, 9, 68, 256):
        operations.append({'kind': kind, 'speed': speed, 'data': [i % 256 for i in range(length)],
                           'length': length, 'endpoint': 2, 'disconnect': bool(length % 2)})
  yield {'operations': operations, 'response': [1, 127, 255]}
  for kind in ('read', 'write', 'xfer', 'xfer2'):
    for length in (4096, 4097):
      yield {'operations': [{'kind': kind, 'length': length, 'data': [7] * length}], 'response': [9]}
  for field in ('missing', 'mode', 'initial_speed', 'read_result', 'write_result', 'kernel_result'):
    for value in ((True,) if field == 'missing' else (0, 1, 4, 8)):
      yield {'operations': operations, 'response': [1, 127, 255], field: value}
  yield {'operations': [{'kind': 'open', 'speed': 50_000_001}]}
  for speed in (1_000_000, 50_000_000):
    for kind in ('open', 'xfer', 'xfer2', 'read', 'write', 'kernel'):
      for point in range(5 + int(speed == 1_000_000) + (0 if kind == 'open' else 3)):
        for error in (4, 5, 9, 13, 19):
          yield {'operations': [{'kind': kind, 'speed': speed, 'data': [0, 127, 255], 'length': 9}],
                 'response': [1, 127, 255], 'fail_at': point, 'error': error}
  for point in range(6):
    yield {'operations': [{'kind': 'open'}, {'kind': 'xfer', 'data': [1]}], 'fail_at': point, 'error': 5}


def operational(trace):
  calls = trace['calls'][:]
  while calls and calls[-1][0] == 'close':
    calls.pop()
  return calls


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--native', type=Path, required=True)
  parser.add_argument('--library', type=Path, required=True)
  parser.add_argument('--native-library', type=Path)
  parser.add_argument('--runner', default='')
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  cases = list(corpus())
  (args.output / 'cases.json').write_text(json.dumps(cases) + '\n')
  rows, calls = [], 0
  index = 0
  while index < len(cases):
    case = cases[index]
    paths = []
    for lane in ('source', 'native'):
      prefix = args.output / f'{index:04}-{lane}'
      library = args.native_library if lane == 'native' and args.native_library else args.library
      env = {**os.environ, 'LD_PRELOAD': str(library), 'PANDA_SPI_LIBRARY': str(library),
             'PANDA_SPI_CASE': json.dumps(case), 'PANDA_SPI_TRACE': str(prefix.with_suffix('.trace.json')),
             'PYTHONPATH': str(args.library.parent)}
      command = ([sys.executable, str(Path(__file__).resolve()), '--original'] if lane == 'source'
                 else [*shlex.split(args.runner), str(args.native)])
      run = subprocess.run(command, env=env, text=True, capture_output=True, timeout=15)
      prefix.with_suffix('.stdout').write_text(run.stdout)
      prefix.with_suffix('.stderr').write_text(run.stderr)
      assert run.returncode == 0, (index, lane, run.returncode, run.stderr)
      paths.append((json.loads(run.stdout), json.loads(prefix.with_suffix('.trace.json').read_text())))
    source, native = paths
    same = source[0] == native[0] and operational(source[1]) == operational(native[1])
    row = {'index': index, 'result': 'PASS' if same else 'FAIL', 'calls': len(operational(source[1])),
           'source_active_at_exit': source[1]['active_at_exit'], 'native_active_at_exit': native[1]['active_at_exit']}
    rows.append(row)
    if not same:
      (args.output / 'failure.json').write_text(json.dumps({'case': case, 'source': source, 'native': native}, indent=2) + '\n')
      raise AssertionError((index, case.get('fail_at'), case.get('error'), 'native source mismatch'))
    assert native[1]['active_at_exit'] == 0, row
    calls += row['calls']
    index += 1
  report = {'result': 'PASS', 'scenarios': len(rows), 'calls': calls, 'rows': rows,
            'sha256': {str(path): sha256(path.read_bytes()).hexdigest() for path in
                       (args.native, args.library, args.native_library or args.library, ROOT / 'panda/python/spi.py', Path(__file__))},
            'native_runner': shlex.split(args.runner),
            'limits': 'Unmodified SpiDevice and kernel method with original spidev3.8; owned syscall fixture only. ' +
                      'Only trailing cleanup close calls are excluded from policy comparison; raw traces retained. ' +
                      'Python negative-descriptor ValueError and Rust EBADF are both nonretryable I/O failures. ' +
                      'Kernel read buffer enlarged in fixture to contain supplied reply; no physical device.'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({key: value for key, value in report.items() if key != 'rows'}))


if __name__ == '__main__':
  original() if sys.argv[1:] == ['--original'] else main()
