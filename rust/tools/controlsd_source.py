"""Load unchanged controlsd code, substituting only IPC/Params/clock boundaries."""

import ast
from collections import deque
from contextlib import redirect_stdout
import io
import math
from numbers import Number
from pathlib import Path
import sys
import types

import numpy as np
from openpilot.cereal import car, log
from check_message_state import source as messaging_source
from locationd_loop_source import reader

CURRENT_STORE = None


def parameters_factory(*args):
  return CURRENT_STORE


def load(store):
  global CURRENT_STORE
  CURRENT_STORE = store
  root = Path(__file__).resolve().parents[2]
  parameters = types.ModuleType('openpilot.common.params')
  parameters.Params = parameters_factory
  sys.modules[parameters.__name__] = parameters
  logs = []
  swaglog = types.ModuleType('openpilot.common.swaglog')
  swaglog.cloudlog = types.SimpleNamespace(**{level: lambda text, level=level: logs.append([level, text]) for level in ('info', 'error', 'warning')})
  sys.modules[swaglog.__name__] = swaglog
  realtime = types.ModuleType('openpilot.common.realtime')
  realtime.DT_CTRL, realtime.DT_MDL = 0.01, 0.05
  sys.modules[realtime.__name__] = realtime
  from opendbc.car.values import BRANDS

  namespace = {'BRANDS': BRANDS}
  tree = ast.parse((root / 'opendbc_repo/opendbc/car/car_helpers.py').read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in ('load_interfaces', '_get_interface_names')]
  exec(compile(tree, 'source-interface-registry', 'exec'), namespace)
  interfaces = namespace['load_interfaces'](namespace['_get_interface_names']())
  from opendbc.car.vehicle_model import VehicleModel
  from opendbc.car.volkswagen.values import MEB_CURVATURE_PID_KP, MEB_CURVATURE_PID_KI, MEB_CURVATURE_PID_KF, MEB_CURVATURE_MAX
  from openpilot.common.pid import MultiplicativeUnwindPID
  from openpilot.common.constants import CV
  from openpilot.selfdrive.controls.lib.drive_helpers import clip_curvature, get_lag_adjusted_curvature, is_volkswagen_meb, CONTROL_N
  from openpilot.selfdrive.controls.lib.latcontrol import LatControl, MIN_LATERAL_CONTROL_SPEED
  from openpilot.selfdrive.controls.lib.latcontrol_angle import LatControlAngle, STEER_ANGLE_SATURATION_THRESHOLD
  from openpilot.selfdrive.controls.lib.latcontrol_pid import LatControlPID
  from openpilot.selfdrive.controls.lib.latcontrol_torque import LatControlTorque
  from openpilot.selfdrive.controls.lib.longcontrol import LongControl
  from openpilot.selfdrive.controls.lib.steer_ratio import resolve_vehicle_model_steer_ratio
  from openpilot.selfdrive.locationd.helpers import PoseCalibrator, Pose
  from openpilot.selfdrive.carrot.carrot_controls import CarrotControls
  from openpilot.selfdrive.carrot.carrot_man_input import get_carrot_man
  from openpilot.selfdrive.modeld.constants import ModelConstants

  scope = {
    'interfaces': interfaces,
    'VehicleModel': VehicleModel,
    'MEB_CURVATURE_PID_KP': MEB_CURVATURE_PID_KP,
    'MEB_CURVATURE_PID_KI': MEB_CURVATURE_PID_KI,
    'MEB_CURVATURE_PID_KF': MEB_CURVATURE_PID_KF,
    'MEB_CURVATURE_MAX': MEB_CURVATURE_MAX,
    'MultiplicativeUnwindPID': MultiplicativeUnwindPID,
    'CV': CV,
    'clip_curvature': clip_curvature,
    'get_lag_adjusted_curvature': get_lag_adjusted_curvature,
    'is_volkswagen_meb': is_volkswagen_meb,
    'CONTROL_N': CONTROL_N,
    'LatControl': LatControl,
    'MIN_LATERAL_CONTROL_SPEED': MIN_LATERAL_CONTROL_SPEED,
    'LatControlAngle': LatControlAngle,
    'STEER_ANGLE_SATURATION_THRESHOLD': STEER_ANGLE_SATURATION_THRESHOLD,
    'LatControlPID': LatControlPID,
    'LatControlTorque': LatControlTorque,
    'LongControl': LongControl,
    'resolve_vehicle_model_steer_ratio': resolve_vehicle_model_steer_ratio,
    'PoseCalibrator': PoseCalibrator,
    'Pose': Pose,
    'CarrotControls': CarrotControls,
    'get_carrot_man': get_carrot_man,
    'ModelConstants': ModelConstants,
  }
  messaging, environment = messaging_source()
  clock = types.SimpleNamespace(monotonic=lambda: 0.0)
  messaging['time'] = clock
  scope.update(
    np=np,
    math=math,
    Number=Number,
    deque=deque,
    car=car,
    log=log,
    time=clock,
    Params=parameters.Params,
    DT_CTRL=0.01,
    DT_MDL=0.05,
    cloudlog=swaglog.cloudlog,
    messaging=types.SimpleNamespace(new_message=messaging['new_message']),
    Priority=types.SimpleNamespace(CTRL_HIGH=53),
  )
  tree = ast.parse((root / 'openpilot/selfdrive/controls/controlsd.py').read_text())
  tree.body = [node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom, ast.If))]
  exec(compile(tree, 'source-controlsd', 'exec'), scope)
  return scope, messaging, environment, logs


class Finished(Exception):
  pass


def trace(case):
  from controlsd_parameters import Store

  store = Store({key: bytes(value) for key, value in case['params'].items()})
  source, messaging, environment, logs = load(store)
  environment['simulation'] = str(int(case['simulation']))
  current = {'index': -1, 'time': 99.0}
  output, published, scheduling, rate = [], [], [], []
  prints = io.StringIO()
  source['time'].monotonic = lambda: current['time']
  source['get_carrot_man'].__globals__['time'] = source['time']

  def capture(controls):
    output.append({'publications': list(published), 'params': list(store.operations), 'logs': list(logs), 'state': snapshot(controls)})
    published.clear()
    store.operations.clear()
    logs.clear()

  class SubMaster(messaging['SubMaster']):
    def update(self, timeout):
      assert timeout == 15
      current['index'] += 1
      if current['index'] == len(case['frames']):
        raise Finished()
      frame = case['frames'][current['index']]
      current['time'] = frame['time']
      store.values.update({key: bytes(value) for key, value in frame.get('params', {}).items()})
      self.update_msgs(frame['time'], [reader(packet) for packet in frame['messages']])

  class Ratekeeper:
    def __init__(self, frequency, print_delay_threshold):
      rate.append([frequency, print_delay_threshold])

    def monitor_time(self):
      capture(sys._getframe(1).f_locals['self'])

  def publish(service, message):
    published.append({'service': service, 'event': message.to_dict()})

  source['messaging'].SubMaster = SubMaster
  source['messaging'].PubMaster = lambda services: types.SimpleNamespace(send=publish)
  source['messaging'].log_from_bytes = lambda data, schema: reader_car(data)
  source['Ratekeeper'] = Ratekeeper
  source['config_realtime_process'] = lambda cores, priority: scheduling.append([cores, priority])
  with redirect_stdout(prints):
    try:
      source['main']()
    except Finished:
      pass
  assert scheduling == [[6, 53]] and rate == [[100, None]]
  return {'name': case['name'], 'rows': output, 'printed': prints.getvalue()}


def reader_car(data):
  with car.CarParams.from_bytes(data) as cp:
    return cp.as_builder().as_reader()


def snapshot(controls):
  lateral = controls.LaC
  state = {
    'curvature': controls.curvature,
    'desired': controls.desired_curvature,
    'safety_limited': controls.steer_limited_by_safety,
    'long_state': str(controls.LoC.long_control_state),
    'long_pid': [controls.LoC.pid.p, controls.LoC.pid.i, controls.LoC.pid.f, controls.LoC.pid.control],
    'last_accel': float(controls.LoC.last_output_accel),
    'coasting': controls.LoC.coasting.correction,
    'saturation': lateral.sat_count,
    'suspended': controls.carrot_controls.lat_suspend_active,
    'suspend_times': [controls.carrot_controls.lat_suspend_enter_t, controls.carrot_controls.lat_suspend_hold_t],
  }
  if hasattr(lateral, 'pid'):
    state['lat_pid'] = [lateral.pid.p, lateral.pid.i, lateral.pid.d, lateral.pid.f, lateral.pid.control]
  if controls.meb_curvature_pid is not None:
    pid = controls.meb_curvature_pid
    state['meb_pid'] = [pid.p, pid.i, pid.f, pid.control, pid.i_unwind_factor]
  return state


if __name__ == '__main__':
  import json

  request = json.loads(Path(sys.argv[1]).read_text())
  result = [trace(case) for case in request['cases']]
  Path(sys.argv[2]).write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps({'cases': len(result), 'frames': sum(len(case['rows']) for case in result)}))
