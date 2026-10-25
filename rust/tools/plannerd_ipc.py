"""Owned original-C++ IPC peers compare the real Python and Rust planner daemons."""

import argparse
import gc
import hashlib
import importlib
import json
import math
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import time

from openpilot.cereal import log, messaging
from plannerd_owner_fixtures import SERVICES, car_params, messages, parameters
from plannerd_owner_source import difference

OUTPUTS = ['longitudinalPlan', 'lateralPlan', 'driverAssistance']


def next_model_target(nominal, previous_sent, previous_model_done):
  target = max(nominal, previous_sent + 0.05)
  if previous_model_done - previous_sent >= 0.05:
    target = max(target, previous_model_done + 0.05)
  return target


class ModelScheduler:
  """선택된 native endpoint는 run당 한 번 열고 오류를 oracle로 대체하지 않는다."""

  def __init__(self, stderr_path):
    self.binary = os.environ.get('PLANNER_NATIVE_SCHEDULER')
    self.stderr_path = stderr_path
    self.process = None
    self.stderr = None
    self.failed = False

  def __enter__(self):
    if self.binary is None:
      return self
    if not self.binary:
      raise RuntimeError('PLANNER_NATIVE_SCHEDULER must name an executable')
    self.stderr = self.stderr_path.open('wb')
    try:
      self.process = subprocess.Popen([self.binary, 'schedule'], stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE, stderr=self.stderr, bufsize=0)
      os.set_blocking(self.process.stdin.fileno(), False)
      os.set_blocking(self.process.stdout.fileno(), False)
    except BaseException:
      # Popen 성공 뒤 pipe 설정 실패도 child를 남기지 않는다.
      self.__exit__(*sys.exc_info())
      raise
    return self

  @staticmethod
  def _ready(descriptor, reading, deadline):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
      raise TimeoutError('native scheduler response exceeded 2 seconds')
    readers, writers, _ = select.select(
      [descriptor] if reading else [], [] if reading else [descriptor], [], remaining,
    )
    if not (readers if reading else writers):
      raise TimeoutError('native scheduler response exceeded 2 seconds')

  def target(self, nominal, previous_sent, previous_model_done):
    if self.binary is None:
      return next_model_target(nominal, previous_sent, previous_model_done)
    try:
      if self.failed or self.process is None or self.process.poll() is not None:
        raise RuntimeError('native scheduler is not running')
      if not all(math.isfinite(value) for value in (nominal, previous_sent, previous_model_done)):
        raise ValueError('native scheduler wire timestamps must be finite')
      deadline = time.monotonic() + 2.0
      request = f'{nominal!r}\t{previous_sent!r}\t{previous_model_done!r}\n'.encode('ascii')
      descriptor = self.process.stdin.fileno()
      while request:
        self._ready(descriptor, False, deadline)
        try:
          count = os.write(descriptor, request)
        except BlockingIOError:
          continue
        if count == 0:
          raise RuntimeError('native scheduler request pipe closed')
        request = request[count:]
      response = bytearray()
      descriptor = self.process.stdout.fileno()
      while b'\n' not in response:
        if len(response) >= 256:
          raise RuntimeError('native scheduler response is too long')
        self._ready(descriptor, True, deadline)
        try:
          chunk = os.read(descriptor, 256 - len(response))
        except BlockingIOError:
          continue
        if not chunk:
          raise RuntimeError('native scheduler EOF before target')
        response.extend(chunk)
      line, _, extra = response.partition(b'\n')
      if extra:
        raise RuntimeError('native scheduler returned multiple responses')
      target = float(line.decode('ascii'))
      if not math.isfinite(target):
        raise RuntimeError('native scheduler returned a nonfinite target')
      return target
    except BaseException:
      self.failed = True
      raise

  def __exit__(self, exc_type, _exc, _traceback):
    process = self.process
    try:
      if process is not None:
        deadline = time.monotonic() + 3.0
        try:
          process.stdin.close()
        except OSError:
          pass
        if (exc_type is not None or self.failed) and process.poll() is None:
          process.terminate()
        forced = False
        try:
          status = process.wait(timeout=min(1.0, max(0.0, deadline - time.monotonic())))
        except subprocess.TimeoutExpired:
          forced = True
          process.terminate()
          try:
            status = process.wait(timeout=min(1.0, max(0.0, deadline - time.monotonic())))
          except subprocess.TimeoutExpired:
            process.kill()
            status = process.wait(timeout=max(0.0, deadline - time.monotonic()))
        if exc_type is None and (self.failed or forced or status != 0):
          raise RuntimeError(f'native scheduler did not close normally: {status}')
    finally:
      if process is not None:
        for stream in (process.stdin, process.stdout):
          if stream is not None:
            stream.close()
      if self.stderr is not None:
        self.stderr.close()
    return False


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
  with (case / 'stdout.log').open('wb') as stdout, (case / 'stderr.log').open('wb') as stderr, \
       ModelScheduler(case / 'scheduler.stderr.log') as scheduler:
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
      previous_model_done = started - 0.05
      previous_sent = previous_model_done
      for index in range(args.frames):
        # Pace from the previous send, so ordinary solver work does not slow
        # the source's inputs relative to the native candidate. If processing
        # overruns a complete period, recover without a catch-up burst.
        target = scheduler.target(started + index * 0.05, previous_sent, previous_model_done)
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
        sent = time.monotonic()
        for name, packet in packets.items():
          if name != 'modelV2':
            publisher.send(name, packet)
        publisher.send('modelV2', packets['modelV2'])
        wait_for('driverAssistance', before)
        model_done = time.monotonic()
        previous_model_done = model_done
        previous_sent = sent
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
        timing.append({'index': index, 'target': target, 'sent': sent, 'model_done': model_done, 'live_done': time.monotonic()})
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
