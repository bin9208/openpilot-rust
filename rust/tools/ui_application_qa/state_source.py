"""Run the unchanged UIState/Device class bodies with owned input/Params/hardware seams."""

import ast
from collections.abc import Callable
from enum import Enum
import json
from pathlib import Path
import sys
from types import SimpleNamespace
import numpy as np

root = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(root))
from openpilot.selfdrive.ui.carrot_param_cache import RealtimeUiParamSnapshot, TimedSnapshotCache, read_realtime_ui_params
from openpilot.common.filter_simple import FirstOrderFilter
from openpilot.cereal import car

scene = json.load(sys.stdin)
now = 0.0
models = {'compiled': False, 'compile_pending': False}
effects = []
transitions = []
worker_busy = False


class Params:
  def __init__(self, *args):
    self.values = {key: bytes(value) for key, value in scene['params'].items()}
    self.failures = []
    self.reads = []

  def raw(self, key):
    self.reads.append(key)
    if key in self.failures:
      raise RuntimeError('fixture read failure')
    return self.values.get(key)

  def get(self, key):
    value = self.raw(key)
    return value if key == 'CarParamsPersistent' or value is None else value.decode()

  def get_int(self, key):
    return int(self.raw(key) or b'0')

  def get_bool(self, key):
    return self.raw(key) == b'1'


class SubMaster:
  def __init__(self, services):
    self.values = {}

  def __getitem__(self, name):
    return self.values[name]

  def update(self, timeout):
    pass


class Thread:
  def __init__(self, target, args):
    self.target, self.args = target, args

  def start(self):
    self.target(*self.args)

  def is_alive(self):
    return worker_busy


def log_from_bytes(value, schema):
  with schema.from_bytes(value) as cp:
    return SimpleNamespace(alphaLongitudinalAvailable=cp.alphaLongitudinalAvailable, openpilotLongitudinalControl=cp.openpilotLongitudinalControl)


log = SimpleNamespace(
  PandaState=SimpleNamespace(PandaType=SimpleNamespace(unknown=0)),
  LongitudinalPersonality=SimpleNamespace(standard=1),
  SelfdriveState=SimpleNamespace(OpenpilotState=SimpleNamespace(preEnabled='preEnabled', overriding='overriding')),
)
gui = SimpleNamespace(target_fps=20, big_ui=lambda: scene['big'], mouse_events=[], set_should_render=lambda awake: None)
hardware = SimpleNamespace(
  get_device_type=lambda: 'mici' if scene['mici'] else 'tici',
  set_screen_brightness=lambda value: effects.append({'kind': 'brightness', 'value': value}),
  set_display_power=lambda value: effects.append({'kind': 'display_power', 'value': value}),
)
namespace = {
  'rl': SimpleNamespace(get_fps=lambda: fps),
  'np': np,
  'time': SimpleNamespace(monotonic=lambda: now),
  'threading': SimpleNamespace(Thread=Thread),
  'Callable': Callable,
  'Enum': Enum,
  'Params': Params,
  'messaging': SimpleNamespace(SubMaster=SubMaster, log_from_bytes=log_from_bytes),
  'car': car,
  'log': log,
  'FirstOrderFilter': FirstOrderFilter,
  'TimedSnapshotCache': TimedSnapshotCache,
  'RealtimeUiParamSnapshot': RealtimeUiParamSnapshot,
  'read_realtime_ui_params': read_realtime_ui_params,
  'PrimeState': lambda: SimpleNamespace(start=lambda: None),
  'gui_app': gui,
  'HARDWARE': hardware,
  'PC': scene['pc'],
  'BACKLIGHT_OFFROAD': 65 if scene['mici'] else 50,
  'cloudlog': SimpleNamespace(debug=lambda *args: None),
  'active_usbgpu_compiled_path': lambda: 'fixture' if models['compiled'] else None,
  'usbgpu_compile_pending': lambda: models['compile_pending'],
}
path = root / 'openpilot/selfdrive/ui/ui_state.py'
tree = ast.parse(path.read_text())
classes = [node for node in tree.body if isinstance(node, ast.ClassDef)]
exec(compile(ast.Module(body=classes, type_ignores=[]), str(path), 'exec'), namespace)
state = namespace['UIState']()
namespace['ui_state'] = state
device = namespace['Device']()
namespace['device'] = device
state.add_engaged_transition_callback(lambda: transitions.append('engaged'))
state.add_offroad_transition_callback(lambda: transitions.append('offroad'))
device.add_interactive_timeout_callback(lambda: effects.append({'kind': 'interactive_timeout'}))
state.params.reads.clear()
results = []
for step in scene['frames']:
  frame = step['input']
  now = frame['now']
  fps = frame['fps']
  models = step.get('models', {'compiled': False, 'compile_pending': False})
  worker_busy = step.get('worker_busy', False)
  for key, value in step.get('params', {}).items():
    if value is None:
      state.params.values.pop(key, None)
    else:
      state.params.values[key] = bytes(value)
  state.params.failures = step.get('failures', [])
  if 'override_timeout' in step:
    device.set_override_interactive_timeout(step['override_timeout'])
  if 'offroad_brightness' in step:
    device.set_offroad_brightness(step['offroad_brightness'])
  gui.mouse_events = [SimpleNamespace(left_down=step.get('touch', False))]
  state.sm.frame = frame['frame']
  state.sm.updated = {'pandaStates': frame['panda_updated'], 'wideRoadCameraState': frame['wide_updated'], 'selfdriveState': frame['selfdrive_updated']}
  state.sm.recv_frame = {'pandaStates': frame['panda_receive_frame']}
  state.sm.alive = {'wideRoadCameraState': frame['wide_alive']}
  state.sm.valid = {'wideRoadCameraState': frame['wide_valid']}
  state.sm.values = {
    'pandaStates': [SimpleNamespace(**p) for p in frame['pandas']],
    'wideRoadCameraState': SimpleNamespace(exposureValPercent=frame['exposure_percent']),
    'deviceState': SimpleNamespace(started=frame['device_started']),
    'selfdriveState': SimpleNamespace(enabled=frame['enabled'], state=frame['control_state']),
    'carControl': SimpleNamespace(latActive=frame['lat_active']),
  }
  state.update()
  results.append(
    {
      'status': state.status.value,
      'lat_active': state.lat_active,
      'started_frame': state.started_frame,
      'started_time': state.started_time,
      'started': state.started,
      'ignition': state.ignition,
      'recording_audio': state.recording_audio,
      'panda_type': state.panda_type,
      'light_sensor': state.light_sensor,
      'is_release': state.is_release,
      'is_metric': state.is_metric,
      'always_on_dm': state.always_on_dm,
      'has_longitudinal_control': state.has_longitudinal_control,
      'show_brightness_ratio': state.show_brightness_ratio,
      'show_model_view': state.show_model_view,
      'share_data': state.share_data,
      'show_camera_with_cluster': state.show_camera_with_cluster,
      'usbgpu_compiled': state.usbgpu_compiled,
      'usbgpu_compile_pending': state.usbgpu_compile_pending,
      'param_update_time': state._param_update_time,
      'engaged': state.engaged,
      'awake': device.awake,
      'interaction_time': device._interaction_time,
      'last_brightness': device._last_brightness,
      'brightness_filter': device._brightness_filter.x,
      'brightness_timer': device._brightness_timer,
      'transitions': list(transitions),
      'effects': list(effects),
      'reads': list(state.params.reads),
      'next_refresh_time': state._realtime_params.next_refresh_time,
    }
  )
  effects.clear()
  transitions.clear()
  state.params.reads.clear()
json.dump(results, sys.stdout)
