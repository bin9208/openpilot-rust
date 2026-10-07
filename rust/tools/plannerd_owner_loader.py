"""Load original planner owners, isolating only I/O, clocks and native library location."""

import importlib
import importlib.util
import sys
import types

from check_message_state import source
from controlsd_parameters import Store
from generate_selfdrive_alerts import load_source
from plannerd_policy_source import source_modules


def register_solvers(native_root):
  for kind in ('lateral', 'longitudinal'):
    directory = native_root / kind / 'c_generated_code'
    package_name = f'openpilot.selfdrive.controls.lib.{kind}_mpc_lib.c_generated_code'
    package = types.ModuleType(package_name)
    package.__path__ = [str(directory)]
    sys.modules[package_name] = package
    name = package_name + '.acados_ocp_solver_pyx'
    spec = importlib.util.spec_from_file_location(name, directory / 'acados_ocp_solver_pyx.so')
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)


class SourceOwners:
  def __init__(self, native_root, values):
    self.store = Store({key: value.encode() for key, value in values.items()})
    self.logs = []
    self.now = 100.0
    self.wall = 1000.0
    source_modules()
    parameters = types.ModuleType('openpilot.common.params')
    parameters.Params = lambda *args, **kwargs: self.store
    sys.modules[parameters.__name__] = parameters
    logging = types.ModuleType('openpilot.common.swaglog')
    logging.cloudlog = types.SimpleNamespace(**{name: self.logger(name) for name in ('info', 'warning', 'error', 'event')})
    sys.modules[logging.__name__] = logging
    events = types.ModuleType('openpilot.selfdrive.selfdrived.events')
    events.Events = load_source('tici')['Events']
    sys.modules[events.__name__] = events
    scope, _environment = source()
    scope['time'].monotonic = lambda: self.now
    messaging = types.ModuleType('openpilot.cereal.messaging')
    messaging.new_message = scope['new_message']
    messaging.SubMaster = scope['SubMaster']
    sys.modules[messaging.__name__] = messaging
    register_solvers(native_root)
    self.long = importlib.import_module('openpilot.selfdrive.controls.lib.longitudinal_planner')
    self.lat = importlib.import_module('openpilot.selfdrive.controls.lib.lateral_planner')
    self.carrot = importlib.import_module('openpilot.selfdrive.carrot.carrot_functions')
    self.navigation = importlib.import_module('openpilot.selfdrive.carrot.carrot_man_input')
    long_mpc = importlib.import_module('openpilot.selfdrive.controls.lib.longitudinal_mpc_lib.long_mpc')
    lat_mpc = importlib.import_module('openpilot.selfdrive.controls.lib.lateral_mpc_lib.lat_mpc')
    for module in (self.lat, long_mpc, lat_mpc, self.navigation):
      module.time = types.SimpleNamespace(monotonic=lambda: self.now)
    self.carrot.time = types.SimpleNamespace(time=lambda: self.wall)
    self.messaging = messaging

  def logger(self, level):
    def record(*args, **kwargs):
      self.logs.append((level, args, kwargs))

    return record
