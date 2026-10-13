import argparse
import hashlib
import http.client
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import tempfile
import time


def fixture(root, label, source):
  web = root / 'openpilot/selfdrive/carrot/web'
  (web / 'js').mkdir(parents=True)
  (root / 'openpilot/selfdrive/assets').mkdir()
  (root / 'openpilot/selfdrive/carrot_settings.json').write_text('{"params": []}')
  (web / 'js/marker.js').write_text(label)
  config = root / 'openpilot/selfdrive/carrot/server/config.py'
  config.parent.mkdir()
  shutil.copy2(source / 'openpilot/selfdrive/carrot/server/config.py', config)
  spec = importlib.util.spec_from_file_location('owned_runtime_config', config)
  assert spec is not None and spec.loader is not None
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  return {'root': str(root), 'web': module.WEB_DIR, 'settings': module.DEFAULT_SETTINGS_PATH, 'label': label}


def environment(root):
  env = os.environ.copy()
  for key in ('OPENPILOT_ROOT', 'BASEDIR', 'CARROT_DATA_DIR', 'CARROT_SETTINGS_PATH', 'PARAMS_ROOT', 'OPENPILOT_PREFIX'):
    env.pop(key, None)
  env.update(CARROT_DATA_DIR=str(root / 'data'), PARAMS_ROOT=str(root / 'params'))
  return env


def stopped(binary, cwd, env, arguments):
  result = subprocess.run([str(binary), *arguments], cwd=cwd, env=env, capture_output=True, text=True, timeout=10)
  return {'arguments': arguments, 'cwd': str(cwd), 'exit': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr}


def serving(binary, cwd, env, expected):
  with socket.socket() as reservation:
    reservation.bind(('127.0.0.1', 0))
    port = reservation.getsockname()[1]
  arguments = ['--host', '127.0.0.1', '--port', str(port)]
  child = subprocess.Popen([str(binary), *arguments], cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
  try:
    deadline = time.monotonic() + 5
    while True:
      assert child.poll() is None, 'CLI exited before owned asset request'
      connection = http.client.HTTPConnection('127.0.0.1', port, timeout=2)
      try:
        connection.request('GET', '/js/marker.js', headers={'Accept-Encoding': 'identity'})
        response = connection.getresponse()
        body = response.read()
        headers = list(response.getheaders())
        break
      except ConnectionRefusedError:
        assert time.monotonic() < deadline, 'CLI listener did not start'
        time.sleep(0.02)
      finally:
        connection.close()
    child.send_signal(signal.SIGTERM)
    stdout, stderr = child.communicate(timeout=10)
    row = {'arguments': arguments, 'cwd': str(cwd), 'expected': expected, 'status': response.status,
           'headers': headers, 'body_hex': body.hex(), 'exit': child.returncode, 'stdout': stdout, 'stderr': stderr}
    assert response.status == 200 and body.decode() == expected['label'], row
    assert child.returncode == 0 and f"serving {expected['web']} on" in stdout, row
    return row
  finally:
    if child.poll() is None:
      child.kill()
      child.communicate(timeout=5)


def run(binary, output, before):
  output.mkdir(parents=True, exist_ok=True)
  source = Path(__file__).resolve().parents[2]
  free = shutil.disk_usage(source).free
  growth = binary.stat().st_size * 2 + 1024 * 1024
  assert free >= 25 * 1024 ** 3 + growth, free
  assert not Path('/data/openpilot/openpilot/selfdrive/carrot/data/state').exists()
  with tempfile.TemporaryDirectory(prefix='carrot-owned-cli-') as temporary:
    base = Path(temporary)
    first = fixture(base / 'first', 'relocated executable assets', source)
    second = fixture(base / 'second', 'working directory assets', source)
    outside = base / 'outside'
    outside.mkdir()
    nested = base / 'second/nested/child'
    nested.mkdir(parents=True)
    paths = [base / 'first/native/bin/carrot-server', outside / 'bin/carrot-server']
    for path in paths:
      path.parent.mkdir(parents=True)
      shutil.copy2(binary, path)
    env = environment(base)
    rows = {}
    if before:
      with socket.socket() as occupied:
        occupied.bind(('127.0.0.1', 0))
        occupied.listen()
        arguments = ['--host', '127.0.0.1', '--port', str(occupied.getsockname()[1])]
        row = stopped(paths[0], outside, env, arguments)
      row['expected'] = first
      row['selected_runtime_assets'] = f"serving {first['web']} on" in row['stdout']
      rows['relocated-before'] = row
      assert row['exit'] == 1 and not row['selected_runtime_assets'], row
    else:
      rows['executable-ancestors'] = serving(paths[0], outside, env, first)
      rows['cwd-ancestors'] = serving(paths[1], nested, env, second)
      rows['openpilot-root-priority'] = serving(paths[0], outside, {**env, 'OPENPILOT_ROOT': second['root'], 'BASEDIR': first['root']}, second)
      rows['basedir'] = serving(paths[1], outside, {**env, 'BASEDIR': first['root']}, first)
      missing = str(base / 'not-present')
      bad_env = {**env, 'OPENPILOT_ROOT': missing, 'BASEDIR': first['root']}
      rows['help-before-root'] = stopped(paths[1], outside, bad_env, ['--help'])
      assert rows['help-before-root']['exit'] == 0 and 'usage:' in rows['help-before-root']['stdout']
      rows['argument-before-root'] = stopped(paths[1], outside, bad_env, ['--unknown'])
      assert rows['argument-before-root']['exit'] == 1 and 'unrecognized arguments' in rows['argument-before-root']['stderr']
      rows['missing-root'] = stopped(paths[1], outside, env, [])
      assert rows['missing-root']['exit'] == 1 and 'cannot locate runtime repository' in rows['missing-root']['stderr']
      rows['invalid-explicit-root'] = stopped(paths[1], outside, bad_env, [])
      assert rows['invalid-explicit-root']['exit'] == 1 and 'No such file or directory' in rows['invalid-explicit-root']['stderr']
    (output / 'observations.json').write_text(json.dumps(rows, indent=2) + '\n')
    (output / 'result.json').write_text(json.dumps({'passed': True, 'before': before, 'cases': len(rows),
      'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
      'source_config_sha256': hashlib.sha256((source / 'openpilot/selfdrive/carrot/server/config.py').read_bytes()).hexdigest(),
      'free_before': free, 'estimated_copy_growth': growth,
      'scope': 'runtime-root/CLI ordering only; native partial server, owned Params/state/assets and loopback; source config executed after relocation'}) + '\n')


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--before', action='store_true')
  args = parser.parse_args()
  run(args.binary.resolve(), args.output, args.before)


if __name__ == '__main__':
  main()
