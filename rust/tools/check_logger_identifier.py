"""Compare source RouteCount/BootCount parsing and failed I/O boundaries (#99)."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess


def run(binary, key, directory, value, failure, runner):
  directory.mkdir(parents=True)
  params = directory / 'params/d'
  params.mkdir(parents=True)
  path = params / key
  path.write_bytes(value)
  if failure == 'directory':
    path.unlink()
    path.mkdir()
  elif failure == 'unreadable':
    path.chmod(0)
  elif failure == 'unwritable':
    params.chmod(0o500)
  environment = dict(os.environ, PARAMS_ROOT=str(params.parent), OPENPILOT_PREFIX='d')
  command = [*runner, str(binary), key]
  process = subprocess.run(command, env=environment, capture_output=True)
  params.chmod(0o700)
  if path.is_file():
    path.chmod(0o600)
  (directory / 'stdout.log').write_bytes(process.stdout or b'<empty>\n')
  (directory / 'stderr.log').write_bytes(process.stderr or b'<empty>\n')
  (directory / 'invocation.json').write_text(
    json.dumps({'argv': command, 'exit': process.returncode, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}, indent=2)
  )
  assert process.returncode == 0, process.stderr
  identifier = process.stdout.decode().strip()
  assert re.fullmatch('[0-9a-f]{8}--[0-9a-f]{10}', identifier), identifier
  result = {'prefix': identifier[:8], 'counter': path.read_bytes().hex() if path.is_file() else '<directory>'}
  (directory / 'result.json').write_text(json.dumps(result, indent=2))
  return result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('original', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  results = []
  for key in ['RouteCount', 'BootCount']:
    for i, value in enumerate(
      [b'', b'0', b' \t+42tail', b' -1suffix', b'4294967295', b'4294967296', b'18446744073709551615', b'18446744073709551616', b'junk', b'23\x00tail']
    ):
      for failure in ['ordinary', 'directory', 'unreadable', 'unwritable']:
        case = f'{key}-{i}-{failure}'
        root = args.output.resolve() / case
        expected = run(args.original.resolve(), key, root / 'source', value, failure, [])
        actual = run(args.binary.resolve(), key, root / 'native', value, failure, args.runner)
        assert actual == expected, (case, expected, actual)
        results.append({'scenario': case, 'result': 'PASS'})
        print(case, 'PASS', flush=True)
  (args.output / 'result.json').write_text(json.dumps({'result': 'PASS', 'cases': results}, indent=2))


if __name__ == '__main__':
  main()
