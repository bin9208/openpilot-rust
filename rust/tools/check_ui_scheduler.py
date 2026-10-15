"""Unchanged DisplayScheduler source versus native ordered thread/child policy effects."""

import argparse
import ast
import copy
import json
import hashlib
from pathlib import Path
import subprocess
from types import SimpleNamespace

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path)
parser.add_argument('--runner', type=Path)
parser.add_argument('--reference-only', action='store_true')
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
manifest = json.loads((root / 'rust/upstream/display-scheduling-source.json').read_text())
assert manifest['source_commit'] == 'e8e152dbcfd9574a1791ed12026d1880e87689b3'
item = manifest['files']['openpilot/common/display_scheduling.py']
data = item['content'].encode()
assert hashlib.sha256(data).hexdigest() == item['sha256']
assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == item['git_blob']
source = ast.parse(data)
cls = next(n for n in source.body if isinstance(n, ast.ClassDef))
workers = {str(i): {'policy': 1, 'nice': 0, 'cores': [0]} for i in [1, 2, 3]}
steps = []
for i in range(80):
  steps.append(
    {
      'now': i / 10,
      'onroad': 10 <= i < 50,
      'force': i in [21, 22],
      'online': not 30 <= i < 40,
      'online_cores': [4] if 30 <= i < 35 else [6] if 35 <= i < 40 else [7] if 40 <= i < 45 else [4, 6, 7],
      'child': i % 2 == 0,
      'gone': 2 if i == 20 else None,
      'fail_affinity': 1 if i == 10 else None,
      'deny_restore': 30 <= i < 60,
    }
  )


def run_scenario(enabled, core, include_little, extra, name, device_type=None):
  scene = {'enabled': enabled, 'core': core, 'include_little': include_little, 'extra_onroad_cores': extra, 'device_type': device_type, 'workers': copy.deepcopy(workers), 'steps': steps}
  current = copy.deepcopy(workers)
  calls = []
  now = 0
  step = {}

  def worker(tid):
    if step.get('gone') == tid:
      raise ProcessLookupError(3, 'gone')
    return current[str(tid)]

  def policy(tid):
    calls.append(['policy', tid])
    return worker(tid)['policy']

  def other(tid, *args):
    calls.append(['other', tid])
    worker(tid)['policy'] = 0

  def priority(_, tid):
    calls.append(['priority', tid])
    return worker(tid)['nice']

  def nice(_, tid, value):
    calls.append(['nice', tid, value])
    if step.get('deny_restore') and value == 0:
      raise PermissionError(13, 'denied')
    worker(tid)['nice'] = value

  def affinity(tid):
    calls.append(['affinity', tid])
    return set(worker(tid)['cores'])

  def set_affinity(tid, cores):
    calls.append(['set_affinity', tid, sorted(cores)])
    if step.get('fail_affinity') == tid:
      step['fail_affinity'] = None
      raise OSError(22, 'offline')
    worker(tid)['cores'] = sorted(cores)

  def online(core):
    calls.append(['online', core])
    return core in step['online_cores']

  def threads(pid=None):
    calls.append(['threads', pid])
    return [1, 2] if pid is None else [3]

  namespace = {
    'sys': SimpleNamespace(platform='linux'),
    'time': SimpleNamespace(monotonic=lambda: now),
    'LITTLE_CORES': {0, 1, 2, 3},
    'DISPLAY_NICE': 19,
    'core_online': online,
    'thread_ids': threads,
    'os': SimpleNamespace(
      sched_getscheduler=policy,
      sched_setscheduler=other,
      SCHED_OTHER=0,
      sched_param=lambda n: n,
      getpriority=priority,
      setpriority=nice,
      PRIO_PROCESS=0,
      sched_getaffinity=affinity,
      sched_setaffinity=set_affinity,
    ),
  }
  exec(compile(ast.Module(body=[cls], type_ignores=[]), 'display_scheduling.py', 'exec'), namespace)
  scheduler = namespace['DisplayScheduler'](core, enabled=enabled, include_little=include_little, extra_onroad_cores=tuple(extra))
  if device_type is not None:
    ui_item = manifest['files']['openpilot/selfdrive/ui/ui.py']
    ui_data = ui_item['content'].encode()
    assert hashlib.sha256(ui_data).hexdigest() == ui_item['sha256']
    ui_main = next(node for node in ast.parse(ui_data).body if isinstance(node, ast.FunctionDef) and node.name == 'main')
    assert [ast.unparse(node.targets[0]) for node in ui_main.body[:2]] == ['c3', 'scheduler']
    namespace.update(TICI=enabled, HARDWARE=SimpleNamespace(get_device_type=lambda: device_type))
    exec(compile(ast.Module(body=ui_main.body[:2], type_ignores=[]), 'final ui.py scheduler construction', 'exec'), namespace)
    scheduler = namespace['scheduler']

  expected = []
  for original in steps:
    step = dict(original)
    now = step['now']
    scheduler.update(step['onroad'], force=step['force'], child_pid=123 if step['child'] else None)
    expected.append(copy.deepcopy({'onroad': scheduler.onroad, 'next_check': scheduler.next_check, 'workers': current, 'calls': calls}))
    calls.clear()
  (args.output / f'{name}-input.json').write_text(json.dumps(scene))
  (args.output / f'{name}-source.json').write_text(json.dumps(expected))
  if args.reference_only: return
  assert args.binary is not None
  command = ([str(args.runner)] if args.runner else []) + [str(args.binary)]
  result = subprocess.run(command, input=json.dumps(scene), text=True, capture_output=True, check=True)
  (args.output / f'{name}-native.json').write_text(result.stdout)
  (args.output / f'{name}-native.log').write_text(result.stderr)
  actual = json.loads(result.stdout)
  for i, (a, b) in enumerate(zip(expected, actual, strict=True)):
    assert a == b, (name, i, a, b)


scenarios = [(False, 6, False, [], 'disabled'), (True, 6, False, [], 'c4'),
             (True, 6, True, [7], 'c3'), (True, 4, False, [], 'cluster'),
             (True, 6, True, [6, 7, 7], 'duplicates'),
             *[(True, 6, False, [], 'ui-' + device, device) for device in ['tici', 'tizi', 'mici', 'pc', 'unknown']],
             (False, 6, False, [], 'ui-disabled', 'tici')]
for options in scenarios:
  run_scenario(*options)

receipt = dict(source_commit=manifest['source_commit'], scenarios=len(scenarios), steps=len(scenarios)*len(steps),
               exact_ordered_effects=not args.reference_only, native_compared=not args.reference_only)
(args.output / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
