"""Unchanged DisplayScheduler source versus native ordered thread/child policy effects."""

import argparse
import ast
import copy
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
source = ast.parse((root / 'openpilot/common/display_scheduling.py').read_text())
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
      'child': i % 2 == 0,
      'gone': 2 if i == 20 else None,
      'fail_affinity': 1 if i == 10 else None,
      'deny_restore': 30 <= i < 60,
    }
  )


def run_scenario(enabled):
  scene = {'enabled': enabled, 'workers': copy.deepcopy(workers), 'steps': steps}
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
    return step['online']

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
  scheduler = namespace['DisplayScheduler'](6, enabled=enabled)
  expected = []
  for original in steps:
    step = dict(original)
    now = step['now']
    scheduler.update(step['onroad'], force=step['force'], child_pid=123 if step['child'] else None)
    expected.append(copy.deepcopy({'onroad': scheduler.onroad, 'next_check': scheduler.next_check, 'workers': current, 'calls': calls}))
    calls.clear()
  name = 'enabled' if enabled else 'disabled'
  (args.output / f'{name}-input.json').write_text(json.dumps(scene))
  (args.output / f'{name}-source.json').write_text(json.dumps(expected))
  result = subprocess.run([str(args.binary)], input=json.dumps(scene), text=True, capture_output=True, check=True)
  (args.output / f'{name}-native.json').write_text(result.stdout)
  (args.output / f'{name}-native.log').write_text(result.stderr)
  actual = json.loads(result.stdout)
  for i, (a, b) in enumerate(zip(expected, actual, strict=True)):
    assert a == b, (name, i, a, b)


for enabled in [False, True]:
  run_scenario(enabled)

(args.output / 'result.json').write_text(json.dumps({'scenarios': 2, 'steps': 160, 'exact_ordered_effects': True, 'native_own_process_policy': 0}, indent=2))
print(
  'PASS: 160 unchanged-source/native display sweeps; realtime downgrade/nice/affinity order,',
  'hotplug fallback, child threads, exited worker and permission handling; native own-process SCHED_OTHER readback',
)
