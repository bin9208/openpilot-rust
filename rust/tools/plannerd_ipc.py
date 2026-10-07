"""Owned original-C++ IPC peers compare the real Python and Rust planner daemons."""

import argparse
import gc
import hashlib
import importlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

from openpilot.cereal import log, messaging
from plannerd_owner_fixtures import SERVICES, car_params, messages, parameters
from plannerd_owner_source import difference

OUTPUTS = ['longitudinalPlan', 'lateralPlan', 'driverAssistance']


def policy(raw):
  with log.Event.from_bytes(raw) as event:
    service = event.which()
    value = getattr(event, service).to_dict()
    valid = bool(event.valid)
  for key in ('processingDelay', 'solverExecutionTime', 'plannerExecutionTime', 'fastRadarExecutionTime'):
    value.pop(key, None)
  return service, valid, value


def peers_ready(publishers, child):
  deadline = time.monotonic() + 15.0
  while time.monotonic() < deadline:
    if child.poll() is not None:
      raise RuntimeError(f'daemon exited before subscriptions: {child.returncode}')
    if all(socket.all_readers_updated() for socket in publishers.sock.values()):
      return
    time.sleep(0.01)
  raise TimeoutError('original CPP input peers did not observe every planner subscription')


def run(args, root, mode, epoch, prefix):
  os.environ['OPENPILOT_PREFIX'] = prefix
  publisher = messaging.PubMaster(SERVICES)
  case = root / f'{mode}-{epoch}'
  case.mkdir()
  params = case / 'params' / prefix
  params.mkdir(parents=True)
  values = parameters() | {'EnableRadarTracks': '1', 'MyDrivingModeAuto': '0'}
  for key, value in values.items():
    (params / key).write_text(value)
  cp = car_params()
  child_env = dict(
    os.environ,
    PARAMS_ROOT=str(params.parent),
    OPENPILOT_ROOT=str(Path.cwd()),
    PLANNER_ACADOS=str(args.artifact),
    SIMULATION='0',
    PYTHONDONTWRITEBYTECODE='1',
    OPENBLAS_NUM_THREADS='1',
  )
  command = (
    [str(args.python), '-P', 'rust/tools/plannerd_daemon_source.py', '--binding', str(args.binding), '--source-native', str(args.source_native)]
    if mode == 'source'
    else [str(args.binary), '--solver', str(args.artifact)]
  )
  records, timing = [], []
  subscribers = {}
  with (case / 'stdout.log').open('wb') as stdout, (case / 'stderr.log').open('wb') as stderr:
    child = subprocess.Popen(command, env=child_env, stdout=stdout, stderr=stderr)
    try:
      time.sleep(0.15)
      assert child.poll() is None, 'daemon failed while awaiting CarParams'
      pending = params / 'CarParams.pending'
      pending.write_bytes(cp.to_bytes())
      pending.rename(params / 'CarParams')
      peers_ready(publisher, child)
      subscribers = {name: messaging.sub_sock(name, conflate=False, timeout=0) for name in OUTPUTS}

      def drain():
        for name, socket in subscribers.items():
          while (raw := socket.receive(non_blocking=True)) is not None:
            with log.Event.from_bytes(raw) as event:
              timestamp = int(event.logMonoTime)
            path = case / f'{len(records):04}-{name}.bin'
            path.write_bytes(raw)
            service, valid, data = policy(raw)
            assert service == name
            records.append({'service': service, 'valid': valid, 'data': data, 'timestamp': timestamp, 'path': str(path)})

      def wait_for(name, previous):
        deadline = time.monotonic() + 2.0
        while time.monotonic() < deadline:
          drain()
          if sum(row['service'] == name for row in records) > previous:
            return
          if child.poll() is not None:
            raise RuntimeError(f'daemon exited while processing: {child.returncode}')
          time.sleep(0.001)
        raise TimeoutError(f'missing {name} publication')

      initial = messaging.new_message('liveTracks', logMonoTime=500_000_000, valid=True)
      initial.liveTracks.init('points', 0)
      publisher.send('liveTracks', initial)
      time.sleep(0.12)
      drain()
      assert not records, 'a liveTracks-only startup published without a model'
      started = time.monotonic() + 0.05
      for index in range(args.frames):
        target = started + index * 0.05
        time.sleep(max(0.0, target - time.monotonic()))
        packets = messages(messaging.new_message, 45 + index, 1.0 + index * 0.05)
        packets['selfdriveState'].selfdriveState.experimentalMode = 12 <= index < 16
        packets['carState'].carState.gasPressed = index == 22
        packets['controlsState'].controlsState.longControlState = 'off' if index == 24 else 'pid'
        if index >= 26:
          state = packets['carState'].carState
          state.vEgo = state.vEgoCluster = state.aEgo = 0.0
          state.standstill = True
          packets['controlsState'].controlsState.longControlState = 'stopping'
          for lead in (packets['radarState'].radarState.leadOne, packets['radarState'].radarState.leadTwo):
            lead.dRel = 15.0 + (0.2 if index >= 35 else 0.0)
            lead.vLead = lead.vLeadK = lead.vRel = 0.1
            lead.aLead = lead.aLeadK = lead.jLead = 0.0
        before = sum(row['service'] == 'driverAssistance' for row in records)
        for name, packet in packets.items():
          if name != 'modelV2':
            publisher.send(name, packet)
        publisher.send('modelV2', packets['modelV2'])
        wait_for('driverAssistance', before)
        model_done = time.monotonic()
        time.sleep(max(0.0, target + 0.025 - time.monotonic()))
        live = messaging.new_message('liveTracks', logMonoTime=1_000_000_000 + index * 50_000_000 + 25_000_000, valid=index != 19)
        points = live.liveTracks.init('points', 2)
        for point, lead in zip(points, (packets['radarState'].radarState.leadOne, packets['radarState'].radarState.leadTwo), strict=True):
          point.trackId, point.measured, point.radarSource = lead.radarTrackId, True, 'frontRadar'
          point.dRel, point.vRel, point.aRel, point.aLead, point.jLead = lead.dRel, lead.vRel, lead.aLeadK, lead.aLeadK, lead.jLead
        before = sum(row['service'] == 'longitudinalPlan' for row in records)
        publisher.send('liveTracks', live)
        if index >= 1 and not 12 <= index < 16:
          wait_for('longitudinalPlan', before)
        else:
          time.sleep(0.005)
          drain()
        timing.append({'index': index, 'target': target, 'model_done': model_done, 'live_done': time.monotonic()})
      time.sleep(0.03)
      drain()
      child.send_signal(signal.SIGTERM)
      status = child.wait(timeout=3.0)
      assert status == -signal.SIGTERM, status
    finally:
      if child.poll() is None:
        child.kill()
        child.wait(timeout=3.0)
  records.sort(key=lambda row: row['timestamp'])
  result = {
    'command': command,
    'returncode': child.returncode,
    'records': records,
    'timing': timing,
    'valid_counts': {name: sum(row['service'] == name and row['valid'] for row in records) for name in OUTPUTS},
    'triggers': {
      name: sum(row['service'] == 'longitudinalPlan' and row['data']['planningTrigger'] == name for row in records) for name in ('modelV2', 'liveTracks')
    },
    'fast_overlays': sum(row['service'] == 'longitudinalPlan' and row['data']['fastLeadMask'] != 0 for row in records),
  }
  (case / 'receipt.json').write_text(json.dumps(result, indent=2) + '\n')
  subscribers.clear()
  del publisher
  gc.collect()
  return result


def main():
  parser = argparse.ArgumentParser()
  for key in ('python', 'binary', 'artifact', 'binding', 'source-native', 'output'):
    parser.add_argument('--' + key, type=Path, required=True)
  parser.add_argument('--frames', type=int, default=40)
  args = parser.parse_args()
  for key in ('python', 'binary', 'artifact', 'binding', 'source_native', 'output'):
    setattr(args, key, getattr(args, key).absolute())
  args.output.mkdir(parents=True, exist_ok=False)
  results = []
  for mode in ('source', 'rust'):
    with tempfile.TemporaryDirectory(prefix='msgq_planner_', dir='/dev/shm') as shared:
      prefix = Path(shared).name.removeprefix('msgq_')
      for epoch in range(2):
        results.append(run(args, args.output, mode, epoch, prefix))

  def comparable(result):
    return [{key: row[key] for key in ('service', 'valid', 'data')} for row in result['records']]

  baseline = comparable(results[0])
  mismatches = [{'run': index, 'differences': difference(baseline, comparable(result))[:50]} for index, result in enumerate(results[1:], 1)]
  cpp = Path(importlib.import_module('msgq.ipc_pyx').__file__)
  identities = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in (args.binary, args.binding, cpp, args.artifact / 'manifest.json')}
  receipt = {
    'identities': identities,
    'runs': [
      {
        'returncode': result['returncode'],
        'messages': len(result['records']),
        'valid_counts': result['valid_counts'],
        'triggers': result['triggers'],
        'fast_overlays': result['fast_overlays'],
      }
      for result in results
    ],
    'comparisons': mismatches,
  }
  (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  assert all(not item['differences'] for item in mismatches), mismatches
  assert all(all(count > 0 for count in result['valid_counts'].values()) for result in results), 'no fully valid steady-state publication'
  assert all(all(count > 0 for count in result['triggers'].values()) and result['fast_overlays'] > 0 for result in results), (
    'missing trigger or overlay coverage'
  )
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()
