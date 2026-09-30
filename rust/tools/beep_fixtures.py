"""Disposable Params and PATH fixtures; no original GPIO paths are opened."""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import sys
import uuid


@contextmanager
def environment(root, binding, volume=b'10'):
  original = dict(os.environ)
  root = root.resolve()
  prefix = 'beep_' + uuid.uuid4().hex
  params = root / prefix
  params.mkdir(parents=True)
  if volume is not None:
    (params / 'SoundVolumeAdjust').write_bytes(volume)
  bindir = root / 'bin'
  bindir.mkdir()
  sudo = bindir / 'sudo'
  sudo.write_text(f'''#!{sys.executable}
import json, os, pathlib, sys, time
root = pathlib.Path(os.environ['BEEP_FIXTURE_ROOT'])
assert sys.argv[1] == 'tee' and len(sys.argv) == 3
path = sys.argv[2]
assert path in ['/sys/class/gpio/export', '/sys/class/gpio/gpio42/direction', '/sys/class/gpio/gpio42/value']
value = sys.stdin.buffer.read().decode()
status_file = root / 'status'
exit_status = int(status_file.read_text()) if status_file.exists() else 0
assert value in ['42\\n', 'out\\n', '0\\n', '1\\n']
def record(phase):
  row = {{'phase':phase,'path':path,'value':value.strip(),'time':time.monotonic(),'pid':os.getpid(),'status':exit_status}}
  fd = os.open(root / 'commands.jsonl', os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
  os.write(fd, (json.dumps(row) + '\\n').encode()); os.close(fd)
record('start')
if path.endswith('/value') and value == '1\\n' and (root / 'hold-on').exists():
  (root / ('held-' + str(os.getpid()))).touch()
  deadline = time.monotonic() + 5
  while not (root / 'release').exists() and time.monotonic() < deadline:
    time.sleep(0.005)
record('end')
sys.exit(exit_status)
''')
  sudo.chmod(0o755)
  os.environ.update(PARAMS_ROOT=str(root), OPENPILOT_PREFIX=prefix, BEEP_FIXTURE_ROOT=str(root), PATH=str(bindir), PYTHONUNBUFFERED='1')
  for key in ['ZMQ', 'CEREAL_FAKE', 'SIMULATION']:
    os.environ.pop(key, None)
  config = {'root': str(root), 'binding': str(binding), 'trace': str(root / 'trace.jsonl'), 'mode': 'policy', 'actions': []}
  try:
    yield config, params
  finally:
    os.environ.clear()
    os.environ.update(original)


def lines(path):
  if not Path(path).exists():
    return []
  return [json.loads(line) for line in Path(path).read_text().splitlines(keepends=True) if line.endswith('\n')]
