from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
import uuid

from openpilot.cereal import car, messaging
import numpy as np
from card_vehicle_source import normalize
from check_card_vehicle import compare
from card_runtime_source import load_binding
from card_qa.runtime_inputs import Scenario, frames

ROOT = Path(__file__).resolve().parents[2]
SERVICES = ('pandaStates', 'carControl', 'onroadEvents', 'carrotMan', 'longitudinalPlan', 'radarState', 'modelV2', 'drivingModelData', 'customReservedRawData0')
OUTPUTS = ('carParams', 'carOutput', 'carState', 'sendcan')
WARMUP = 250
STEPS = WARMUP + 81


def wire(name: str, values, valid: bool = True) -> bytes:
  message = messaging.new_message(name, len(values) if isinstance(values, list) else None)
  message.valid = valid
  if isinstance(values, list):
    setattr(message, name, values)
  else:
    getattr(message, name).from_dict(values)
  return message.to_bytes()


def can_packet(frames: list[dict], timestamp: int) -> bytes:
  message = messaging.new_message('can', len(frames))
  message.logMonoTime = timestamp
  for output, frame in zip(message.can, frames, strict=True):
    output.address = frame['address']; output.src = frame['bus']; output.dat = bytes(frame['data'])
  return message.to_bytes()


def decoded_event(raw: bytes) -> dict:
  with messaging.log.Event.from_bytes(raw) as message:
    value = message.to_dict()
    if message.which() == 'sendcan':
      for frame in value['sendcan']: frame['dat'] = list(frame['dat'])
    return normalize(value)


def state_socket(socket):
  raw = socket.receive()
  assert raw is not None, 'card publication timed out'
  return decoded_event(raw)



def capture(arguments, scenario: Scenario, source: bool) -> dict:
  candidate, enabled = scenario.candidate, scenario.enabled
  mode = 'source' if source else 'native'
  output = arguments.evidence / f'{candidate}-{enabled}-{mode}'
  output.mkdir(parents=True)
  for tick in range(81):
    frames(candidate, tick, arguments.corpus_root)
  prefix = 'cardruntime_' + uuid.uuid4().hex
  queue = Path('/dev/shm') / ('msgq_' + prefix)
  queue.mkdir()
  params_root = output / 'params'; params = params_root / prefix; params.mkdir(parents=True)
  values = dict(OpenpilotEnabledToggle='1' if enabled else '0', DisengageOnAccelerator='1', IsMetric='1',
                LongitudinalPersonality='1', AutoEngage='0', ControlsReady='0', GitRemote='https://example.com/owned/repo.git', GitBranch='fixture', GitCommitDate='owned-date')
  command = [str(arguments.binary), '--root', str(ROOT), '--numerics', str(arguments.numerics), '--max-steps', str(STEPS + 100)]
  command.extend(['--frequency-trace',str(output/'frequency.jsonl')])
  if source:
    command = [os.sys.executable, str(ROOT / 'rust/tools/card_runtime_source.py'), '--binding', str(arguments.binding), '--params-root', str(params_root), '--steps', str(STEPS + 100)]
    command.extend(['--frequency-trace',str(output/'frequency.jsonl')])
  environment = dict(os.environ, OPENPILOT_PREFIX=prefix, PARAMS_ROOT=str(params_root), FINGERPRINT=candidate)
  if arguments.simulation:environment['SIMULATION']='1'
  for flag in ('REPLAY', 'SKIP_FW_QUERY', 'DISABLE_FW_CACHE'): environment.pop(flag, None)
  prior = os.environ.get('OPENPILOT_PREFIX')
  os.environ['OPENPILOT_PREFIX'] = prefix
  try:
    from openpilot.common.params import Params
    settings = Params(str(params_root))
    for key in settings.all_keys():
      default = settings.get_default_value(key)
      if default is not None: settings.put(key, default)
    for key, value in values.items(): (params / key).write_text(value)
    with ExitStack() as stack:
      pubs = {name: messaging.pub_sock(name) for name in SERVICES}
      pubs['can'] = messaging.pub_sock('can')
      subs = {name: messaging.sub_sock(name, conflate=False, timeout=3000) for name in OUTPUTS}
      stdout = stack.enter_context((output / 'stdout.log').open('w'))
      stderr = stack.enter_context((output / 'stderr.log').open('w'))
      process = stack.enter_context(subprocess.Popen(command, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr))
      try:
        pubs['can'].wait_for_readers(timeout=5); pubs['pandaStates'].wait_for_readers(timeout=5)
        pubs['pandaStates'].send(wire('pandaStates', [{}]))
        startup_started = time.monotonic()
        deadline = startup_started + (60 if arguments.elf else 10)
        startup_frames = [dict(address=0x123, data=[0] * 8, bus=0)]
        buffered = {}
        while not (params / 'CarParams').exists():
          assert process.poll() is None, (mode, process.returncode)
          assert time.monotonic() < deadline, 'card startup timed out'
          for name, socket in subs.items():
            if raw := socket.receive(non_blocking=True): buffered[name] = decoded_event(raw)
          pubs['can'].send(can_packet(startup_frames, time.monotonic_ns())); time.sleep(.003)
        startup_seconds = time.monotonic() - startup_started
        initial = {name: buffered[name] if name in buffered else state_socket(subs[name]) for name in ('carParams', 'carOutput', 'carState')}
        process.send_signal(signal.SIGSTOP)
        assert os.WIFSTOPPED(os.waitpid(process.pid, os.WUNTRACED)[1])
        for name, socket in subs.items():
          while raw := socket.receive(non_blocking=True):
            if name == 'carState':
              with messaging.log.Event.from_bytes(raw) as event: initial[name] = normalize(event.to_dict())
        (output / 'initial.json').write_text(json.dumps(initial) + '\n')
        stored = (params / 'CarParams').read_bytes()
        with car.CarParams.from_bytes(stored) as cp:
          passive = cp.passive
          params_value = normalize(cp.to_dict())
        assert passive == (not enabled or candidate == 'MOCK')
        assert params_value['carFingerprint'] == candidate
        maps = Path(f'/proc/{process.pid}/maps').read_text(); (output / 'maps.txt').write_text(maps)
        if not source: assert 'libpython' not in maps
        events = []
        next_tick = time.monotonic()
        for tick in range(STEPS - 1):
          if tick == WARMUP:
            deadline = time.monotonic() + 3
            while True:
              lines = (output / 'frequency.jsonl').read_text().split('\n')[:-1]
              sample = json.loads(lines[-1]) if lines else None
              initialized = passive or (params / 'ControlsReady').read_bytes() == b'1'
              parser_ready = candidate != 'GENESIS_G70' or (sample is not None and sample['parser']['counter'] > 200 and sample['parser']['pt_ready'] and sample['parser']['cam_ready'])
              if initialized and parser_ready:
                (output / 'readiness.json').write_text(json.dumps(dict(warmup_steps=WARMUP, sample=sample, controls_ready=(params / 'ControlsReady').read_bytes() == b'1', passive=passive)) + '\n')
                break
              assert time.monotonic() < deadline, 'ControlsReady/parser readiness barrier not reached'
              time.sleep(.001)
          remaining = next_tick - time.monotonic()
          if remaining > 0: time.sleep(remaining)
          measured_tick = tick - WARMUP
          control = dict(enabled=20 <= measured_tick < 65, latActive=20 <= measured_tick < 65, longActive=20 <= measured_tick < 65,
                         actuators=dict(torque=(measured_tick % 20 - 10) / 10 if measured_tick >= 0 else 0., accel=.5, steeringAngleDeg=2.), hudControl=dict(setSpeed=20.))
          if tick < 20 or tick % (10 if arguments.simulation else 2) == 0:
            pubs['carControl'].send(wire('carControl', control, valid=measured_tick < 0 or measured_tick % 11 != 0))
          pubs['onroadEvents'].send(wire('onroadEvents', []))
          pubs['modelV2'].send(wire('modelV2', {})); pubs['radarState'].send(wire('radarState', {}))
          pubs['longitudinalPlan'].send(wire('longitudinalPlan', dict(xState=0)))
          pubs['drivingModelData'].send(wire('drivingModelData', {}))
          input_tick = tick % 40 if tick < WARMUP else measured_tick
          timestamp = time.monotonic_ns(); pubs['can'].send(can_packet(frames(candidate, input_tick, arguments.corpus_root), timestamp))
          if tick == 0: process.send_signal(signal.SIGCONT)
          publication = {name: state_socket(subs[name]) for name in ('carOutput', 'carState')}
          state = publication['carState']['carState']; radar = state['radarInput']
          assert radar['firstCanMonoTime'] == radar['lastCanMonoTime'] == timestamp
          assert radar['canPacketCount'] == 1
          assert state['canErrorCounter'] == initial['carState']['carState']['canErrorCounter'], (tick, state['canErrorCounter'], initial['carState']['carState']['canErrorCounter'])
          assert radar['receiveMonoTime'] > 0
          if not passive: publication['sendcan'] = state_socket(subs['sendcan'])
          for event in publication.values(): event.pop('logMonoTime')
          state.pop('cumLagMs'); state.pop('radarInput')
          events.append(publication)
          next_tick = time.monotonic() + .005
        assert any(row['carOutput']['valid'] for row in events)
        assert any(not row['carOutput']['valid'] for row in events)
        if candidate != 'MOCK': assert any(row['carState']['carState']['canValid'] for row in events)
        process.send_signal(signal.SIGINT)
        assert process.wait(timeout=5) == (-signal.SIGINT if source else 130), (mode, process.returncode)
        if arguments.simulation:
          frequency=[json.loads(line) for line in (output/'frequency.jsonl').read_text().splitlines()]
          assert any(row['interval'] is not None and row['average_frequency'] is not None and
            not row['min_frequency'] <= row['average_frequency'] <= row['max_frequency'] and
            not row['tracker_valid'] and row['frequency_ok'] for row in frequency), 'existing simulation frequency branch was not observed'
        assert (params / 'CarParamsCache').read_bytes() == stored == (params / 'CarParamsPersistent').read_bytes()
        assert (params / 'ControlsReady').read_bytes() == (b'0' if passive else b'1')
        result = dict(params=params_value, events=events[WARMUP:], warmup=events[:WARMUP], passive=passive, command=command,
                      startup_seconds=startup_seconds)
        (output / 'capture.json').write_text(json.dumps(result) + '\n')
        (output / 'initial.json').write_text(json.dumps(initial) + '\n')
        return result
      finally:
        if process.poll() is None: process.send_signal(signal.SIGCONT); process.terminate(); process.wait(timeout=5)
  finally:
    if prior is None: os.environ.pop('OPENPILOT_PREFIX', None)
    else: os.environ['OPENPILOT_PREFIX'] = prior
    if queue.exists(): shutil.rmtree(queue)


def main() -> None:
  assert np.__version__ == '2.5.3', np.__version__
  parser = argparse.ArgumentParser()
  for name in ('binary', 'numerics', 'binding', 'evidence'): parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--elf', type=Path)
  parser.add_argument('--simulation',action='store_true')
  parser.add_argument('--corpus-root', type=Path)
  arguments = parser.parse_args(); arguments.evidence.mkdir(parents=True, exist_ok=True)
  load_binding(arguments.binding)
  rows = []
  for candidate, enabled in (('COMMA_BODY', True), ('COMMA_BODY', False), ('MOCK', True), ('GENESIS_G70', True), ('TESLA_MODEL_3', True), ('TESLA_MODEL_Y', True)):
    scenario = Scenario(candidate, enabled)
    source = capture(arguments, scenario, True)
    native = capture(arguments, scenario, False)
    compare(source['params'], native['params']); compare(source['events'], native['events'])
    rows.append(dict(candidate=candidate, enabled=enabled, steps=len(native['events']), passive=native['passive']))
  result = dict(status='pass', scenarios=rows, runtime_python=False, binary_sha256=hashlib.sha256(arguments.binary.read_bytes()).hexdigest(),
      executed_elf_sha256=hashlib.sha256((arguments.elf or arguments.binary).read_bytes()).hexdigest(),
      simulation=arguments.simulation,
      observable='unchanged original Car constructor/interface/cruise/core against native CLI using owned original msgq peers; complete CarParams, CarState, prior actuator output, CAN bytes/validity, async Params drain',
      scope='owned SIGSTOP startup/readiness barrier; 250 real IPC warmup steps retained before 80 compared steps; synthetic CAN paced at5ms, CC every2ticks(normal) or10ticks(simulation); production loop unchanged; clocks/cumLagMs excluded; CAN timestamps/counts and no additional timeouts checked throughout; SIGINT cleanup; no physical CAN/device')
  (arguments.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n'); print(json.dumps(result))


if __name__ == '__main__': main()
