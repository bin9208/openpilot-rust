from __future__ import annotations

import ast
from dataclasses import asdict
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace


def original(root: Path):
  sys.path.insert(0, str(root))
  sys.path.insert(0, str(root / 'openpilot/selfdrive/carrot/cluster'))
  from check_message_state import source

  messaging, _ = source()
  path = root / 'openpilot/selfdrive/carrot/cluster/cluster_live.py'
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if not isinstance(node, ast.ImportFrom) or node.module != 'cluster_navi_source']
  cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'OpenpilotLiveSource')
  init = next(node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name == '__init__')
  init.body = [node for node in init.body if not isinstance(node, ast.Try)]
  clock = [10.0]
  scope = {'__file__': str(path), 'messaging': SimpleNamespace(**messaging)}
  exec(compile(ast.fix_missing_locations(tree), str(path), 'exec'), scope)
  scope['time'] = SimpleNamespace(monotonic=lambda: clock[0])
  owner = scope['OpenpilotLiveSource']()
  from openpilot.cereal import log

  owner.log = log
  owner._carrot_navi_media = None
  owner.sm.update = lambda *_: None
  values = {}

  class Params:
    def get(self, key):
      value = values.get(key)
      return value.encode() if value is not None else None

    def get_float(self, key):
      return float(values.get(key) or 0.0)

    def get_int(self, key):
      return int(values.get(key) or 0)

    def get_bool(self, key):
      return values.get(key) == '1'

  owner.params = Params()
  owner.params_memory = None
  return owner, messaging['new_message'], clock, values


def run(root: Path, binary: Path, output: Path) -> None:
  output.mkdir(parents=True, exist_ok=False)
  owner, new_message, clock, params = original(root)
  steps, expected = [], []

  def packet(service, fields, now, valid=True):
    event = new_message(service)
    event.valid = valid
    event.logMonoTime = int(now * 1e9)
    getattr(event, service).from_dict(fields)
    return event

  def step(now, messages=(), updates=None, panels=(False, False, False), hud=0):
    clock[0] = now
    updates = updates or {}
    params.update(updates)
    paths = []
    for index, event in enumerate(messages):
      path = output / f'{len(steps):04}-{index}-{event.which()}.capnp'
      path.write_bytes(event.to_bytes())
      paths.append(str(path))
    owner.set_debug_panels_enabled(live_debug=panels[0], debug_plot=panels[1], navi_debug=panels[2])
    owner.set_hud_debug_mode(hud)
    owner.sm.update_msgs(now, [event.as_reader() for event in messages])
    expected.append(json.loads(json.dumps(asdict(owner.update()))))
    steps.append({'now': now, 'messages': paths, 'params': updates, 'panels': panels, 'hud': hud})

  step(10.0)
  car = {'vEgo': 20.0, 'vEgoCluster': 21.0, 'vCruiseCluster': 100.5, 'cruiseState': {'available': True}, 'fuelGauge': 0.5}
  step(10.1, [packet('carState', car, 10.1), packet('deviceState', {'started': True}, 10.1), packet('selfdriveState', {'enabled': True}, 10.1)])
  debug = {
    'title': 'Signal',
    'lines': ['turn right', True, {'nested': [1, None]}, ' '],
    'speedLimitKph': '５０.9',
    'receivedMono': 10.2,
    'trafficLight': {'redS': 9, 'straightS': 3, 'redOn': True},
  }
  image = {'imageBase64': 'YWJj', 'imageHash': 'frame', 'imageWidth': 24, 'imageHeight': 16}
  step(10.2, updates={'CarrotNaviDebug': json.dumps(debug), 'CarrotNaviImage': json.dumps(image), 'UsbGpuActive': '1'}, panels=(False, False, True))
  step(10.9, updates={'CarrotNaviDebug': None}, panels=(False, False, True))
  step(12.0, panels=(False, False, True))
  step(20.2, panels=(False, False, True))
  step(20.201, panels=(False, False, True))
  for index, value in enumerate([None, True, False, -1, 301, '2_5.9', '١٢', 'nan', [], {}]):
    debug.update(speedLimitKph=value, receivedMono=21 + index, trafficLight={'redS': value, 'redOn': value})
    step(21 + index, updates={'CarrotNaviDebug': json.dumps(debug)}, panels=(False, False, True))
  for index, fields in enumerate(
    [{'imageWidth': 2049}, {'imageHeight': 10001}, {'imageHash': '', 'imageBase64': ''}, {'imageHash': 'Z' * 80, 'imageMime': 'M' * 90}]
  ):
    image.update(fields)
    step(40 + index, updates={'CarrotNaviImage': json.dumps(image)}, hud=3)
  route = {'coordinates': [{'latitude': 37 + index * 0.0001, 'longitude': 127.0} for index in range(8)]}
  man = {'xPosLat': 37.0, 'xPosLon': 127.0, 'xPosAngle': 0.0, 'remote': 'remote'}
  step(50.0, [packet('navRoute', route, 50), packet('carrotMan', man, 50)], panels=(False, False, True))
  step(50.1, panels=(False, False, True))
  step(50.2, [packet('carrotMan', {**man, 'xPosAngle': 360.0}, 50.2)], panels=(False, False, True))
  step(50.3, [packet('carrotMan', {**man, 'xPosLat': 0.0}, 50.3)], panels=(False, False, True))
  for index in range(1, 10):
    now = 60.0 + index
    messages = [
      packet('carState', {**car, 'aEgo': 1.5}, now),
      packet('longitudinalPlan', {'speeds': [23.0], 'accels': [1.0], 'cruiseTarget': 110.0, 'myDrivingMode': 3}, now),
    ]
    step(
      now,
      messages,
      {'ShowPlotMode': str(index), 'CustomSR': '150', 'SteerActuatorDelay': '30', 'LiveParameters': '{"steerRatio": "１２.5", "unused": NaN}'},
      panels=(True, True, False),
    )
  for index, kind in enumerate(['angleState', 'torqueState', 'pidState']):
    now = 80.0 + index
    step(
      now,
      [
        packet('controlsState', {'lateralControlState': {kind: {}}, 'desiredCurvature': 0.01}, now),
        packet('carOutput', {'actuatorsOutput': {'torque': 0.6, 'torqueOutputCan': -4096}}, now),
        packet('carControl', {'latActive': True}, now),
      ],
    )
  case_file = output / 'input.json'
  case_file.write_text(json.dumps({'started': 10.0, 'steps': steps}))
  with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-cluster-input-', dir='/dev/shm') as namespace:
    command = [str(binary), str(case_file)]
    result = subprocess.run(
      command, env={**os.environ, 'OPENPILOT_PREFIX': Path(namespace).name.removeprefix('msgq_')}, capture_output=True, text=True, timeout=30
    )
  (output / 'invocation.json').write_text(
    json.dumps({'command': command, 'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr}, indent=2)
  )
  assert result.returncode == 0, result.stderr
  actual = json.loads(result.stdout)
  (output / 'expected.json').write_text(json.dumps(expected, indent=2))
  (output / 'actual.json').write_text(json.dumps(actual, indent=2))
  from check_message_state import compare

  mismatches = []
  for index, (left, right) in enumerate(zip(expected, actual, strict=True)):
    try:
      compare(left, right)
    except (AssertionError, TypeError) as error:
      mismatches.append({'step': index, 'difference': str(error)})
  receipt = {
    'steps': len(steps),
    'pass': not mismatches,
    'mismatches': mismatches,
    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'scope': 'Original full live update with native Cereal inputs; Params/debug/route/HUD; media disconnected explicitly',
  }
  (output / 'result.json').write_text(json.dumps(receipt, indent=2))
  print(json.dumps(receipt))
  assert not mismatches


if __name__ == '__main__':
  run(*(Path(value).resolve() for value in sys.argv[1:]))
