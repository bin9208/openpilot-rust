"""Load the built, unchanged Cython Params binding with an isolated logging path."""

import importlib.util
import sys
from types import ModuleType, SimpleNamespace


def load(binding, endpoint, log_root):
  hardware = ModuleType('openpilot.system.hardware.hw')
  hardware.Paths = SimpleNamespace(swaglog_ipc=lambda: endpoint, swaglog_root=lambda: str(log_root))
  sys.modules[hardware.__name__] = hardware
  from openpilot.common import swaglog

  spec = importlib.util.spec_from_file_location('openpilot.common.params_pyx', binding)
  module = importlib.util.module_from_spec(spec)
  sys.modules[spec.name] = module
  spec.loader.exec_module(module)
  return module, swaglog
