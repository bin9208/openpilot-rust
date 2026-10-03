import argparse
import hashlib
import json
from pathlib import Path
import signal
import subprocess
import time

from bluetooth_daemon_fixture import Fixture, MAC, Program, wait_for


def capture(root: Path, program: Program, sig: signal.Signals) -> dict:
  fixture = Fixture(root, program)
  result = {}
  try:
    child_pid = None
    if program.permission_wait:
      wait_for(lambda: (fixture.root / 'sudo.jsonl').exists())
      children = Path(f'/proc/{fixture.child.pid}/task/{fixture.child.pid}/children').read_text().split()
      assert len(children) == 1, children
      child_pid = int(children[0])
    else:
      wait_for(lambda: fixture.status().get('grabbed') == [MAC])
    before = fixture.status()
    started = time.monotonic()
    fixture.child.send_signal(sig)
    assert fixture.child.wait(timeout=2) == -sig
    elapsed = time.monotonic() - started
    after = fixture.status()
    result['code'] = fixture.child.returncode
    if sig == signal.SIGINT:
      result['status'] = {key: value for key, value in after.items() if key != 'time'}
      assert result['status'] == {'stationary': False, 'grabbed': [], 'stopped': True}
    else:
      assert {key: value for key, value in before.items() if key != 'time'} == {
        key: value for key, value in after.items() if key != 'time'}
      result['status_preserved'] = True
    if child_pid is not None:
      wait_for(lambda: not Path(f'/proc/{child_pid}').exists(), 1)
      result['permission_child_reaped'] = True
      assert len((fixture.root / 'sudo.jsonl').read_text().splitlines()) == 1
    (fixture.root / 'timing.json').write_text(json.dumps({'elapsed_seconds': elapsed}))
    if not program.permission_wait:
      fixture.child = subprocess.Popen(fixture.command, env=fixture.env, stdout=fixture.log, stderr=subprocess.STDOUT)
      wait_for(lambda: fixture.status().get('time', 0) > after['time'] and fixture.status().get('grabbed') == [MAC])
      fixture.child.send_signal(signal.SIGINT)
      assert fixture.child.wait(timeout=2) == -signal.SIGINT
      result['restart_after_release'] = True
      if program.binary is not None:
        trace = fixture.root / 'execve.trace'
        process = subprocess.run(['strace', '-f', '-e', 'trace=execve', '-o', str(trace), *fixture.command, '--frames', '3'],
                                 env=fixture.env, capture_output=True, text=True, timeout=3)
        assert process.returncode == 0, process.stderr
        calls = [line for line in trace.read_text().splitlines() if 'execve(' in line]
        assert len(calls) == 1 and str(program.binary.resolve()) in calls[0], calls
  finally:
    fixture.close()
    (root / 'result.json').write_text(json.dumps(result, indent=2))
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True, type=Path)
  parser.add_argument('--shim', required=True, type=Path)
  parser.add_argument('--output', required=True, type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  rows = []
  for name, sig, permission in [('sigint', signal.SIGINT, False), ('sigterm', signal.SIGTERM, False),
                                ('permission-sigint', signal.SIGINT, True)]:
    source = capture(args.output / name / 'source', Program(None, args.shim, permission), sig)
    native = capture(args.output / name / 'native', Program(args.binary, args.shim, permission), sig)
    assert source == native, (source, native)
    rows.append({'case': name, 'source': source, 'native': native})
  (args.output / 'result.json').write_text(json.dumps({'cases': rows,
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}, indent=2))
  print('PASS: source/native SIGINT and SIGTERM state, restart, interruptible permission child ownership, native exec trace')


if __name__ == '__main__':
  main()
