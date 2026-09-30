#!/usr/bin/env python3
import argparse
import ast
import json
from pathlib import Path
import random
import subprocess
from types import SimpleNamespace


def original(root, commands):
  path = root / 'openpilot/selfdrive/carrot/server/features/dashcam/upload_jobs.py'
  tree = ast.parse(path.read_text())
  tree.body = [n for n in tree.body if isinstance(n, (ast.Assign, ast.AnnAssign, ast.FunctionDef, ast.ClassDef))]
  clock = [1000.0, 100.0]
  identity = ['']
  scope = {
    'Any': object,
    'asyncio': SimpleNamespace(Task=object),
    'time': SimpleNamespace(time=lambda: clock[0], monotonic=lambda: clock[1]),
    'uuid': SimpleNamespace(uuid4=lambda: SimpleNamespace(hex=identity[0])),
  }
  exec(compile(tree, str(path), 'exec'), scope)
  observations = []
  for command in commands:
    clock[:] = command['clock']
    op = command['op']
    job = scope['_jobs'].get(command.get('id'))
    response = None
    try:
      if op == 'create':
        identity[0] = command['id']
        scope['create_job'](command['segments'])
      elif op == 'append':
        scope['append'](job, command.get('text'))
      elif op == 'touch':
        scope['touch'](job)
      elif op == 'progress':
        scope['progress'](job, **command['patch'])
      elif op == 'cancel':
        response = scope['cancel_job'](command['id'])
      elif op == 'finish':
        scope['finish'](job, **command['patch'])
      elif op == 'expire':
        scope['expire_stale_jobs'](command.get('now'))
      elif op == 'partial':
        job['partial_results'] = command['results']
      elif op == 'task_done':
        job['_task'] = SimpleNamespace(done=lambda: True)
      elif op == 'fail':
        scope['fail_running_job'](job, command['error'])
      else:
        raise ValueError(op)
    except ValueError as error:
      response = {'error': str(error)}
    observations.append({'response': response, 'jobs': [scope['snapshot'](j) for j in scope['_jobs'].values()]})
  return observations


def scenarios():
  clock = [1000.0, 100.0]

  def command(op, **fields):
    clock[0] += 0.125
    clock[1] += 0.125
    return {'op': op, 'clock': clock.copy(), **fields}

  commands = [command('create', id='a', segments=['route--0', 'route--1'])]
  commands += [command('append', id='a', text=text) for text in [None, '', 'a', 'b\r\nc\rd', '\nx', '한글' * 31000]]
  for percent in [-10, 0.5, 1.5, 2.5, 2.4, 98.5, 1000]:
    commands.append(command('progress', id='a', patch={'percent': percent, 'bytes_current': -5, 'current': -1}))
  commands.extend(
    [
      command('progress', id='a', patch={'phase': 'NOT_A_PHASE'}),
      command('progress', id='a', patch={'phase': ' UPLOADING ', 'phase_current': 4, 'phase_total': 10}),
      command('cancel', id='absent'),
      command('cancel', id='a'),
      command('cancel', id='a'),
      command('partial', id='a', results=[{'ok': True}, {'ok': False}]),
      command('fail', id='a', error='interrupted'),
      command('cancel', id='a'),
      command('progress', id='a', patch={'percent': 2}),
      command('create', id='b', segments=[]),
      command('task_done', id='b'),
      command('expire'),
      command('create', id='c', segments=['route--3']),
      command('expire', now=1900),
      command('expire', now=10000),
    ]
  )
  for i in range(15):
    commands += [
      command('create', id=f'finish-{i}', segments=[]),
      command('finish', id=f'finish-{i}', patch={'ok': i % 3 == 0, 'status': 'canceled' if i % 3 == 2 else None}),
    ]
  yield commands
  rng = random.Random(61)
  for _ in range(20):
    commands = [command('create', id='random', segments=['r--0'])]
    for _ in range(100):
      patch = {
        rng.choice(['current', 'total', 'phase_current', 'phase_total', 'bytes_current', 'bytes_total', 'bytes_per_second']): rng.randrange(-100, 1000),
        'percent': rng.randrange(-100, 1200) / 10,
      }
      commands.append(command('progress', id='random', patch=patch))
    commands.append(command('finish', id='random', patch={'ok': True}))
    yield commands


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  root = Path(__file__).resolve().parents[2]
  count = 0
  for index, commands in enumerate(scenarios()):
    expected = original(root, commands)
    source = args.output / f'{index:02}-input.json'
    source.write_text(json.dumps(commands, ensure_ascii=False))
    result = subprocess.run([str(args.binary.resolve())], input=source.read_text(), text=True, capture_output=True, check=True)
    actual = json.loads(result.stdout)
    (args.output / f'{index:02}-source.json').write_text(json.dumps(expected, ensure_ascii=False))
    (args.output / f'{index:02}-native.json').write_text(result.stdout)
    for step, (left, right) in enumerate(zip(expected, actual, strict=True)):
      assert left == right, (index, step, commands[step], left, right)
    count += len(commands)
  report = {'passed': True, 'scenarios': index + 1, 'transitions': count}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
