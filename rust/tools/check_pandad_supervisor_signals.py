import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import signal
import shlex
import subprocess
import sys
import threading
import time
import zmq
from check_pandad_native_supervisor import ROOT, equal, original


def collect(endpoint, ready, stop, rows):
  with zmq.Context() as context, context.socket(zmq.PULL) as receiver:
    receiver.bind(endpoint)
    ready.set()
    while not stop.is_set():
      if receiver.poll(20):
        rows.append(json.loads(receiver.recv()[1:]))


def await_state(process, path, predicate, wchan):
  deadline = time.monotonic() + 10
  while time.monotonic() < deadline:
    if process.poll() is not None:
      raise AssertionError(('parent exited before signal', process.returncode))
    if path.exists():
      try:
        rows = [json.loads(line) for line in path.read_text().splitlines()]
        state = Path(f'/proc/{process.pid}/wchan').read_text().strip()
        if predicate(rows) and any(value in state for value in wchan):
          return {'observed_wchan': state, 'rows_before_signal': rows}
      except (json.JSONDecodeError, FileNotFoundError):
        pass
    time.sleep(0.005)
  raise AssertionError(('parent did not enter expected signal state', path, wchan))


def source():
  case = json.loads(Path(sys.argv[2]).read_text())
  output, firmware, child = map(Path, sys.argv[3:])
  result, _ = original(case, firmware, child=child, live_log=output / 'source-live.jsonl')
  assert result['terminal'] is None, result
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--launcher', type=Path, required=True)
  parser.add_argument('--library', type=Path, required=True)
  parser.add_argument('--preload', type=Path)
  parser.add_argument('--runner', default='')
  parser.add_argument('--child', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  library_dir = args.output / 'libraries'
  library_dir.mkdir()
  (library_dir / 'libusb-1.0.so.0').symlink_to(args.library.resolve())
  default = {'serial': '010002000300040005000600', 'hardware': 9, 'health': [0] * 58, 'cycles': 2}
  cases = [('child', default), ('setup', {**default, 'signal_at': 0}), ('sleep', {**default, 'missing': True})]
  results = []
  for name, case in cases:
    base = args.output / name
    base.mkdir()
    firmware = base / 'firmware'
    firmware.mkdir()
    for filename in ('panda.bin.signed', 'panda_h7.bin.signed'):
      (firmware / filename).write_bytes(bytes(range(128)))
    (base / 'case.json').write_text(json.dumps(case) + '\n')
    _, steps = original(case, firmware)
    fixture = {'raw': True, 'devices': [] if case.get('missing') else [
      {'vendor': 0x3801, 'product': 0xddcc, 'serial': list(case['serial'].encode()), 'bcd': 0x0900}], 'script': {'steps': steps}}
    outcomes = []
    for lane in ('source', 'native'):
      output = base / lane
      output.mkdir()
      (output / 'owned-descriptor').write_bytes(b'owned inherited descriptor')
      prefix = f'panda-signal-{os.getpid()}-{name}-{lane}'
      endpoint = f'ipc:///tmp/logmessage{prefix}'
      env = {**os.environ, 'OPENPILOT_PREFIX': prefix, 'LD_LIBRARY_PATH': str(library_dir.resolve()),
             'PANDA_FIRMWARE_USB_LIBRARY': str(args.library.resolve()), 'PANDA_FIRMWARE_USB_CASE': json.dumps(fixture),
             'PANDA_FIRMWARE_USB_TRACE': str(output / 'usb.json'), 'PANDA_OWNED_DESCRIPTOR': str(output / 'owned-descriptor'),
             'PANDA_CHILD_TRACE': str(output / 'children.jsonl'), 'PANDA_CHILD_MODE': 'hold', 'LOGPRINT': 'debug'}
      rows = []
      stop, ready = threading.Event(), threading.Event()
      thread = None
      if lane == 'native':
        env.pop('LD_PRELOAD', None)
        if args.preload is not None:
          env['LD_PRELOAD'] = str(args.preload.resolve())
        thread = threading.Thread(target=collect, args=(endpoint, ready, stop, rows))
        thread.start()
        assert ready.wait(5)
        command = [*shlex.split(args.runner), str(args.binary.resolve()), str(output / 'root'), str(ROOT), str(firmware),
                   str(args.child.resolve()), str(args.launcher.resolve()), str(case['cycles'])]
      else:
        command = [sys.executable, str(Path(__file__).resolve()), '--source', str(base / 'case.json'),
                   str(output), str(firmware), str(args.child.resolve())]
      with (output / 'stdout').open('w') as stdout, (output / 'stderr').open('w') as stderr:
        process = subprocess.Popen(command, env=env, stdout=stdout, stderr=stderr)
        try:
          if name == 'sleep':
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
              live = rows if lane == 'native' else [json.loads(line) for line in
                                                   (output / 'source-live.jsonl').read_text().splitlines()] if (output / 'source-live.jsonl').exists() else []
              if any(entry['msg'] == 'No pandas found, resetting internal panda' for entry in live):
                break
              assert process.poll() is None
              time.sleep(0.005)
            else:
              raise AssertionError('missing panda reset did not start')
            observation = await_state(process, output / 'stderr' if lane == 'native' else output / 'source-live.jsonl',
                                      lambda _: True, ['hrtimer', 'nanosleep']) if lane == 'source' else {'phase': 'sleep-reset-log'}
          else:
            observation = await_state(process, output / 'children.jsonl', lambda values: any(row['phase'] == 0 for row in values),
                                      ['futex'] if lane == 'native' else ['do_wait'])
          (output / 'signal-observation.json').write_text(json.dumps(observation, indent=2) + '\n')
          started = time.monotonic()
          process.send_signal(signal.SIGINT)
          assert process.wait(timeout=10) == 0
          elapsed = time.monotonic() - started
          if name == 'sleep':
            assert elapsed >= 2.0, (lane, elapsed, 'source sleep must finish after SIGINT')
          if lane == 'native':
            time.sleep(0.05)
        finally:
          if process.poll() is None:
            process.kill()
            process.wait()
          stop.set()
          if thread is not None:
            thread.join(5)
            assert not thread.is_alive()
            Path('/tmp/logmessage' + prefix).unlink(missing_ok=True)
      child_path = output / 'children.jsonl'
      children = [json.loads(line) for line in child_path.read_text().splitlines()] if child_path.exists() else []
      assert all(row['extra_fds'] == [] and row['manager'] for row in children), children
      assert [row['phase'] for row in children] == ([] if name == 'sleep' else [0, signal.SIGINT])
      if lane == 'native':
        logs = [{key: entry[key] for key in ('level', 'msg')} for entry in rows]
        (output / 'logs.json').write_text(json.dumps(rows, indent=2) + '\n')
      else:
        logs = json.loads((output / 'result.json').read_text())['logs']
      assert sum(entry['msg'] == 'Caught signal 2, exiting' for entry in logs) == (2 if name == 'setup' else 1), logs
      outcomes.append(logs)
    assert equal(outcomes[0]) == equal(outcomes[1]), (name, 'source/native signal logs differ')
    results.append({'scenario': name, 'logs': len(outcomes[0]), 'result': 'PASS'})
  report = {'result': 'PASS', 'scenarios': results,
            'native_runner': shlex.split(args.runner),
            'sha256': {str(path): sha256(path.read_bytes()).hexdigest() for path in (args.binary, args.launcher, args.library, args.child, Path(__file__))},
            'limits': 'Real SIGINT, native exec child and log IPC; owned USB and filesystem boundaries. Original wrapper body unchanged. No physical device.'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  source() if sys.argv[1:2] == ['--source'] else main()
