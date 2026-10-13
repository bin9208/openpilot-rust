import argparse
import ast
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace


def original(path, accepted):
  calls = []
  environment = {'LANG': 'original', 'LC_CTYPE': 'original'}

  def setlocale(category, candidate):
    assert category == 6
    calls.append(candidate)
    if candidate != accepted:
      raise ValueError('controlled missing locale')

  tree = ast.parse(path.read_text())
  function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == '_configure_autorun_locale')
  scope = {'locale': SimpleNamespace(setlocale=setlocale, LC_ALL=6, Error=ValueError), 'os': SimpleNamespace(environ=environment)}
  exec(compile(ast.Module([function], type_ignores=[]), str(path), 'exec'), scope)
  scope['_configure_autorun_locale']()
  return {'accepted': accepted, 'attempted': calls, 'environment': environment}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('root', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  source = args.root.resolve() / 'openpilot/selfdrive/carrot/cluster_autorun.py'
  controls = [original(source, accepted) for accepted in ['C.UTF-8', 'C', None]]
  result = {'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'controls': controls}
  if args.binary:
    binary = args.binary.resolve()
    for accepted, expected in zip(['C.UTF-8', 'C', 'none'], controls, strict=True):
      command = [str(binary), 'select', accepted]
      process = subprocess.run(command, text=True, capture_output=True, timeout=10, check=True)
      (output / f'{accepted}.stdout').write_text(process.stdout)
      (output / f'{accepted}.stderr').write_text(process.stderr)
      assert json.loads(process.stdout) == expected
    import os

    environment = {**os.environ, 'LANG': 'C', 'LC_CTYPE': 'C', 'LC_ALL': 'C'}
    command = [str(binary), 'native', str(output / 'native.events')]
    process = subprocess.Popen(command, env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    stdout, stderr = process.communicate(timeout=10)
    (output / 'native.stdout').write_text(stdout)
    (output / 'native.stderr').write_text(stderr)
    assert process.returncode == 0
    receipt = json.loads(stdout)
    assert receipt['pid'] == process.pid and receipt['charmap'] == 'UTF-8'
    assert receipt['environment'] == {'LC_ALL': 'C.UTF-8', 'LC_CTYPE': 'C.UTF-8', 'LANG': 'C.UTF-8'}
    events = [json.loads(line) for line in (output / 'native.events').read_text().splitlines()]
    assert len(events) == 2 and all(event['pid'] == process.pid for event in events)
    assert events[0]['lc_all'] == 'C' and events[1]['lc_all'] == 'C.UTF-8'
    actual = subprocess.run(
      [
        sys.executable,
        '-c',
        'import locale,os,json;locale.setlocale(locale.LC_ALL,"C.UTF-8");print(json.dumps({"charmap":locale.nl_langinfo(locale.CODESET)}))',
      ],
      env=environment,
      text=True,
      capture_output=True,
      timeout=10,
      check=True,
    )
    assert json.loads(actual.stdout)['charmap'] == receipt['charmap']
    result.update(
      {
        'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'controls_exact': 3,
        'native': receipt,
        'same_pid_reexec': True,
        'native_invocation': command,
      }
    )
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
