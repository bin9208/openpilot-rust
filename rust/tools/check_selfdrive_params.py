import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import signal
import subprocess
import sys
import tempfile

import zmq

from check_params_string import receive
from original_params_binding import load

FATAL_READER = '''
import sys
from original_params_binding import load
module, _ = load(__import__('pathlib').Path(sys.argv[1]), sys.argv[3], __import__('pathlib').Path(sys.argv[2]) / 'logs')
print(module.Params(sys.argv[2]).get_int('HyundaiCameraSCC'))
'''


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--native', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  rows = []
  with tempfile.TemporaryDirectory(prefix='selfdrive-params-') as temporary:
    root = Path(temporary)
    os.environ['OPENPILOT_PREFIX'] = 'fixture'
    context = zmq.Context()
    sockets = [context.socket(zmq.PULL) for _ in range(2)]
    endpoints = ['ipc://' + str(root / name) for name in ('source-log', 'native-log')]
    for socket, endpoint in zip(sockets, endpoints, strict=True):
      socket.setsockopt(zmq.RCVTIMEO, 5000)
      socket.bind(endpoint)
    module, swaglog = load(args.binding.resolve(), endpoints[0], root / 'logs')
    params = module.Params(str(root / 'source'))
    with (args.output / 'native.stderr').open('w') as stderr:
      process = subprocess.Popen([str(args.native.resolve()), str(root / 'native'), 'fixture', endpoints[1]],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True)
      try:
        assert process.stdout.readline().strip() == 'ready'
        cases = []
        for key in ('NNFFModelName', 'CanParserResult'):
          cases.extend((key, 'text', value, None) for value in (None, b'', b'model', '모델\0😀'.encode(), b'\xff', b'one\'\xff\n', b'\xc0\x80'))
          cases.extend((key, 'text', None, special) for special in ('permission', 'directory'))
        cases.extend(('HyundaiCameraSccHint', 'boolean', value, None) for value in (None, b'', b'1', b'0', b' 1', b'1\0', b'true', b'\xff'))
        cases.extend(('HyundaiCameraSCC', 'integer', value, None) for value in
                     (None, b'', b'0', b'1', b' +12suffix', b'-2147483648', b'2147483647', b'  0x10', b'123\0tail'))
        cases.extend((key, operation, None, 'directory') for key, operation in
                     (('HyundaiCameraSCC', 'integer'), ('HyundaiCameraSccHint', 'boolean')))
        cases.extend(('NotARegisteredKey', operation, None, None) for operation in ('text', 'integer', 'boolean'))
        cases.extend(('HyundaiCameraSCC', 'integer', value, 'fatal') for value in
                     (b'words', b'+', b'2147483648', b'-2147483649', b'\xff', b'\0', '１２'.encode()))
        for index, (key, operation, value, special) in enumerate(cases):
          for kind in ('source', 'native'):
            path = root / kind / 'fixture' / key
            if path.exists():
              if path.is_dir():
                path.rmdir()
              else:
                path.unlink()
            if special == 'directory':
              path.mkdir()
            elif value is not None or special == 'permission':
              path.write_bytes(value if value is not None else b'text')
              if special == 'permission':
                path.chmod(0)
          source_failure = None
          if special == 'fatal':
            env = dict(os.environ, PYTHONPATH=os.pathsep.join([str(Path(__file__).resolve().parent), os.environ.get('PYTHONPATH', '')]))
            run = subprocess.run([sys.executable, '-c', FATAL_READER, str(args.binding.resolve()), str(root / 'source'), endpoints[0]],
                                 env=env, capture_output=True, timeout=10)
            (args.output / f'source-fatal-{index}.stderr').write_bytes(run.stderr)
            assert run.returncode == -signal.SIGABRT, (run.returncode, run.stderr)
            source_failure = {'returncode': run.returncode, 'stderr': run.stderr.decode(errors='replace')}
            expected = {'error': 'fatal_integer'}
          else:
            try:
              getter = {'text': params.get, 'integer': params.get_int, 'boolean': params.get_bool}[operation]
              expected = {'value': getter(key)}
            except module.UnknownKeyName:
              expected = {'error': 'unknown_key'}
          marker = f'barrier-{index}'
          swaglog.cloudlog.debug(marker)
          source_records = receive(sockets[0], marker)
          process.stdin.write(json.dumps({'operation': operation, 'key': key, 'marker': marker}) + '\n')
          process.stdin.flush()
          actual = json.loads(process.stdout.readline())
          native_records = receive(sockets[1], marker)
          assert actual == expected, (index, key, operation, actual, expected)
          assert [r['record']['msg'] for r in source_records] == [r['record']['msg'] for r in native_records], index
          rows.append({'key': key, 'operation': operation, 'bytes': None if value is None else value.hex(), 'special': special,
                       'source': expected, 'native': actual, 'source_failure': source_failure,
                       'source_logs': source_records, 'native_logs': native_records})
        process.stdin.close()
        assert process.wait(timeout=5) == 0
      finally:
        if process.poll() is None:
          process.kill()
          process.wait()
        for socket in sockets:
          socket.close()
        context.term()
  report = {'passed': True, 'cases': rows, 'native_sha256': hashlib.sha256(args.native.read_bytes()).hexdigest(),
            'binding_sha256': hashlib.sha256(args.binding.read_bytes()).hexdigest()}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2))
  print(json.dumps({'passed': True, 'cases': len(rows)}))


if __name__ == '__main__':
  main()
