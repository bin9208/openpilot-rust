import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

from check_camerad_stress import cases


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['reference', 'native', 'parser', 'fixture', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--qemu', type=Path)
  parser.add_argument('--sysroot', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  base = {key: value for key, value in os.environ.items() if not key.startswith(('SPECTRA_ERROR_', 'STRESS_', 'LD_PRELOAD'))}
  reports = []

  def command(executable: Path, extra: list, preload: bool) -> list:
    prefix = []
    if args.qemu:
      assert args.sysroot
      prefix = [str(args.qemu), '-L', str(args.sysroot)]
      if preload:
        prefix += ['-E', 'LD_PRELOAD=' + str(args.fixture)]
    return [*prefix, str(executable), *extra]

  def run(name: str, side: str, executable: Path, environment: dict, actions: str = '', extra: list | None = None) -> dict:
    prefix = args.output / f'{name}-{side}'
    trace = prefix.with_suffix('.jsonl')
    env = {**base, **environment, 'STRESS_TRACE': str(trace)}
    if not args.qemu:
      env['LD_PRELOAD'] = str(args.fixture)
    cmd = command(executable, extra or [], True)
    result = subprocess.run(cmd, input=actions, text=True, capture_output=True, env=env, timeout=10)
    prefix.with_suffix('.stdout').write_text(result.stdout)
    prefix.with_suffix('.stderr').write_text(result.stderr)
    record = {
      'command': cmd,
      'environment': environment,
      'actions': actions,
      'exit': result.returncode,
      'result': json.loads(result.stdout) if result.returncode == 0 else None,
      'trace': [json.loads(line) for line in trace.read_text().splitlines()] if trace.exists() else [],
      'stderr': result.stderr,
    }
    prefix.with_suffix('.json').write_text(json.dumps(record, indent=2) + '\n')
    assert result.returncode == 0, (name, side, result.returncode, result.stderr)
    return record

  for case in cases():
    source = run(case['name'], 'source', args.reference, case['environment'], case['actions'])
    native = run(case['name'], 'native', args.native, case['environment'], case['actions'])
    expected = {
      'results': [{'triggered': value} if isinstance(value, bool) else value for value in case['expected']],
      'error': case['error'],
      'error_step': len(case['expected']) if case['error'] else None,
    }
    assert source['result'] == expected, (case['name'], source['result'], expected)
    for key in ['result', 'trace']:
      assert source[key] == native[key], (case['name'], key, source[key], native[key])
    assert native['stderr'] == '', (case['name'], native['stderr'])
    reports.append({'name': case['name'], 'kind': 'policy', 'observations': len(source['trace'])})
  values = [
    '0',
    '-0',
    '  +1.5tail',
    '0x1.8p-2suffix',
    '1e+2x',
    'inf',
    '-Infinity',
    'nan',
    '-nan',
    'NaN(0x123)',
    'NaN(payload)',
    '',
    ' ',
    '+',
    'x',
    '1e9999',
    '-1e9999',
    '1e-9999',
    '2.2250738585072014e-308',
    '4.9406564584124654e-324',
  ]
  for index, value in enumerate(values):
    name = f'parse-{index}'
    source = run(name, 'source', args.reference, {}, extra=['parse', value])
    native = run(name, 'native', args.parser, {}, extra=[value])
    assert source['result']['result'] == native['result']['result'], (name, source['result'], native['result'])
    for key in ['trace', 'stderr']:
      assert source[key] == native[key], (name, key, source[key], native[key])
    for record in [source, native]:
      before = record['result']['previous_errno']
      assert before != 0
      assert record['result']['after_errno'] == (record['trace'][0]['errno'] or before)
    reports.append(
      {
        'name': name,
        'input': value,
        'kind': 'parser',
        'observations': len(source['trace']),
        'initial_errno': {'source': source['result']['previous_errno'], 'native': native['result']['previous_errno']},
      }
    )
  before = time.clock_gettime_ns(time.CLOCK_BOOTTIME)
  real = subprocess.run(command(args.native, ['real'], False), capture_output=True, text=True, check=True, env=base, timeout=10)
  after = time.clock_gettime_ns(time.CLOCK_BOOTTIME)
  value = json.loads(real.stdout)['now']
  assert before <= value <= after
  report = {
    'status': 'PASS',
    'profile': 'arm' if args.qemu else 'host',
    'cases': reports,
    'native_log_sink': 'uninitialized; actual binary logging validated separately',
    'real_boottime': {'before': before, 'value': value, 'after': after},
    'executables': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in [args.reference, args.native, args.parser, args.fixture]},
  }
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'status': 'PASS', 'cases': len(reports), 'observations': sum(case['observations'] for case in reports)}))


if __name__ == '__main__':
  main()
