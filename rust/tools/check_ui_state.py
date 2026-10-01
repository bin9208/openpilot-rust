"""Direct source/native UIState + Device traces; no display or hardware mutation."""

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
env = dict(os.environ, PYTHONPATH=str(root))
sys.path.insert(0, str(root))
from openpilot.cereal import car

cp = car.CarParams.new_message(alphaLongitudinalAvailable=True, openpilotLongitudinalControl=False).to_bytes()
base = {
  'frame': 0,
  'now': 0.0,
  'fps': 20,
  'panda_updated': True,
  'panda_receive_frame': 0,
  'pandas': [{'pandaType': 1, 'ignitionLine': False, 'ignitionCan': False}],
  'wide_updated': True,
  'wide_alive': True,
  'wide_valid': True,
  'exposure_percent': 12.0,
  'device_started': False,
  'selfdrive_updated': True,
  'enabled': False,
  'control_state': 'disabled',
  'lat_active': False,
}
results = []
for name, big, pc, mici in [('big', True, False, False), ('mici', False, False, True), ('pc', True, True, False)]:
  frames = []
  for i in range(800):
    frame = copy.deepcopy(base)
    frame.update(frame=i + 1, now=i / 10.0, panda_receive_frame=i + 1)
    onroad = 80 <= i < 510
    frame['pandas'][0]['ignitionLine'] = onroad
    frame['device_started'] = onroad
    frame['enabled'] = onroad and i % 90 < 70
    frame['control_state'] = ['disabled', 'enabled', 'preEnabled', 'overriding', 'softDisabling'][i // 20 % 5]
    frame['lat_active'] = i % 3 == 0
    frame['exposure_percent'] = [0, 8, 15, 91, 92, 99, 120][i // 25 % 7]
    if 170 < i < 300:
      frame.update(panda_updated=False, panda_receive_frame=170)
    if 310 <= i < 320:
      frame['pandas'] = []
    if i == 330:
      frame['pandas'][0]['pandaType'] = 0
    if 400 <= i < 430:
      frame.update(wide_updated=False, wide_alive=i % 2 == 0, wide_valid=i % 3 == 0)
    step = {'input': frame, 'touch': i in [60, 90, 560, 710], 'worker_busy': i > 0 and i % 7 == 0}
    if i in [100, 250, 380]:
      step['params'] = {
        key: list(value.encode())
        for key, value in {
          'IsMetric': str(i != 250),
          'RecordAudio': '1',
          'AlwaysOnDM': '1',
          'ShowCustomBrightness': '0' if i == 380 else '70',
          'ShowCameraWithCluster': '1',
          'ShareData': '1',
        }.items()
      }
      step['params']['IsMetric'] = list(b'0' if i == 250 else b'1')
    if i == 200:
      step['params'] = {'CarParamsPersistent': list(cp), 'AlphaLongitudinalEnabled': list(b'1')}
    if i == 500:
      step['params'] = {'CarParamsPersistent': None, 'AlphaLongitudinalEnabled': list(b'0')}
    if 150 <= i < 164:
      step['failures'] = ['RecordAudio']
    if i == 600:
      step['override_timeout'] = 3
    step['models'] = {'compiled': i > 400, 'compile_pending': i < 400}
    frames.append(step)
  scene = {'big': big, 'pc': pc, 'mici': mici, 'params': {'ShowCustomBrightness': list(b'100')}, 'frames': frames}
  (args.output / f'{name}-input.json').write_text(json.dumps(scene))
  outputs = []
  for lane, command in [('source', [sys.executable, str(root / 'rust/tools/ui_application_qa/state_source.py')]), ('native', [str(args.binary)])]:
    result = subprocess.run(command, input=json.dumps(scene), env=env, text=True, capture_output=True)
    (args.output / f'{name}-{lane}.json').write_text(result.stdout or '{}')
    (args.output / f'{name}-{lane}.log').write_text(result.stderr or '(no stderr)\n')
    assert result.returncode == 0, (lane, result.stderr)
    outputs.append(json.loads(result.stdout))
  expected, actual = outputs
  for index, (source, native) in enumerate(zip(expected, actual, strict=True)):
    state, device = native['state'], native['device']
    flat = {**state, **state['slow'], **device, **native, 'next_refresh_time': state['realtime']['next_refresh_time'], **state['realtime']['value']}
    for key, value in source.items():
      observed = flat[key]
      # Brightness is f64 source arithmetic; discrete rounded values and action order remain exact.
      if isinstance(value, float):
        assert abs(value - observed) <= 1e-11, (name, index, key, value, observed)
      else:
        assert value == observed, (name, index, key, value, observed)
  results.append({'viewport': name, 'frames': len(expected), 'exact_discrete': True, 'float_abs_bound': 1e-11})
(args.output / 'results.json').write_text(
  json.dumps({'scenarios': results, 'source_sha256': hashlib.sha256((root / 'openpilot/selfdrive/ui/ui_state.py').read_bytes()).hexdigest()}, indent=2)
)
print('PASS: 2400 actual-source/native UI state, cached Params reads, transition ordering, display timeout and brightness frames')
