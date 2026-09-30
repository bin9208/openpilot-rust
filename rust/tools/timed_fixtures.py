"""Temporary host fixtures; no real sudo/date/rm/ln or external HTTP permitted."""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid

import zmq


@contextmanager
def environment(directory, timezone='UTC'):
  old = dict(os.environ)
  prefix = 'timed_' + uuid.uuid4().hex
  root = directory.resolve()
  root.mkdir(parents=True, exist_ok=True)
  params = root / prefix
  params.mkdir()
  zoneinfo = root / 'zoneinfo'
  for zone in ['Asia/Seoul', 'Etc/GMT', 'Etc/GMT-9', 'Etc/GMT+12', 'Etc/GMT-14', 'America/New_York']:
    path = zoneinfo / zone
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('synthetic timezone file, never installed on host')
  bindir = root / 'bin'
  bindir.mkdir()
  fixture = bindir / 'sudo'
  fixture.write_text(f'''#!{sys.executable}
import json, os, pathlib, sys
args = sys.argv[1:]
root = pathlib.Path(os.environ['TIMED_FIXTURE_ROOT']).resolve()
with (root / 'commands.jsonl').open('a') as f:
  f.write(json.dumps(args) + '\\n')
if os.environ.get('TIMED_COMMAND_FAIL') == args[0]:
  sys.exit(7)
if args[0] == 'date':
  assert args[1] == '-s' and args[2].startswith('@')
elif args[0] == 'rm':
  target = pathlib.Path(args[2])
  assert target.is_relative_to(root) and args[1] == '-f'
  target.unlink(missing_ok=True)
elif args[0] == 'ln':
  target, link = map(pathlib.Path, args[2:])
  assert target.is_relative_to(root) and link.is_relative_to(root) and args[1] == '-s'
  link.symlink_to(target)
else:
  raise AssertionError(args)
''')
  fixture.chmod(0o755)
  os.environ.update(OPENPILOT_PREFIX=prefix, PARAMS_ROOT=str(root), TIMED_FIXTURE_ROOT=str(root), PATH=str(bindir), TZ=timezone)
  for key in ['ZMQ', 'CEREAL_FAKE', 'TIMED_COMMAND_FAIL', 'LOGPRINT']:
    os.environ.pop(key, None)
  time.tzset()
  config = {'params_root': str(root), 'paths': {'zoneinfo': str(zoneinfo), 'localtime': str(root / 'etc/localtime'),
             'systemd': str(root / 'systemd')}, 'endpoint': 'http://127.0.0.1:1/unused',
            'wall': 1790000000000000000, 'monotonic': 0, 'actions': []}
  try:
    yield config, params
  finally:
    os.environ.clear()
    os.environ.update(old)
    time.tzset()


def commands(config):
  path = Path(config['params_root']) / 'commands.jsonl'
  if not path.exists():
    return []
  return [json.loads(line) for line in path.read_text().splitlines()]


def native(binary, config, output):
  context = zmq.Context()
  collector = context.socket(zmq.PULL)
  collector.setsockopt(zmq.LINGER, 0)
  collector.bind('ipc:///tmp/logmessage' + os.environ['OPENPILOT_PREFIX'])
  try:
    result = subprocess.run([binary], input=json.dumps(config) + '\n', text=True, capture_output=True, timeout=30)
    output.write_text(result.stdout)
    output.with_suffix('.stderr').write_text(result.stderr)
    assert result.returncode == 0, result.stderr
    records = []
    while collector.poll(50):
      raw = collector.recv()
      record = json.loads(raw[1:])
      records.append(record)
    output.with_suffix('.logs.json').write_text(json.dumps(records, indent=2) + '\n')
    return [json.loads(line) for line in result.stdout.splitlines()], records
  finally:
    collector.close()
    context.term()


def normalized(config, params, records):
  root = str(Path(config['params_root']))
  values = {key: (params / key).read_text() if (params / key).exists() else None for key in ['TimezoneName', 'TimezoneSource']}
  link = Path(config['paths']['localtime'])
  return {'params': values, 'target': str(link.readlink()).replace(root, '<root>') if link.is_symlink() else None,
          'commands': [[arg.replace(root, '<root>') for arg in row] for row in commands(config)],
          'records': records}
