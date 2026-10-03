import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def cases() -> list[dict]:
  result = []

  def add(name: str, probability: str | None = None, interval: str | None = None,
          actions: str = 'stress 0 0\n', expected: list | None = None, error: str | None = None,
          samples: str = '1:0,1:1000,1:2000,1:3000', random: str | None = '0', **extra) -> None:
    environment = {'STRESS_CLOCK': samples, **extra}
    for key, value in [('SPECTRA_ERROR_PROB', probability), ('SPECTRA_ERROR_DT', interval), ('STRESS_RAND', random)]:
      if value is not None:
        environment[key] = value
    result.append(dict(name=name, environment=environment, actions=actions,
                       expected=expected if expected is not None else [], error=error))

  add('constructor-lazy', 'bad', 'bad', actions='')
  add('now-does-not-parse', 'bad', 'bad', actions='now\n', expected=[{'now': 1_000_000_000}])
  add('parse-after-now', 'bad', actions='now\nstress 2 3\n', expected=[{'now': 1_000_000_000}], error='invalid_argument')
  add('defaults', expected=[False])
  for index, value in enumerate(['0', '-0', '-1', '-inf', 'nan', 'NaN(payload)']):
    add(f'disabled-{index}', value, expected=[False])
  add('one-excludes-rand-max', '1', expected=[False], random='2147483647')
  add('unclamped-probability', '1.01', expected=[True], random='2147483647')
  add('infinite-probability', 'inf', expected=[True], random='2147483647')
  add('half-random-boundary', '.5', '0', actions='stress 0 0\nstress 1 1\n',
      expected=[True, False], random='1073741823,1073741824')
  add('strict-millisecond-interval', '1', '1', actions='stress 0 0\nstress 1 1\nstress 2 2\nstress 0 3\n',
      expected=[False, False, True, False], samples='0:999999,0:1000000,0:1000001,0:2000000,0:3000000', random='0,0,0,0')
  add('shared-three-camera-last-trigger', '1', '5', actions='stress 0 0\nstress 1 1\nstress 2 2\nstress 0 3\n',
      expected=[True, False, True, True], samples='0:6000000,0:7000000,0:12000000,0:12000001,0:13000000,0:19000000,0:20000000', random='0,0,0,0')
  for index, (interval, expected) in enumerate([('inf', False), ('nan', False), ('-inf', True), ('-1', True), ('-0', True)]):
    add(f'interval-{index}', '1', interval, expected=[expected])
  add('prefixes', ' \t+0x1.8p-1tail', ' +0.5suffix', expected=[True])
  add('cached-env', '1', '0', actions='stress 1 2\nstress 2 3\n', expected=[True, True], random='0,0',
      STRESS_MUTATE_PROB='invalid', STRESS_MUTATE_DT='invalid')
  add('real-libc-rand-sequence', '0.5', '0', actions='stress 0 0\n'*10, expected=[False, True, False, False, False, True, True, False, True, False],
      samples=','.join(f'{index}:0' for index in range(1, 21)), random=None)
  for variable in ['probability', 'interval']:
    for index, (value, error) in enumerate([('', 'invalid_argument'), ('x', 'invalid_argument'), ('+', 'invalid_argument'),
                                            ('1e9999', 'out_of_range'), ('1e-9999', 'out_of_range')]):
      add(f'{variable}-error-{index}', **{variable: value, 'error': error})
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['reference', 'native', 'parser', 'fixture', 'output']:
    parser.add_argument('--'+name, type=Path, required=True)
  parser.add_argument('--runner', nargs='*', default=[])
  parser.add_argument('--profile', default='host')
  arguments = parser.parse_args()
  arguments.output.mkdir(parents=True, exist_ok=False)
  base = {key: value for key, value in os.environ.items() if not key.startswith(('SPECTRA_ERROR_', 'STRESS_'))}
  reports = []

  def run(name: str, side: str, executable: Path, environment: dict, actions: str = '', args: list | None = None) -> dict:
    prefix = arguments.output / f'{name}-{side}'
    trace = prefix.with_suffix('.jsonl')
    command = [*arguments.runner, str(executable), *(args or [])]
    env = {**base, **environment, 'LD_PRELOAD': str(arguments.fixture), 'STRESS_TRACE': str(trace)}
    result = subprocess.run(command, input=actions, text=True, capture_output=True, env=env, timeout=10)
    prefix.with_suffix('.stdout').write_text(result.stdout)
    prefix.with_suffix('.stderr').write_text(result.stderr)
    record = dict(command=command, environment=environment, actions=actions, exit=result.returncode,
                  result=json.loads(result.stdout) if result.returncode == 0 else None,
                  trace=[json.loads(line) for line in trace.read_text().splitlines()] if trace.exists() else [], stderr=result.stderr)
    prefix.with_suffix('.json').write_text(json.dumps(record, indent=2)+'\n')
    assert result.returncode == 0, (name, side, result.returncode, result.stderr)
    return record

  for case in cases():
    original = run(case['name'], 'source', arguments.reference, case['environment'], case['actions'])
    native = run(case['name'], 'native', arguments.native, case['environment'], case['actions'])
    expected = dict(results=[{'triggered': item} if isinstance(item, bool) else item for item in case['expected']],
                    error=case['error'], error_step=len(case['expected']) if case['error'] else None)
    assert original['result'] == expected, (case['name'], original['result'], expected)
    for key in ['result', 'trace', 'stderr']:
      assert original[key] == native[key], (case['name'], key, original[key], native[key])
    reports.append(dict(name=case['name'], kind='policy', observations=len(original['trace'])))
  for index, value in enumerate(['0', '-0', '  +1.5tail', '0x1.8p-2suffix', '1e+2x', 'inf', '-Infinity', 'nan', '-nan',
                                  'NaN(0x123)', 'NaN(payload)', '', ' ', '+', 'x', '1e9999', '-1e9999', '1e-9999',
                                  '2.2250738585072014e-308', '4.9406564584124654e-324']):
    name = f'parse-{index}'
    original = run(name, 'source', arguments.reference, {}, args=['parse', value])
    native = run(name, 'native', arguments.parser, {}, args=[value])
    assert original['result']['result'] == native['result']['result'], (name, original['result'], native['result'])
    for key in ['trace', 'stderr']:
      assert original[key] == native[key], (name, key, original[key], native[key])
    for record in [original, native]:
      before = record['result']['previous_errno']
      assert before != 0, (name, record['result'])
      expected_errno = record['trace'][0]['errno'] or before
      assert record['result']['after_errno'] == expected_errno, (name, record['result'], expected_errno)
    reports.append(dict(name=name, input=value, kind='parser', observations=len(original['trace']),
                        initial_errno=dict(source=original['result']['previous_errno'], native=native['result']['previous_errno'])))
  before = time.clock_gettime_ns(time.CLOCK_BOOTTIME)
  actual = subprocess.run([*arguments.runner, str(arguments.native), 'real'], capture_output=True, text=True, check=True, timeout=10)
  after = time.clock_gettime_ns(time.CLOCK_BOOTTIME)
  clock = json.loads(actual.stdout)['now']
  assert before <= clock <= after, (before, clock, after)
  files = [arguments.reference, arguments.native, arguments.parser, arguments.fixture]
  report = dict(profile=arguments.profile, status='PASS', cases=reports, real_boottime=dict(before=before, value=clock, after=after),
                executables={str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in files})
  (arguments.output/'report.json').write_text(json.dumps(report, indent=2)+'\n')
  print(json.dumps(dict(profile=arguments.profile, status='PASS', cases=len(reports), observations=sum(case['observations'] for case in reports))))


if __name__ == '__main__':
  main()
