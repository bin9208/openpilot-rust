"""Run the original FPS monitor method at its warning and strict-mode boundaries."""

import argparse
import ast
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
source = root / 'openpilot/system/ui/lib/application.py'
tree = ast.parse(source.read_text())
gui = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'GuiApplication')
method = next(node for node in gui.body if isinstance(node, ast.FunctionDef) and node.name == '_monitor_fps')
constants = [
  node
  for node in tree.body
  if isinstance(node, ast.Assign)
  and any(isinstance(target, ast.Name) and target.id in ['FPS_LOG_INTERVAL', 'FPS_DROP_THRESHOLD', 'FPS_CRITICAL_THRESHOLD'] for target in node.targets)
]


class Exit(Exception):
  pass


def stop(code):
  assert code == 1
  raise Exit


messages = []
state = SimpleNamespace(_target_fps=20, _last_fps_log_time=0.0, close_ffmpeg=lambda: None)
context = {
  'rl': SimpleNamespace(get_fps=lambda: frame['fps']),
  'time': SimpleNamespace(monotonic=lambda: frame['now']),
  'cloudlog': SimpleNamespace(warning=lambda value: messages.append(value), error=lambda value: None),
  'os': SimpleNamespace(_exit=stop),
}
exec(compile(ast.Module(body=constants + [method], type_ignores=[]), str(source), 'exec'), context)
samples = []
for second in [0.0, 4.999, 5.0, 5.001, 9.999, 10.0, 15.0, 20.0]:
  for fps in [0, 9, 10, 17, 18, 20, 30]:
    samples.append({'now': second, 'fps': fps, 'strict': False})
    samples.append({'now': second, 'fps': fps, 'strict': True})
expected = []
for frame in samples:
  context['STRICT_MODE'] = frame['strict']
  before = len(messages)
  critical = False
  try:
    context['_monitor_fps'](state)
  except Exit:
    critical = True
  expected.append({'warning': len(messages) > before, 'critical': critical, 'last_log': state._last_fps_log_time})
actual = json.loads(subprocess.check_output([str(args.binary)], input=json.dumps(samples), text=True))
(args.output / 'input.json').write_text(json.dumps(samples, indent=2))
(args.output / 'source.json').write_text(json.dumps(expected, indent=2))
(args.output / 'native.json').write_text(json.dumps(actual, indent=2))
assert actual == expected
print(f'PASS: {len(samples)} unchanged-source/native FPS warning throttle and strict-mode threshold decisions')
