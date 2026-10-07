#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# Imported by check_card_runtime_pumped.py with retained repository bindings.
# ──────────────────
from __future__ import annotations

from contextlib import ExitStack
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
import uuid

from openpilot.cereal import car, messaging
from card_vehicle_source import normalize
from card_qa.runtime_inputs import Scenario
from card_qa.runtime_pumped_types import Capture, Publications, RuntimeInvocation, RuntimePeer, Subscriber
from card_qa.runtime_pumped_shutdown import finish
from check_card_runtime import OUTPUTS, ROOT, decoded_event


def set_phase(output: Path, phase: str) -> None:
  pending = output / 'pump-control-next'
  pending.write_text(phase)
  pending.replace(output / 'pump-control')


def pause_pump(output: Path) -> None:
  set_phase(output, 'pause')
  deadline = time.monotonic() + 3
  while (output / 'pump/phase').read_text() != 'pause':
    assert time.monotonic() < deadline, 'pump pause acknowledgment'
    time.sleep(.001)


def drain(subscribers: dict[str, Subscriber]) -> Publications:
  """Retain every publication without blocking the independent sender."""
  rows = {name: [] for name in OUTPUTS}
  for name, socket in subscribers.items():
    while raw := socket.receive(non_blocking=True):
      rows[name].append(decoded_event(raw))
  return rows


def stop(process: subprocess.Popen) -> None:
  process.send_signal(signal.SIGSTOP)
  assert os.WIFSTOPPED(os.waitpid(process.pid, os.WUNTRACED)[1])


def phase_capture(context: RuntimePeer, phase: str) -> Publications:
  """Collect a finite phase, retaining raw clocks and packet metadata."""
  process, pump, subscribers, output = context.process, context.pump, context.subscribers, context.output
  passive, candidate = context.passive, context.candidate
  if phase == 'warmup':
    from card_qa.runtime_phase_fence import warmup
    return warmup(context)
  set_phase(output, phase)
  deadline = time.monotonic() + 3
  while True:
    sends = (output / 'pump/sends.jsonl').read_text().splitlines()
    if sends and json.loads(sends[-1])['mode'] == phase:
      break
    assert time.monotonic() < deadline, 'first phase CAN send'
    time.sleep(.001)
  process.send_signal(signal.SIGCONT)
  rows = {name: [] for name in OUTPUTS}
  deadline = time.monotonic() + 30
  while True:
    assert process.poll() is None, process.returncode
    assert pump.poll() is None, pump.returncode
    assert time.monotonic() < deadline, (phase, {name: len(values) for name, values in rows.items()})
    for name, values in drain(subscribers).items():
      rows[name].extend(values)
    complete = output / 'pump/complete'
    if complete.exists() and complete.read_text() == phase:
      sends = [json.loads(line) for line in (output / 'pump/sends.jsonl').read_text().splitlines()]
      timestamps = {row['timestamp'] for row in sends if row['mode'] == phase}
      received = {row['carState']['radarInput']['lastCanMonoTime'] for row in rows['carState']}
      if max(timestamps) in received:
        trace = (output / 'frequency.jsonl').read_text().splitlines()
        sample = json.loads(trace[-1]) if trace else None
        ready = sample and sample['frequency_ok'] and (passive or sample['controls_ready'])
        if ready and candidate == 'GENESIS_G70':
          ready = sample['parser']['counter'] > 200 and sample['parser']['pt_ready'] and sample['parser']['cam_ready']
        if ready and sample['frame'] >= context.observed_frames + len(rows['carState']) - 1:
          stop(process)
          pause_pump(output)
          for name, values in drain(subscribers).items():
            rows[name].extend(values)
          (output / (phase + '-raw.json')).write_text(json.dumps(rows) + '\n')
          return rows
    time.sleep(.001)


def capture(arguments: RuntimeInvocation, scenario: Scenario, source: bool) -> Capture:
  """Run the original constructor or native CLI with an owned 100 Hz pump."""
  mode = 'source' if source else 'native'
  output = arguments.evidence / f'{scenario.candidate}-{scenario.enabled}-{mode}'
  output.mkdir(parents=True)
  prefix = 'cardpump_' + uuid.uuid4().hex
  queue = Path('/dev/shm') / ('msgq_' + prefix)
  queue.mkdir()
  params_root = output / 'params'
  params = params_root / prefix
  params.mkdir(parents=True)
  prior = os.environ.get('OPENPILOT_PREFIX')
  os.environ['OPENPILOT_PREFIX'] = prefix
  from openpilot.common.params import Params
  settings = Params(str(params_root))
  for key in settings.all_keys():
    default = settings.get_default_value(key)
    if default is not None:
      settings.put(key, default)
  values = {'OpenpilotEnabledToggle': '1' if scenario.enabled else '0', 'DisengageOnAccelerator': '1', 'IsMetric': '1',
    'LongitudinalPersonality': '1', 'AutoEngage': '0', 'ControlsReady': '0', 'GitRemote': 'https://example.com/owned/repo.git',
    'GitBranch': 'fixture', 'GitCommitDate': 'owned-date'}
  if scenario.alpha_long is not None:
    values['AlphaLongitudinalEnabled'] = '1' if scenario.alpha_long else '0'
  for key, value in values.items():
    (params / key).write_text(value)
  command = [str(arguments.binary), '--root', str(arguments.runtime_root), '--numerics', str(arguments.numerics), '--max-steps', '2000']
  if source:
    command = [os.sys.executable, str(ROOT / 'rust/tools/card_runtime_source.py'), '--binding', str(arguments.binding),
      '--params-root', str(params_root), '--steps', '2000']
    if arguments.runtime_root != ROOT:
      bootstrap = '; '.join(['import sys', 'sys.path[0] = sys.argv[2].rsplit("/", 1)[0]', 'import opendbc, runpy',
        'opendbc.DBC_PATH = sys.argv[1]', 'sys.argv = sys.argv[2:]', 'runpy.run_path(sys.argv[0], run_name="__main__")'])
      command = [os.sys.executable, '-c', bootstrap, str(arguments.runtime_root / 'opendbc_repo/opendbc/dbc'), *command[1:]]
  command.extend(['--frequency-trace', str(output / 'frequency.jsonl')])
  command.extend(['--fixture-phase-fence', str(output / 'phase-fence')])
  environment = dict(os.environ, OPENPILOT_PREFIX=prefix, PARAMS_ROOT=str(params_root), FINGERPRINT=scenario.candidate)
  if arguments.simulation:
    environment['SIMULATION'] = '1'
  for flag in ('REPLAY', 'SKIP_FW_QUERY', 'DISABLE_FW_CACHE'):
    environment.pop(flag, None)
  set_phase(output, 'pause')
  from card_qa.runtime_phase_fence import arm, startup
  arm(output, 0)
  pump_command = [os.sys.executable, str(ROOT / 'rust/tools/card_qa/runtime_pump.py'), '--prefix', prefix,
    '--inputs', str(arguments.evidence / (scenario.candidate + '-inputs.json')), '--control', str(output / 'pump-control'), '--evidence', str(output / 'pump')]
  pump_command.extend(['--cc-every', str(arguments.controls_every), '--can-interval', str(arguments.can_interval)])
  try:
    with ExitStack() as stack:
      subscribers = {name: messaging.sub_sock(name, conflate=False, timeout=100) for name in OUTPUTS}
      stdout = stack.enter_context((output / 'stdout.log').open('w'))
      stderr = stack.enter_context((output / 'stderr.log').open('w'))
      pump_log = stack.enter_context((output / 'pump.log').open('w'))
      pump = stack.enter_context(subprocess.Popen(pump_command, cwd=ROOT, env=environment, stdout=pump_log, stderr=pump_log))
      process = stack.enter_context(subprocess.Popen(command, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr))
      try:
        deadline = time.monotonic() + 10
        while not (output / 'pump/ready').exists():
          assert pump.poll() is None, pump.returncode
          assert time.monotonic() < deadline
          time.sleep(.001)
        set_phase(output, 'startup')
        startup_started = time.monotonic()
        initial = startup(process, pump, subscribers, output)
        startup_seconds = time.monotonic() - startup_started
        stored = (params / 'CarParams').read_bytes()
        with car.CarParams.from_bytes(stored) as cp:
          passive = cp.passive
          params_value = normalize(cp.to_dict())
        expected_passive = not scenario.enabled or scenario.candidate in (
          'MOCK', 'RAM_HD_5TH_GEN', 'SUBARU_OUTBACK_2023', 'SUBARU_FORESTER_PREGLOBAL', 'VOLKSWAGEN_PASSAT_NMS')
        assert passive == expected_passive
        assert params_value['carFingerprint'] == scenario.candidate
        maps = Path(f'/proc/{process.pid}/maps').read_text()
        (output / 'maps.txt').write_text(maps)
        if not source:
          assert 'libpython' not in maps
        context = RuntimePeer(process, pump, subscribers, output, passive, scenario.candidate, len(initial['carState']))
        warmup = phase_capture(context, 'warmup')
        trace = [json.loads(line) for line in (output / 'frequency.jsonl').read_text().splitlines()]
        sample = trace[-1]
        controls_ready = (params / 'ControlsReady').read_bytes() == b'1'
        assert passive or controls_ready, 'actual ControlsReady barrier'
        if scenario.candidate == 'GENESIS_G70':
          assert sample['parser']['counter'] > 200 and sample['parser']['pt_ready'] and sample['parser']['cam_ready'], sample
        (output / 'readiness.json').write_text(json.dumps({'controls_ready': controls_ready, 'passive': passive, 'sample': sample}) + '\n')
        context = RuntimePeer(process, pump, subscribers, output, passive, scenario.candidate, len(initial['carState']) + len(warmup['carState']))
        stream = phase_capture(context, 'stream')
        shutdown = finish(context, stream, source)
        cache_equal = (params / 'CarParamsCache').read_bytes() == stored == (params / 'CarParamsPersistent').read_bytes()
        final_ready = (params / 'ControlsReady').read_bytes()
        (output / 'lifecycle.json').write_text(json.dumps({'runtime_exit': shutdown['runtime_exit'], 'pump_exit': shutdown['pump_exit'],
          'empty_can_counter_increment': shutdown['empty_can_counter_increment'],
          'params_cache_persistent_equal': cache_equal, 'controls_ready': list(final_ready)}) + '\n')
        assert cache_equal
        assert final_ready == (b'0' if passive else b'1')
        if arguments.simulation:
          frequency = [json.loads(line) for line in (output / 'frequency.jsonl').read_text().splitlines()]
          assert any(row['average_frequency'] is not None and not row['min_frequency'] <= row['average_frequency'] <= row['max_frequency']
            and not row['tracker_valid'] and row['frequency_ok'] for row in frequency), 'existing simulation frequency branch not observed'
        result = {'params': params_value, 'warmup': warmup, 'stream': stream, 'passive': passive, 'command': command,
          'pump_command': pump_command, 'startup_seconds': startup_seconds}
        (output / 'capture.json').write_text(json.dumps(result) + '\n')
        return result
      finally:
        if process.poll() is None:
          process.send_signal(signal.SIGCONT)
          process.terminate()
          process.wait(timeout=5)
        if pump.poll() is None:
          pump.terminate()
          pump.wait(timeout=5)
  finally:
    if prior is None:
      os.environ.pop('OPENPILOT_PREFIX', None)
    else:
      os.environ['OPENPILOT_PREFIX'] = prior
    shutil.rmtree(queue)
