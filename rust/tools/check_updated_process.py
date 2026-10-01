"""Source/native command output, environment and owned session/group lifetime."""

import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]


def alive(pid):
  try:
    return Path(f'/proc/{pid}/stat').read_text().split(')', 1)[1].split()[0] != 'Z'
  except (FileNotFoundError, ProcessLookupError):
    return False


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--launcher', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  summary = []
  with tempfile.TemporaryDirectory(prefix='updated-command-') as temporary:
    root = Path(temporary)
    env = dict(
      os.environ,
      PYTHONPATH=str(ROOT),
      GIT_CONFIG_NOSYSTEM='1',
      GIT_CONFIG_GLOBAL='/dev/null',
      GIT_CONFIG_COUNT='2',
      GIT_CONFIG_KEY_0='test.preserved',
      GIT_CONFIG_VALUE_0='keep',
      GIT_CONFIG_KEY_1='gc.auto',
      GIT_CONFIG_VALUE_1='1',
    )
    cases = [
      ('merged-output', [sys.executable, '-c', "import sys;sys.stdout.write('one\\r\\n');sys.stdout.flush();sys.stderr.write('two\\r');sys.stderr.flush()"]),
      ('nonzero', [sys.executable, '-c', "print('failure detail',flush=True);raise SystemExit(7)"]),
      (
        'environment',
        [
          sys.executable,
          '-c',
          "import subprocess;"
          + "[subprocess.run(['git','config','--get',key],check=True) "
          + "for key in ['test.preserved','gc.auto','gc.autoDetach','maintenance.auto']]",
        ],
      ),
    ]
    for name, argv in cases:
      pair = []
      for implementation in ['source', 'native']:
        config = {'launcher': str(args.launcher.resolve()), 'argv': argv, 'cwd': str(root), 'endpoint': 'ipc://' + str(root / 'logs.sock')}
        command = [sys.executable, str(ROOT / 'rust/tools/updated_process_source.py')] if implementation == 'source' else [str(args.binary.resolve())]
        result = subprocess.run(command, input=json.dumps(config), env=env, capture_output=True, text=True, timeout=10)
        (args.output / f'{name}-{implementation}.stdout').write_text(result.stdout or '<empty>\n')
        (args.output / f'{name}-{implementation}.stderr').write_text(result.stderr or '<empty>\n')
        assert result.returncode == 0, result.stderr
        pair.append(json.loads(result.stdout))
      assert pair[0] == pair[1], (name, pair)
      if name == 'merged-output':
        assert pair[1] == {'output': 'one\ntwo\n', 'code': 0}
      if name == 'environment':
        assert pair[1]['output'] == 'keep\n0\nfalse\nfalse\n'
      summary.append({'scenario': name, 'passed': True, 'result': pair[1]})
    for parent_exits in [False, True]:
      for implementation in ['source', 'native']:
        pid_file = root / f'{implementation}-{parent_exits}.pid'
        worker = (
          "import os,signal,time;from pathlib import Path;"
          + "signal.signal(signal.SIGINT,signal.SIG_IGN);signal.signal(signal.SIGTERM,signal.SIG_IGN);"
          + f"Path({str(pid_file)!r}).write_text(str(os.getpid()));time.sleep(60)"
        )
        leader = f"import subprocess,sys,time;subprocess.Popen([sys.executable,'-c',{worker!r}]);" + ('sys.exit(0)' if parent_exits else 'time.sleep(60)')
        config = {
          'launcher': str(args.launcher.resolve()),
          'argv': [sys.executable, '-c', leader],
          'cwd': str(root),
          'endpoint': 'ipc://' + str(root / 'logs.sock'),
        }
        command = [sys.executable, str(ROOT / 'rust/tools/updated_process_source.py')] if implementation == 'source' else [str(args.binary.resolve())]
        process = subprocess.Popen(command, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        worker_pid = None
        try:
          process.stdin.write(json.dumps(config))
          process.stdin.close()
          process.stdin = None
          deadline = time.monotonic() + 5
          while not pid_file.exists() or not pid_file.read_text():
            assert time.monotonic() < deadline, (implementation, process.poll())
            time.sleep(0.01)
          worker_pid = int(pid_file.read_text())
          started = time.monotonic()
          process.send_signal(signal.SIGINT)
          stdout, stderr = process.communicate(timeout=4)
          elapsed = time.monotonic() - started
          assert process.returncode == 0 and json.loads(stdout)['interrupted']
          deadline = time.monotonic() + 1
          while alive(worker_pid) and time.monotonic() < deadline:
            time.sleep(0.01)
          assert not alive(worker_pid), ('orphan survived', worker_pid)
          prefix = f'group-{implementation}-{parent_exits}'
          (args.output / f'{prefix}.stdout').write_text(stdout)
          (args.output / f'{prefix}.stderr').write_text(stderr or '<empty>\n')
          summary.append({'scenario': prefix, 'passed': True, 'elapsed': elapsed, 'worker_stopped': True, 'exit_code': process.returncode})
        finally:
          if process.poll() is None:
            process.kill()
            process.wait(timeout=3)
          if worker_pid is not None and alive(worker_pid):
            os.kill(worker_pid, signal.SIGKILL)
    (args.output / 'result.json').write_text(json.dumps({'passed': True, 'cases': summary}, indent=2) + '\n')
    print('PASS updater commands: merged output, nonzero status, preserved maintenance environment, and SIGINT group cleanup with live/exited leaders')


if __name__ == '__main__':
  main()
