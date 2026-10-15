"""Hash-verified final radar source, executable only for test comparison."""

import ast
import enum
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

  if getattr(package, "__oracle_pin__", None) == "e8e152dbcfd9574a1791ed12026d1880e87689b3":
    return
  prefix = package.__name__
  for name in ['native', 'predictor', 'primary', 'stationary_lane', 'in_lane', 'trajectory_cutin', 'trajectory_cutout', 'lead_selection', 'controller']:
    path = 'openpilot/selfdrive/carrot/radar_motion/' + name + '.py'
    module = types.ModuleType(prefix + '.' + name)
    module.__file__ = f'{FIXTURE}:{path}'
    sys.modules[module.__name__] = module
    setattr(package, name, module)
    exec(compile(source(path), module.__file__, 'exec'), module.__dict__)

  exec(compile(source('openpilot/selfdrive/carrot/radar_motion/__init__.py'), str(FIXTURE), 'exec'), package.__dict__)
  name = 'openpilot.selfdrive.carrot.radar.tools.radar_validation_replay'
  module = types.ModuleType(name)
  path = 'openpilot/selfdrive/carrot/radar/tools/radar_validation_replay.py'
  module.__file__ = str(FIXTURE.parents[2] / path)
  sys.modules[name] = module
  exec(compile(source(path), str(FIXTURE) + ':' + path, 'exec'), module.__dict__)
  import openpilot.selfdrive.carrot.radar.tools as tools
  tools.radar_validation_replay = module
  import opendbc.car.hyundai.values as hyundai
  tree = ast.parse(source('opendbc_repo/opendbc/car/hyundai/values.py'))
  scope = {'IntFlag': enum.IntFlag}
  node = next(n for n in tree.body if isinstance(n, ast.ClassDef) and n.name == 'HyundaiExtFlags')
  exec(compile(ast.Module(body=[node], type_ignores=[]), str(FIXTURE), 'exec'), scope)
  hyundai.HyundaiExtFlags = scope['HyundaiExtFlags']
  package.__oracle_pin__ = "e8e152dbcfd9574a1791ed12026d1880e87689b3"


def tests(output):
  """Materialize the sealed source tests beside their source-inspection inputs."""
  root = output.resolve() / 'pinned-source'
  for path in json.loads(FIXTURE.read_text())['files']:
    if path.endswith(('.py', '.json')):
      target = root / path
      target.parent.mkdir(parents=True, exist_ok=True)
      target.write_text(source(path))
  repository = FIXTURE.parents[2]
  for path in ['openpilot/selfdrive/controls/radard.py', 'opendbc_repo/opendbc/car/volkswagen/values.py']:
    target = root / path
    target.parent.mkdir(parents=True, exist_ok=True)
    if not target.is_symlink():
      target.symlink_to(repository / path)
  return root / 'openpilot/selfdrive/carrot/tests'


def daemon():
  """Run the final source daemon with the same sealed radar policy cohort."""
  load()
  name = 'openpilot.selfdrive.carrot.radar.radard_dpath'
  module = types.ModuleType(name)
  module.__file__ = str(FIXTURE) + ':radard_dpath.py'
  sys.modules[name] = module
  exec(compile(source('openpilot/selfdrive/carrot/radar/radard_dpath.py'), module.__file__, 'exec'), module.__dict__)
  module.main()


def provenance():
  fixture = json.loads(FIXTURE.read_text())
  return {'source_commit': fixture['source_commit'], 'source_fixture': 'rust/upstream/radar-final-source.json',
          'sources': {path: record['sha256'] for path, record in fixture['files'].items()}}
