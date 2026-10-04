#!/usr/bin/env python3
import runpy
import os
import tomllib
from pathlib import Path
import sys
import types

from plannerd_policy_source import source_modules


def reject_runtime_boundary(*args, **kwargs):
  raise RuntimeError('acados generation attempted to use a runtime boundary')


def main():
  parameters = types.ModuleType('openpilot.common.params')
  parameters.Params = reject_runtime_boundary
  sys.modules[parameters.__name__] = parameters
  logging = types.ModuleType('openpilot.common.swaglog')
  logging.cloudlog = types.SimpleNamespace(**dict.fromkeys(('warning', 'info', 'error', 'event'), reject_runtime_boundary))
  sys.modules[logging.__name__] = logging
  source_modules()
  import numpy

  locked = tomllib.loads((Path(__file__).resolve().parents[2] / 'uv.lock').read_text())
  numpy_version = next(package['version'] for package in locked['package'] if package['name'] == 'numpy')
  if numpy.__version__ != numpy_version:
    raise RuntimeError('generator NumPy differs from uv.lock')
  import acados
  import casadi

  expected = Path(os.environ['ACADOS_SOURCE_DIR']).parent.resolve()
  if Path(acados.__file__).resolve().parent != expected or Path(casadi.__file__).resolve().parent != expected.parent / 'casadi':
    raise RuntimeError('generator imported an unexpected acados/CasADi package')
  if casadi.__version__ != '3.6.7':
    raise RuntimeError('generator CasADi version differs from pinned wheel')
  runpy.run_path(sys.argv[1], run_name='__main__')


if __name__ == '__main__':
  main()
