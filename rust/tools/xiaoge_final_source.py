"""Bind changed Xiaoge reference modules to the candidate's upstream pin.

The independent Rust checkout deliberately retains baseline Python runtime files.
Validation must not compare the new model/policy against those older definitions.
"""
import hashlib
import importlib
from pathlib import Path
import subprocess
import sys
from types import ModuleType

PIN = 'e8e152dbcfd9574a1791ed12026d1880e87689b3'
ROOT = Path(__file__).resolve().parents[2]
HASHES = {}


def load(*, server=False, producer=False):
  names = ['openpilot.selfdrive.carrot.xiaoge.v_asm_inference']
  if server or producer:
    names.append('openpilot.selfdrive.carrot.xiaoge.v_asm_server')
  if producer:
    names.append('openpilot.selfdrive.carrot.xiaoge_data')
  for full_name in names:
    parent, _, name = full_name.rpartition('.')
    package = importlib.import_module(parent)
    previous = sys.modules.get(full_name)
    if previous is not None and getattr(previous, '__source_commit__', None) == PIN:
      continue
    path = full_name.replace('.', '/') + '.py'
    source = subprocess.check_output(['git', 'show', f'{PIN}:{path}'], cwd=ROOT)
    module = ModuleType(full_name)
    module.__file__ = str(ROOT / path)
    module.__package__ = package.__name__
    module.__source_commit__ = PIN
    module.__source_sha256__ = hashlib.sha256(source).hexdigest()
    HASHES[path] = module.__source_sha256__
    sys.modules[full_name] = module
    setattr(package, name, module)
    exec(compile(source, f'{PIN}:{path}', 'exec'), module.__dict__)
