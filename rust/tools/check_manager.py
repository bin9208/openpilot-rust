"""Focused source/native manager comparison; fixture outputs remain inspectable."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('binding', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--scenario', action='append')
  args = parser.parse_args()
  args.binary, args.binding, args.output = (path.resolve() for path in (args.binary, args.binding, args.output))
  args.output.mkdir(parents=True, exist_ok=True)
  environment = dict(os.environ)
  environment['PYTHONPATH'] = str(Path.cwd())
  for key in ('DISABLE_WIDE_ROAD', 'DONGLE_ID', 'GIT_ORIGIN', 'GIT_BRANCH', 'GIT_COMMIT', 'CLEAN'):
    environment.pop(key, None)
  scenarios = args.scenario or ['normal', 'registration_failure', 'poll_failure', 'prepare_only', 'interrupted', 'default_edges', 'reset_defaults']
  for scenario in scenarios:
    for kind in ('source', 'native'):
      output = args.output / f'{scenario}-{kind}'
      command = ([sys.executable, 'rust/tools/manager_reference.py', str(args.binding)] if kind == 'source' else [str(args.binary)]) + [str(output), scenario]
      with (args.output / f'{scenario}-{kind}.log').open('w') as log:
        subprocess.run(command, env=environment, check=True, stdout=log, stderr=subprocess.STDOUT)
    source = json.loads((args.output / f'{scenario}-source/result.json').read_text())
    native = json.loads((args.output / f'{scenario}-native/result.json').read_text())
    if source != native:
      for key in source:
        if source[key] != native[key]:
          print(scenario, key, 'source:', source[key], 'native:', native[key])
      raise AssertionError(f'{scenario} differs')
    print(f'PASS {scenario}: exact trace, Params bytes, environment')
  summary = {'passed': scenarios, 'source': 'openpilot/system/manager/manager.py',
             'limitations': 'isolated external boundaries; no device/production selection'}
  (args.output / 'summary.json').write_text(json.dumps(summary, indent=2))


if __name__ == '__main__':
  main()
