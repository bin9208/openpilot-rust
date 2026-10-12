from __future__ import annotations

import ast
from dataclasses import asdict
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace

def original(root: Path):
  sys.path.insert(0, str(root))
  from check_message_state import source as messaging_source

  sys.path.insert(0, str(root / 'openpilot/selfdrive/carrot/cluster'))
  from cluster_models import ClusterAlert
  from cluster_utils import clamp

  path = root / 'openpilot/selfdrive/carrot/cluster/cluster_live.py'
  tree = ast.parse(path.read_text())
  names = {
    '_live_cluster_alert',
    'onroad_state',
    'status_text',
    'screen_brightness_percent',
    '_camera_brightness_percent',
    '_service_data',
    '_service_receive_time',
    '_service_alive',
    '_service_updated',
    '_service_valid',
    '_profile_start',
    '_profile_add',
  }
  source_class = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'OpenpilotLiveSource')
  selected = ast.ClassDef(
    name=source_class.name,
    bases=[],
    keywords=[],
    body=[node for node in source_class.body if isinstance(node, ast.FunctionDef) and node.name in names],
    decorator_list=[],
  )
  route = ast.parse((root / 'openpilot/selfdrive/carrot/cluster/cluster_route_replay.py').read_text())
  safe = next(node for node in route.body if isinstance(node, ast.FunctionDef) and node.name == 'safe_get')
  now = [10.0]
  clock = SimpleNamespace(monotonic=lambda: now[0])
  scope = {
    'Any': object,
    'ClusterAlert': ClusterAlert,
    'time': clock,
    'math': math,
    'clamp': clamp,
    'SELFDRIVE_STATE_TIMEOUT_SECONDS': 5.0,
    'SELFDRIVE_UNRESPONSIVE_TIMEOUT_SECONDS': 10.0,
  }
  module = ast.fix_missing_locations(ast.Module(body=[safe, selected], type_ignores=[]))
  exec(compile(module, str(path), 'exec'), scope)
  messaging, _ = messaging_source()
  owner = scope['OpenpilotLiveSource'].__new__(scope['OpenpilotLiveSource'])
  owner.profile_enabled = False
  owner._profile_samples = []
  owner.services = [
    'carState',
    'carParams',
    'modelV2',
    'radarState',
    'liveTracks',
    'longitudinalPlan',
    'lateralPlan',
    'controlsState',
    'selfdriveState',
    'carControl',
    'carOutput',
    'deviceState',
    'roadCameraState',
    'cameraOdometry',
    'liveCalibration',
    'livePose',
    'drivingModelData',
    'liveDelay',
    'liveParameters',
    'liveTorqueParameters',
    'navInstruction',
    'navInstructionCarrot',
    'navRoute',
    'carrotMan',
    'carrotNavi',
    'wideRoadCameraState',
  ]
  owner.sm = messaging['SubMaster'](owner.services)
  owner.start_t = 10.0
  owner.frames = 0
  owner.last_state = None
  return owner, messaging['new_message'], now, ClusterAlert


def run(root: Path, native: Path, output: Path) -> None:
  output.mkdir(parents=True)
  owner, message, clock, alert_type = original(root)
  steps, expected = [], []

  def step(now: float, packets: list, onroad: bool, current=None) -> None:
    clock[0] = now
    paths = []
    for index, packet in enumerate(packets):
      path = output / f'{len(steps):04}-{index}.capnp'
      path.write_bytes(packet.to_bytes())
      paths.append(str(path))
    owner.sm.update_msgs(now, [packet.as_reader() for packet in packets])
    current_alert = alert_type(**current) if current is not None else None
    alert = owner._live_cluster_alert(current_alert, onroad)
    expected.append(
      {
        'onroad': owner.onroad_state(),
        'brightness': owner.screen_brightness_percent(),
        'alert': None if alert is None else asdict(alert),
        'status': owner.status_text(),
      }
    )
    steps.append({'now': now, 'messages': paths, 'onroad': onroad, 'current': current, 'frames': owner.frames, 'radar': 0, 'detected': 0})

  step(10.0, [], True)
  for now in [15.0, 15.001]:
    step(now, [], True)
  drive = message('selfdriveState')
  drive.selfdriveState.enabled = True
  step(16.0, [drive], True)
  current = {'text1': 'Original alert', 'text2': 'Retained', 'size': 1, 'status': 1, 'alert_type': 'owned-original'}
  for now in [21.0, 21.001, 30.999, 31.0]:
    step(now, [], True, current)
  step(32.0, [], False, current)
  step(40.0, [], True)
  step(45.001, [], True)
  for index, exposure in enumerate([float('nan'), float('inf'), -10.0, 92.0] + [value / 4 for value in range(441)]):
    device = message('deviceState')
    device.deviceState.started = index % 2 == 0
    device.deviceState.screenBrightnessPercent = 72
    wide = message('wideRoadCameraState')
    wide.wideRoadCameraState.exposureValPercent = exposure
    wide.valid = index % 5 != 0
    step(50.0 + index * 0.125, [device, wide], False)
  case = {'include_can': False, 'started': 10.0, 'steps': steps}
  case_file = output / 'input.json'
  case_file.write_text(json.dumps(case, allow_nan=False))
  with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-cluster-live-', dir='/dev/shm') as namespace:
    prefix = Path(namespace).name.removeprefix('msgq_')
    command = [str(native), str(case_file)]
    env = {**os.environ, 'OPENPILOT_PREFIX': prefix}
    completed = subprocess.run(command, env=env, capture_output=True, text=True, timeout=30)
    (output / 'invocation.json').write_text(
      json.dumps({'command': command, 'prefix': prefix, 'exit_code': completed.returncode, 'stdout': completed.stdout, 'stderr': completed.stderr}, indent=2)
    )
  assert completed.returncode == 0, completed.stderr
  actual = json.loads(completed.stdout)
  (output / 'expected.json').write_text(json.dumps(expected, indent=2))
  (output / 'actual.json').write_text(json.dumps(actual, indent=2))
  mismatches = [{'step': index, 'expected': left, 'actual': right} for index, (left, right) in enumerate(zip(expected, actual, strict=True)) if left != right]
  result = {
    'steps': len(steps),
    'pass': not mismatches,
    'mismatches': mismatches[:10],
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    'scope': 'original status/brightness/alert methods and complete Cereal SubMaster state; full ClusterUiState projection remains pending',
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2))
  print(json.dumps(result))
  assert not mismatches


if __name__ == '__main__':
  run(*(Path(argument).resolve() for argument in sys.argv[1:]))
