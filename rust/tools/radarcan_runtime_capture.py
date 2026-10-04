from __future__ import annotations

from contextlib import ExitStack
from dataclasses import dataclass
import gc
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
from typing import Final
import uuid

from radarcan_exact import assert_exact
from radarcan_runtime_types import Case, Publication

ROOT: Final = Path(__file__).resolve().parents[2]


def recorded_environment(environment: dict[str, str]) -> dict[str, str]:
  names = ('PATH', 'PYTHONPATH', 'PYTHONHASHSEED', 'OPENBLAS_NUM_THREADS', 'OPENPILOT_PREFIX',
    'PARAMS_ROOT', 'LOGPRINT', 'REPLAY', 'SIMULATION', 'LD_LIBRARY_PATH', 'LD_PRELOAD')
  return {name: environment[name] for name in names if name in environment}


@dataclass(frozen=True, slots=True)
class Arguments:
  binary: Path
  binding: Path
  numerics: Path
  dbc: Path
  evidence: Path


def parameters(root: Path, prefix: str, case: Case) -> bytes:
  from openpilot.cereal import car
  from openpilot.common.params import Params
  os.environ['OPENPILOT_PREFIX'] = prefix
  values = Params(str(root))
  cp = car.CarParams.new_message(carFingerprint=case['candidate'], radarDelay=case['delay'],
    radarTimeStep=case['period'], radarUnavailable=case['unavailable'], flags=case.get('flags', 0),
    extFlags=case.get('ext_flags', 0), safetyConfigs=[{} for _ in range(case.get('safety_count', 1))])
  raw = cp.to_bytes()
  Path(values.get_param_path('RadarTrackFlip')).write_bytes(b'1' if case.get('flip', False) else b'0')
  for key, value in case.get('params', {}).items():
    Path(values.get_param_path(key)).write_bytes(value.encode())
  return raw


def provenance(process: subprocess.Popen, output: Path) -> None:
  proc = Path('/proc') / str(process.pid)
  maps = (proc / 'maps').read_text()
  if output.name == 'native':
    assert 'libpython' not in maps
  (output / 'maps.txt').write_text(maps)
  (output / 'process.json').write_text(json.dumps({'pid': process.pid, 'exe': str((proc / 'exe').resolve()),
    'exe_sha256': hashlib.sha256((proc / 'exe').read_bytes()).hexdigest(),
    'fds': {fd.name: os.readlink(fd) for fd in (proc / 'fd').iterdir()}, 'captured_ns': time.monotonic_ns()}) + '\n')


def stop(process: subprocess.Popen) -> int:
  if process.poll() is None:
    process.send_signal(signal.SIGSTOP)
    assert os.WIFSTOPPED(os.waitpid(process.pid, os.WUNTRACED)[1])
  return time.monotonic_ns()


def finish(process: subprocess.Popen) -> dict[str, int | bool]:
  forced = False
  if process.poll() is None:
    process.send_signal(signal.SIGINT)
    process.send_signal(signal.SIGCONT)
    try:
      process.wait(timeout=3)
    except subprocess.TimeoutExpired:
      forced = True
      process.kill()
      process.wait(timeout=3)
  return {'exit': process.returncode, 'forced': forced}


def drain(subscriber, rows: list[Publication]) -> None:
  from openpilot.cereal import messaging
  while raw := subscriber.receive(non_blocking=True):
    received = time.monotonic_ns()
    event = messaging.log_from_bytes(raw)
    assert event.which() == 'liveTracks'
    rows.append({'received_ns': received, 'event': event.to_dict(), 'raw': raw.hex()})


def capture(args: Arguments, case: Case) -> dict[str, int | str]:
  """Broadcast identical input bytes to independent actual source/native loops."""
  from openpilot.cereal import messaging
  output = args.evidence / case['name']
  output.mkdir(parents=True)
  case_path = output / 'case.json'
  case_path.write_text(json.dumps(case) + '\n')
  prefixes = ['radar193_' + uuid.uuid4().hex for _ in range(2)]
  queues = [Path('/dev/shm') / ('msgq_' + prefix) for prefix in prefixes]
  prior = os.environ.get('OPENPILOT_PREFIX')
  processes, subscribers, rows, modes = [], [], [[], []], ['source', 'native']
  producer = None
  stopped = []
  shutdowns = []
  result = None
  try:
    with ExitStack() as stack:
      producer_command = [os.sys.executable, '-P', str(ROOT / 'rust/tools/radarcan_runtime_producer.py'),
        '--case', str(case_path), '--output', str(output / 'producer.json'), '--order', case.get('order', 'can-state'),
        '--ready', str(output / 'ready.json'), '--start', str(output / 'producer-start')]
      if case.get('prequeue', False):
        producer_command.extend(['--prequeue-complete', str(output / 'prequeued')])
      for prefix, queue, mode in zip(prefixes, queues, modes, strict=True):
        queue.mkdir()
        producer_command.extend(['--prefix', prefix])
        lane = output / mode
        lane.mkdir()
        root = lane / 'params'
        raw = parameters(root, prefix, case)
        (lane / 'CarParams.bin').write_bytes(raw)
        subscribers.append(messaging.sub_sock('liveTracks', conflate=False))
      environment = dict(os.environ, OPENBLAS_NUM_THREADS='1', LOGPRINT='ERROR')
      for flag in ('REPLAY', 'SIMULATION'):
        environment.pop(flag, None)
      producer_log = stack.enter_context((output / 'producer.log').open('w'))
      producer = subprocess.Popen(producer_command, cwd=ROOT, env=environment, stdout=producer_log, stderr=producer_log)
      launched = time.monotonic_ns()
      for prefix, mode in zip(prefixes, modes, strict=True):
        lane = output / mode
        command = [str(args.binary), '--numerics', str(args.numerics), '--dbc', str(args.dbc)]
        if mode == 'source':
          command = [os.sys.executable, '-P', str(ROOT / 'rust/tools/radarcan_runtime_source.py'),
            '--binding', str(args.binding), '--params-root', str(lane / 'params'), '--dbc', str(args.dbc)]
        command.extend(['--steps', '2000', '--fixture-constructor-ready', str(lane / 'constructor.json'),
          '--fixture-constructor-start', str(output / 'start')])
        env = dict(environment, OPENPILOT_PREFIX=prefix, PARAMS_ROOT=str(lane / 'params'))
        (lane / 'command.json').write_text(json.dumps({'command': command, 'environment': recorded_environment(env)}, indent=2))
        stdout = stack.enter_context((lane / 'stdout.log').open('w'))
        stderr = stack.enter_context((lane / 'stderr.log').open('w'))
        processes.append(subprocess.Popen(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr))
      deadline = time.monotonic() + 20
      completion_ns = None
      retained = False
      started = False
      parameters_released = False
      parameters_changed = False
      while True:
        for process in processes:
          assert process.poll() is None, ('radar process exit', process.returncode)
        assert time.monotonic() < deadline, 'bounded radar IPC capture deadline'
        for subscriber, values in zip(subscribers, rows, strict=True):
          drain(subscriber, values)
        if not parameters_changed and 'params_after_first' in case and all(rows):
          for prefix, mode in zip(prefixes, modes, strict=True):
            for key, value in case['params_after_first'].items():
              path = output / mode / 'params' / prefix / key
              pending = path.with_suffix('.pending')
              pending.write_text(value)
              pending.replace(path)
          (output / 'params-updated.json').write_text(json.dumps({'timestamp_ns': time.monotonic_ns(),
            'after_publications': [len(values) for values in rows], 'values': case['params_after_first']}) + '\n')
          parameters_changed = True
        if not parameters_released and (output / 'ready.json').exists():
          for prefix, mode in zip(prefixes, modes, strict=True):
            lane = output / mode
            pending = lane / 'params' / prefix / 'CarParams.pending'
            pending.write_bytes((lane / 'CarParams.bin').read_bytes())
            pending.replace(pending.with_name('CarParams'))
          parameters_released = True
        if parameters_released and not started and all((output / mode / 'constructor.json').exists() for mode in modes):
          constructors = [json.loads((output / mode / 'constructor.json').read_text()) for mode in modes]
          assert all(record['status'] == 'ready' for record in constructors), constructors
          for process, mode in zip(processes, modes, strict=True):
            provenance(process, output / mode)
          retained = True
          (output / 'producer-start').write_text(str(time.monotonic_ns()))
          if not case.get('prequeue', False):
            (output / 'start').write_text(str(time.monotonic_ns()))
          started = True
        if case.get('prequeue', False) and (output / 'prequeued').exists() and not (output / 'start').exists():
          (output / 'start').write_text(str(time.monotonic_ns()))
        if not retained and rows[0] and rows[1]:
          for process, mode in zip(processes, modes, strict=True):
            provenance(process, output / mode)
          retained = True
        if producer.poll() is not None:
          assert producer.returncode == 0, ('independent producer exit', producer.returncode)
          if completion_ns is None:
            completion_ns = time.monotonic_ns()
          if time.monotonic_ns() - completion_ns >= 20_000_000:
            for process in processes:
              stopped.append(stop(process))
            break
        time.sleep(.0005)
      for process, mode in zip(processes, modes, strict=True):
        if not retained:
          provenance(process, output / mode)
      for subscriber, values, mode in zip(subscribers, rows, modes, strict=True):
        drain(subscriber, values)
        (output / mode / 'publications.json').write_text(json.dumps(values) + '\n')
      producer_result = json.loads((output / 'producer.json').read_text())
      assert 'params_after_first' not in case or parameters_changed
      assert len(producer_result['rows']) == len(case['actions'])
      assert producer_result['encoded_done_ns'] - producer_result['origin_ns'] < 100_000_000
      assert max(stopped) - producer_result['rows'][-1]['send_started_ns'] < 100_000_000
      for mode, values, stop_ns in zip(modes, rows, stopped, strict=True):
        for value in values:
          assert launched <= value['event']['logMonoTime'] <= stop_ns
          assert value['event']['logMonoTime'] <= value['received_ns']
        if 'expected_publications' in case:
          assert len(values) == case['expected_publications'], (mode, len(values), case['expected_publications'])
        if 'expected_error_publications' in case:
          errors = sum(not value['event']['valid'] for value in values)
          assert errors == case['expected_error_publications'], (mode, errors, case['expected_error_publications'])
        assert all(value['event']['liveTracks']['radarTrackFlipped'] == case.get('flip', False) for value in values)
      payloads = [[{key: value['event'][key] for key in ('valid', 'liveTracks')} for value in values] for values in rows]
      assert_exact(payloads[1], payloads[0])
      result = {'name': case['name'], 'status': 'pass', 'input_steps': len(case['actions']), 'publications': len(rows[0])}
  finally:
    for process in processes:
      shutdowns.append(finish(process))
    if producer is not None and producer.poll() is None:
      producer.kill()
      producer.wait(timeout=3)
    (output / 'shutdown.json').write_text(json.dumps(shutdowns) + '\n')
    subscribers.clear()
    gc.collect()
    for queue in queues:
      if queue.exists():
        shutil.rmtree(queue)
    if prior is None:
      os.environ.pop('OPENPILOT_PREFIX', None)
    else:
      os.environ['OPENPILOT_PREFIX'] = prior
    (output / 'cleanup.json').write_text(json.dumps({'queues_absent': [not queue.exists() for queue in queues],
      'processes_exited': [process.poll() is not None for process in processes],
      'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}) + '\n')
  assert all(not value['forced'] for value in shutdowns), shutdowns
  assert result is not None
  result.update(source_exit=shutdowns[0]['exit'], native_exit=shutdowns[1]['exit'])
  (output / 'result.json').write_text(json.dumps(result) + '\n')
  return result
