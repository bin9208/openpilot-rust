"""Select the final Card owner and its changed source dependencies for QA only."""
import hashlib
import importlib
import json
import subprocess
import sys
import types

from can_source import ROOT

PIN = 'e8e152dbcfd9574a1791ed12026d1880e87689b3'


def install(output):
  from card_hyundai_final_source import install as hyundai
  from card_toyota_final_source import install as toyota
  from card_tesla_final_source import install as tesla
  output.mkdir(parents=True, exist_ok=True)
  reports = [operation(output) for operation in (hyundai, toyota, tesla)]
  hashes = {}
  for name in ('openpilot.selfdrive.controls.lib.lateral_readiness',
               'openpilot.selfdrive.car.toyota_pcm_speed', 'openpilot.selfdrive.car.cruise',
               'openpilot.selfdrive.car.card'):
    path = name.replace('.', '/') + '.py'
    data = subprocess.check_output(['git', 'show', f'{PIN}:{path}'], cwd=ROOT)
    parent, attribute = name.rsplit('.', 1)
    package = importlib.import_module(parent)
    module = types.ModuleType(name)
    module.__file__ = f'{PIN}:{path}'
    sys.modules[name] = module
    setattr(package, attribute, module)
    exec(compile(data, module.__file__, 'exec'), module.__dict__)
    hashes[path] = hashlib.sha256(data).hexdigest()
  (output / 'source-provenance.json').write_text(json.dumps(
    {'source_commit': PIN, 'files': hashes, 'brand_packages': reports}, indent=2) + '\n')
