"""Native updater identity binding with owned children and real managerState fixtures."""

import argparse
from contextlib import contextmanager
import json
from pathlib import Path
import select
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output = args.output.resolve()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
free = shutil.disk_usage(args.output).free
(args.output / 'disk.json').write_text(json.dumps({'free_bytes': free, 'reserve_bytes': 25 * 1024**3, 'estimated_growth_bytes': 1024**2}))
assert free >= 25 * 1024**3 + 1024**2
expected = args.output / 'openpilot-updated'
compile_command = ['cc', '-O2', '-Wall', '-Wextra', '-Werror', str(root / 'rust/tools/ui_application_qa/updater_target.c'), '-o', str(expected)]
with (args.output / 'build.log').open('w') as log:
  log.write(json.dumps({'command': compile_command}) + '\n')
  log.flush()
  subprocess.run(compile_command, stdout=log, stderr=subprocess.STDOUT, check=True)
  log.write('exit_code=0\n')
other = args.output / 'other-worker'
shutil.copyfile(expected, other)
other.chmod(0o755)
records = []


def read_line(process):
  assert select.select([process.stdout], [], [], 5)[0], f'no child response from {process.pid}'
  return process.stdout.readline().strip()


@contextmanager
def target(executable, alternative=None):
  command = [str(executable)] + ([str(alternative)] if alternative else [])
  process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
  try:
    assert read_line(process) == 'ready'
    yield process
  finally:
    if process.poll() is None:
      process.terminate()
      try:
        process.wait(timeout=5)
      except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)
    process.stdin.close()
    process.stdout.close()
    process.stderr.close()


def fixture(process, request='Check', legacy=False, **fields):
  return {'expected': str(expected), 'processes': [{'name': 'updated', 'pid': process.pid, 'running': True, **fields}], 'request': request, 'legacy': legacy}


def invoke(name, value):
  command = [str(args.binary)]
  result = subprocess.run(command, input=json.dumps(value) + '\n', text=True, capture_output=True, check=True, timeout=5)
  (args.output / f'{name}.json').write_text(json.dumps(value, indent=2))
  (args.output / f'{name}.stdout').write_text(result.stdout)
  (args.output / f'{name}.stderr').write_text(result.stderr or '(empty)\n')
  outcome = json.loads(result.stdout)
  records.append({'name': name, 'invocation': command, 'outcome': outcome})
  return outcome


with target(expected) as child:
  for legacy in [False, True]:
    for request, event in [('Check', 'check'), ('Download', 'download')]:
      name = f'{request}-{legacy}'
      value = invoke(name, fixture(child, request, legacy))
      assert value['Sent']['pid'] == child.pid
      assert value['Sent']['method'] == ('RecheckedPid' if legacy else 'Pidfd')
      observed = read_line(child)
      assert observed == event
      records[-1]['child_observable'] = observed
  for name, fields in [('not-running', {'running': False}), ('other-name', {'name': 'modeld'}), ('zero-pid', {'pid': 0}), ('negative-pid', {'pid': -1})]:
    assert invoke(name, fixture(child, **fields)) == {'Unavailable': 'NotRunning'}
  assert not select.select([child.stdout], [], [], 0.1)[0]
with target(other) as child:
  assert invoke('wrong-executable', fixture(child)) == {'Unavailable': 'IdentityChanged'}
  assert not select.select([child.stdout], [], [], 0.1)[0]
for legacy in [False, True]:
  with target(expected, other) as child:
    value = fixture(child, legacy=legacy)
    value['hold'] = True
    probe = subprocess.Popen([str(args.binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
      probe.stdin.write(json.dumps(value) + '\n')
      probe.stdin.flush()
      assert read_line(probe) == 'bound'
      child.stdin.write('X')
      child.stdin.flush()
      assert read_line(child) == 'ready'
      output, error = probe.communicate('send\n', timeout=5)
      assert probe.returncode == 0, (output, error)
      outcome = json.loads(output)
      assert outcome == {'Unavailable': 'IdentityChanged'}
      name = f'exec-replaced-{legacy}'
      (args.output / f'{name}.json').write_text(json.dumps(value, indent=2))
      (args.output / f'{name}.stdout').write_text(output)
      records.append({'name': name, 'outcome': outcome, 'child_observable': 'replacement alive, no signal'})
      assert not select.select([child.stdout], [], [], 0.1)[0]
    finally:
      if probe.poll() is None:
        probe.kill()
        probe.wait(timeout=5)
  with target(expected) as old:
    value = fixture(old, legacy=legacy)
    value['hold'] = True
    probe = subprocess.Popen([str(args.binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
      probe.stdin.write(json.dumps(value) + '\n')
      probe.stdin.flush()
      assert read_line(probe) == 'bound'
      old.terminate()
      old.wait(timeout=5)
      with target(expected) as replacement:
        output, error = probe.communicate('send\n', timeout=5)
        assert probe.returncode == 0, (output, error)
        outcome = json.loads(output)
        assert outcome == {'Unavailable': 'Gone'}
        name = f'restarted-bound-{legacy}'
        (args.output / f'{name}.stdout').write_text(output)
        records.append({'name': name, 'outcome': outcome, 'child_observable': 'new updater received no old request'})
        assert not select.select([replacement.stdout], [], [], 0.1)[0]
        assert invoke(f'stale-manager-{legacy}', fixture(old, legacy=legacy)) == {'Unavailable': 'Gone'}
        assert 'Sent' in invoke(f'new-manager-{legacy}', fixture(replacement, legacy=legacy))
        assert read_line(replacement) == 'check'
    finally:
      if probe.poll() is None:
        probe.kill()
        probe.wait(timeout=5)
(args.output / 'results.json').write_text(json.dumps({'compile': compile_command, 'cases': records, 'scope': 'owned child processes only'}, indent=2))
print(f'PASS: {len(records)} native managerState PID/identity/signal cases with pidfd and legacy recheck paths')
