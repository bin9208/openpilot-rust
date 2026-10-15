"""Hash-verified final radar source, executable only for test comparison."""

import hashlib
import json
from pathlib import Path
import sys
import types

FIXTURE = Path(__file__).resolve().parents[1] / 'upstream/radar-final-source.json'


def source(path):
  fixture = json.loads(FIXTURE.read_text())
  assert fixture['source_commit'] == 'e8e152dbcfd9574a1791ed12026d1880e87689b3'
  record = fixture['files'][path]
  data = record['content'].encode()
  assert hashlib.sha256(data).hexdigest() == record['sha256']
  assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == record['git_blob']
  return record['content']


def load():
  import openpilot.selfdrive.carrot.radar_motion as package

  prefix = package.__name__
  for name in ['native', 'predictor', 'primary', 'stationary_lane', 'in_lane', 'trajectory_cutin', 'controller']:
    path = 'openpilot/selfdrive/carrot/radar_motion/' + name + '.py'
    module = types.ModuleType(prefix + '.' + name)
    module.__file__ = f'{FIXTURE}:{path}'
    sys.modules[module.__name__] = module
    setattr(package, name, module)
    exec(compile(source(path), module.__file__, 'exec'), module.__dict__)
