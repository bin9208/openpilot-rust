from __future__ import annotations

from contextlib import ExitStack
from dataclasses import dataclass
import gc
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
from typing import Literal, assert_never
import uuid

from radarcan_exact import assert_exact
from radarcan_runtime_capture import Arguments, ROOT, drain, finish, parameters, provenance, recorded_environment
from radarcan_runtime_types import Case, Publication


@dataclass(frozen=True, slots=True)
class Lifecycle:
  name: str
  phase: Literal['params-wait', 'barrier', 'runtime', 'fatal']
  signal: int
  expected_exits: tuple[int, int]
  case: Case
  dbc: Path
  car_params: bytes | None = None
  constructor_status: tuple[str | None, str | None] = (None, None)
  error_markers: tuple[tuple[str, ...], tuple[str, ...]] = ((), ())
  parameter_directories: tuple[str, ...] = ()


def capture(args: Arguments, scenario: Lifecycle) -> dict[str, int | str]:
  from openpilot.cereal import messaging
  output = args.evidence / scenario.name
  output.mkdir(parents=True)
  modes = ['source', 'native']
  prefixes = ['radar193_' + uuid.uuid4().hex for _ in modes]
  queues = [Path('/dev/shm') / ('msgq_' + prefix) for prefix in prefixes]
  prior = os.environ.get('OPENPILOT_PREFIX')
  processes, publishers, subscribers = [], [], []
  rows: list[list[Publication]] = [[], []]
  shutdowns, prepared = [], []
  try:
    with ExitStack() as stack:
      for prefix, queue, mode in zip(prefixes, queues, modes, strict=True):
        queue.mkdir()
        lane = output / mode
        lane.mkdir()
        root = lane / 'params'
        prepared.append(parameters(root, prefix, scenario.case))
        for key in scenario.parameter_directories:
          path = root / prefix / key
          if path.exists():
            path.unlink()
          path.mkdir()
        publishers.append(messaging.PubMaster(['can', 'carState']))
        subscribers.append(messaging.sub_sock('liveTracks', conflate=False))
        command = [str(args.binary), '--numerics', str(args.numerics), '--dbc', str(scenario.dbc)]
        if mode == 'source':
          command = [os.sys.executable, '-P', str(ROOT / 'rust/tools/radarcan_runtime_source.py'),
            '--binding', str(args.binding), '--params-root', str(root), '--dbc', str(scenario.dbc)]
        if scenario.phase != 'runtime':
          command.extend(['--steps', '2000', '--fixture-constructor-ready', str(lane / 'constructor.json'),
            '--fixture-constructor-start', str(output / 'start')])
        environment = dict(os.environ, OPENPILOT_PREFIX=prefix, PARAMS_ROOT=str(root),
          OPENBLAS_NUM_THREADS='1', PYTHONUNBUFFERED='1', LOGPRINT='ERROR')
        environment.pop('REPLAY', None)
        (lane / 'command.json').write_text(json.dumps({'command': command, 'environment': recorded_environment(environment)}, indent=2))
        stdout = stack.enter_context((lane / 'stdout.log').open('w'))
        stderr = stack.enter_context((lane / 'stderr.log').open('w'))
        processes.append(subprocess.Popen(command, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr))
      deadline = time.monotonic() + 10
      while not all(publisher.all_readers_updated(topic) for publisher in publishers for topic in ('can', 'carState')):
        assert all(process.poll() is None for process in processes)
        assert time.monotonic() < deadline, 'constructor lifecycle sockets not registered'
        time.sleep(.001)
      for process, mode in zip(processes, modes, strict=True):
        provenance(process, output / mode)
      if scenario.phase != 'params-wait':
        for prefix, mode, raw in zip(prefixes, modes, prepared, strict=True):
          path = output / mode / 'params' / prefix / 'CarParams'
          pending = path.with_suffix('.pending')
          pending.write_bytes(scenario.car_params if scenario.car_params is not None else raw)
          pending.replace(path)
      else:
        time.sleep(.02)
        (output / 'wait-state.json').write_text(json.dumps({mode: Path(f'/proc/{process.pid}/wchan').read_text()
          for process, mode in zip(processes, modes, strict=True)}) + '\n')
      match scenario.phase:
        case 'barrier':
          while not all((output / mode / 'constructor.json').exists() for mode in modes):
            assert all(process.poll() is None for process in processes)
            assert time.monotonic() < deadline, 'constructor lifecycle readiness'
            time.sleep(.001)
          constructors = [json.loads((output / mode / 'constructor.json').read_text()) for mode in modes]
          assert all(record['status'] == 'ready' for record in constructors), constructors
        case 'runtime':
          while not all(rows):
            assert all(process.poll() is None for process in processes)
            assert time.monotonic() < deadline, 'ordinary runtime publication'
            for subscriber, values in zip(subscribers, rows, strict=True):
              drain(subscriber, values)
            time.sleep(.001)
          payloads = [{key: values[0]['event'][key] for key in ('valid', 'liveTracks')} for values in rows]
          assert_exact(payloads[1], payloads[0])
          assert not payloads[0]['valid'] and payloads[0]['liveTracks']['errors']['canError']
        case 'fatal' | 'params-wait':
          pass
        case unreachable:
          assert_never(unreachable)
      if scenario.signal:
        for process in processes:
          process.send_signal(scenario.signal)
      for process in processes:
        process.wait(timeout=max(.1, deadline - time.monotonic()))
      exits = tuple(process.returncode for process in processes)
      for mode, values in zip(modes, rows, strict=True):
        (output / mode / 'publications.json').write_text(json.dumps(values) + '\n')
      assert exits == scenario.expected_exits, (scenario.name, exits, scenario.expected_exits)
      for mode, status, markers in zip(modes, scenario.constructor_status, scenario.error_markers, strict=True):
        path = output / mode / 'constructor.json'
        if status is None:
          assert not path.exists(), (mode, 'unexpected constructor completion')
        else:
          assert path.exists() and json.loads(path.read_text())['status'] == status, (mode, status)
        stderr = (output / mode / 'stderr.log').read_text()
        assert all(marker in stderr for marker in markers), (mode, markers, stderr)
      result = {'name': scenario.name, 'status': 'pass', 'source_exit': exits[0], 'native_exit': exits[1]}
      (output / 'parameter-directories.json').write_text(json.dumps(scenario.parameter_directories))
      (output / 'result.json').write_text(json.dumps(result) + '\n')
      return result
  finally:
    for process in processes:
      shutdowns.append(finish(process))
    (output / 'shutdown.json').write_text(json.dumps(shutdowns) + '\n')
    subscribers.clear()
    publishers.clear()
    gc.collect()
    for queue in queues:
      if queue.exists():
        shutil.rmtree(queue)
    if prior is None:
      os.environ.pop('OPENPILOT_PREFIX', None)
    else:
      os.environ['OPENPILOT_PREFIX'] = prior
    (output / 'cleanup.json').write_text(json.dumps({'queues_absent': [not queue.exists() for queue in queues],
      'processes_exited': [process.poll() is not None for process in processes]}) + '\n')


def scenarios(case: Case, args: Arguments) -> list[Lifecycle]:
  missing = args.evidence / 'missing-dbc'
  missing.mkdir()
  output = []
  for name, value in [('sigint', signal.SIGINT), ('sigterm', signal.SIGTERM)]:
    for phase, exits, statuses in [('params-wait', (-signal.SIGINT, -signal.SIGINT), (None, None)),
                                   ('barrier', (-value, -value), ('ready', 'ready')),
                                   ('runtime', (-value, -value), (None, None))]:
      output.append(Lifecycle(f'lifecycle-{phase}-{name}', phase, value, exits, case, args.dbc,
        constructor_status=statuses))
  output.extend([
    Lifecycle('lifecycle-CarParams-directory-sigint', 'params-wait', signal.SIGINT,
      (-signal.SIGINT, -signal.SIGINT), case, args.dbc, parameter_directories=('CarParams',)),
    Lifecycle('lifecycle-CarParams-directory-sigterm', 'params-wait', signal.SIGTERM,
      (-signal.SIGINT, -signal.SIGINT), case, args.dbc, parameter_directories=('CarParams',)),
    Lifecycle('lifecycle-RadarTrackFlip-directory', 'runtime', signal.SIGINT,
      (-signal.SIGINT, -signal.SIGINT), case, args.dbc, parameter_directories=('RadarTrackFlip',)),
    Lifecycle('lifecycle-EnableRadarTracks-directory', 'barrier', signal.SIGINT,
      (-signal.SIGINT, -signal.SIGINT),
      {**case, 'candidate': 'HYUNDAI_IONIQ_5', 'flags': 8193, 'ext_flags': 0,
       'params': {'EnableRadarTracks': '1', 'EnableCornerRadar': '0', 'HyundaiCameraSCC': '0'}}, args.dbc,
      constructor_status=('ready', 'ready'), parameter_directories=('EnableRadarTracks',)),
    Lifecycle('lifecycle-missing-dbc', 'fatal', 0, (1, 1), case, missing,
      constructor_status=('error', 'error'), error_markers=(('FileNotFoundError', 'vw_meb.dbc'), ('required radar DBC asset', 'vw_meb.dbc'))),
    Lifecycle('lifecycle-negative-history', 'fatal', 0, (1, 1), {**case, 'delay': -.02}, args.dbc,
      constructor_status=('error', 'error'), error_markers=(('ValueError', 'maxlen must be non-negative'), ('maxlen must be non-negative',))),
    Lifecycle('lifecycle-unknown-candidate', 'fatal', 0, (1, 1), {**case, 'candidate': 'UNRECOGNIZED_RADAR_FIXTURE'}, args.dbc,
      constructor_status=(None, 'error'), error_markers=(('KeyError', 'UNRECOGNIZED_RADAR_FIXTURE'), ('UNRECOGNIZED_RADAR_FIXTURE',))),
    Lifecycle('lifecycle-malformed-CarParams', 'fatal', 0, (1, 1), case, args.dbc, car_params=b'x',
      error_markers=(('ValueError', 'multiple of eight'), ('multiple of eight',))),
    Lifecycle('lifecycle-inherited-Params-integer-abort', 'fatal', 0, (-signal.SIGABRT, 1),
      {**case, 'candidate': 'HYUNDAI_IONIQ_5', 'flags': 8193, 'ext_flags': 0,
       'params': {'EnableRadarTracks': 'invalid', 'EnableCornerRadar': '1', 'HyundaiCameraSCC': '0'}}, args.dbc,
      constructor_status=(None, 'error'), error_markers=(('stoi',), ('inherited fatal Params.get_int(EnableRadarTracks)',))),
  ])
  return output
