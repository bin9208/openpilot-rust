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
import stat
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


class InputCapture:
  """옵션 원본 관측만 소유한다. source/rust/epoch의 bytes를 합치거나 생성하지 않는다."""

  PACKET_LIMIT = 1024 * 1024
  PARAM_LIMIT = 64 * 1024
  PARAMS_LIMIT = 4 * 1024 * 1024
  RUN_LIMIT = 64 * 1024 * 1024
  TOTAL_LIMIT = 256 * 1024 * 1024
  MANIFEST_LIMIT = 1024 * 1024
  SOURCE_FILES = (
    'rust/tools/plannerd_ipc.py', 'rust/tools/plannerd_owner_fixtures.py',
    'openpilot/common/params_keys.h', 'openpilot/cereal/log.capnp',
    'openpilot/cereal/custom.capnp', 'opendbc_repo/opendbc/car/car.capnp',
    'openpilot/cereal/services.py', 'openpilot/selfdrive/modeld/constants.py',
    'openpilot/cereal/messaging/__init__.py',
  )

  @staticmethod
  def _identity(value):
    return (value.st_dev, value.st_ino, value.st_size, value.st_mode, value.st_mtime_ns, value.st_ctime_ns)

  @classmethod
  def _read(cls, path, limit):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    with os.fdopen(descriptor, 'rb') as stream:
      before = os.fstat(stream.fileno())
      if not stat.S_ISREG(before.st_mode) or before.st_size > limit:
        raise ValueError('capture input must be a bounded regular file')
      data = stream.read(limit + 1)
      if len(data) != before.st_size or cls._identity(before) != cls._identity(os.fstat(stream.fileno())):
        raise RuntimeError('capture input changed while reading')
      if cls._identity(before) != cls._identity(path.lstat()):
        raise RuntimeError('capture input path identity changed')
    return data

  @classmethod
  def _sha(cls, path):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    with os.fdopen(descriptor, 'rb') as stream:
      before = os.fstat(stream.fileno())
      if not stat.S_ISREG(before.st_mode) or before.st_size > 512 * 1024 * 1024:
        raise ValueError('capture identity must be a bounded regular file')
      digest = hashlib.sha256()
      count = 0
      while data := stream.read(65536):
        count += len(data)
        if count > 512 * 1024 * 1024:
          raise ValueError('capture identity exceeds bound')
        digest.update(data)
      if count != before.st_size or cls._identity(before) != cls._identity(os.fstat(stream.fileno())):
        raise RuntimeError('capture identity changed while hashing')
      if cls._identity(before) != cls._identity(path.lstat()):
        raise RuntimeError('capture identity path changed')
    return digest.hexdigest()

  @staticmethod
  def _mkdir(path):
    if path.parent.resolve(strict=True) != path.parent:
      raise ValueError('capture parent must be canonical')
    path.mkdir(mode=0o700, exist_ok=False)
    metadata = path.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or stat.S_IMODE(metadata.st_mode) != 0o700 or metadata.st_uid != os.getuid():
      raise ValueError('capture directory must be private and owned')

  @staticmethod
  def _create(path, data):
    if path.parent.resolve(strict=True) != path.parent:
      raise ValueError('capture file parent must be canonical')
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    with os.fdopen(descriptor, 'wb', buffering=0) as stream:
      metadata = os.fstat(stream.fileno())
      if metadata.st_uid != os.getuid() or stat.S_IMODE(metadata.st_mode) != 0o600:
        raise ValueError('capture file must be private and owned')
      remaining = memoryview(data)
      while remaining:
        count = stream.write(remaining)
        if not count:
          raise OSError('capture file write made no progress')
        remaining = remaining[count:]
      after = os.fstat(stream.fileno())
      if after.st_size != len(data) or InputCapture._identity(after) != InputCapture._identity(path.lstat()):
        raise RuntimeError('capture output identity changed while writing')

  @classmethod
  def _destination(cls, variable, leaf):
    setting = os.environ.get(variable)
    if setting is None:
      return None
    if not setting or not Path(setting).is_absolute():
      raise ValueError(f'{variable} must name an absolute owned capture path')
    job = Path(os.environ['CI_JOB_TEMP'])
    if (os.environ.get('GITHUB_ACTIONS') != 'true' or job.parent != Path('/tmp') or not job.name.startswith('openpilot-ci.')
        or len(job.name.removeprefix('openpilot-ci.')) != 6 or job.resolve(strict=True) != job):
      raise ValueError('capture requires the current owned Actions job')
    for directory in (job, job / 'proof'):
      metadata = directory.lstat()
      if (not stat.S_ISDIR(metadata.st_mode) or stat.S_IMODE(metadata.st_mode) != 0o700
          or metadata.st_uid != os.getuid() or directory.resolve(strict=True) != directory):
        raise ValueError('capture requires private canonical job/proof directories')
    owner = ':'.join(os.environ[name] for name in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
    if cls._read(job / 'owner', 512).decode() != owner:
      raise ValueError('capture job ownership differs')
    path = Path(setting)
    if path != job / 'proof' / leaf:
      raise ValueError('capture path is outside its exact owned proof destination')
    return path

  @classmethod
  def from_environment(cls, frames):
    directory = cls._destination('PLANNER_IPC_CAPTURE_DIR', 'planner-ipc-original-inputs')
    if directory is None:
      return None
    if frames != 40:
      raise ValueError('IPC capture requires the existing four runs of forty frames')
    root = Path(__file__).resolve().parents[2]
    source = {name: cls._sha(root / name) for name in cls.SOURCE_FILES}
    executable = Path(sys.executable).resolve(strict=True)
    numpy = sys.modules.get('numpy')
    if numpy is None:
      raise RuntimeError('actual IPC NumPy module is required for capture provenance')
    commit = os.environ['GITHUB_SHA']
    if len(commit) != 40 or any(character not in '0123456789abcdef' for character in commit):
      raise ValueError('capture source SHA is invalid')
    provenance = {
      'python_executable': sys.executable, 'python_executable_canonical': str(executable),
      'python_executable_sha256': cls._sha(executable), 'python_version': sys.version,
      'numpy_version': numpy.__version__, 'origin_run_id': os.environ['GITHUB_RUN_ID'],
      'origin_run_attempt': os.environ['GITHUB_RUN_ATTEMPT'], 'origin_job': os.environ['GITHUB_JOB'],
      'host_architecture': os.uname().machine, 'input_transport': 'actual-original-cpp-pubmaster',
    }
    return cls(directory, commit, source, provenance, root)

  def __init__(self, directory, commit, source, provenance, source_root=None):
    self._mkdir(directory)
    self.directory, self.commit, self.source = directory, commit, dict(source)
    self.provenance, self.source_root = dict(provenance), source_root
    self.total, self.runs, self.failed = 0, [], False

  def _write(self, directory, relative, data, limit, run=None):
    if self.failed:
      raise RuntimeError('capture previously failed')
    try:
      if not isinstance(data, bytes) or len(data) > limit or self.total + len(data) > self.TOTAL_LIMIT:
        raise ValueError('capture byte bound exceeded')
      if run is not None and run.total + len(data) > self.RUN_LIMIT:
        raise ValueError('per-run capture byte bound exceeded')
      self._create(directory / relative, data)
      self.total += len(data)
      if run is not None:
        run.total += len(data)
      return {'path': relative, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
    except BaseException:
      self.failed = True
      raise

  def _verify_sources(self):
    if self.source_root is not None:
      for name, digest in self.source.items():
        if self._sha(self.source_root / name) != digest:
          raise RuntimeError('original capture producer source changed')

  def begin_run(self, mode, epoch, prefix, params, command):
    expected = (('source', 0), ('source', 1), ('rust', 0), ('rust', 1))
    if len(self.runs) >= 4 or (mode, epoch) != expected[len(self.runs)]:
      raise ValueError('capture run order differs from the actual IPC caller')
    run = self.Run(self, mode, epoch, prefix, params, command)
    self.runs.append(run)
    return run

  def finish(self, output_receipt):
    if self.failed or len(self.runs) != 4 or not all(run.complete for run in self.runs):
      raise RuntimeError('incomplete original IPC capture')
    self._verify_sources()
    index = {'schema_version': 1, 'kind': 'independent-original-planner-ipc-run-index',
             'status': 'COMPLETE', 'source_commit': self.commit, 'source_sha256': self.source,
             'runs': [{'mode': run.mode, 'epoch': run.epoch, 'events': run.events,
                       'params_files': len(run.params), 'manifest': run.manifest} for run in self.runs],
             'events': sum(run.events for run in self.runs), 'frames': 160, 'car_params_files': 4,
             'actual_output_receipt_sha256': self._sha(output_receipt)}
    data = (json.dumps(index, indent=2, sort_keys=True) + '\n').encode()
    self._write(self.directory, 'index.json', data, self.MANIFEST_LIMIT)

  @classmethod
  def record_parser_observations(cls, observation):
    directory = cls._destination('PLANNER_SCHEDULER_PARSE_CAPTURE_DIR', 'planner-scheduler-original-parse')
    if directory is None:
      return
    data = (json.dumps(observation, indent=2, sort_keys=True) + '\n').encode()
    if len(data) > 65536:
      raise ValueError('parser-stage capture exceeds 64 KiB')
    cls._mkdir(directory)
    cls._create(directory / 'observations.json', data)

  class Run:
    def __init__(self, owner, mode, epoch, prefix, params, command):
      self.owner, self.mode, self.epoch = owner, mode, epoch
      self.directory = owner.directory / f'{mode}-{epoch}'
      owner._mkdir(self.directory)
      owner._mkdir(self.directory / 'events')
      owner._mkdir(self.directory / 'params')
      self.total, self.events, self.frames, self.params = 0, 0, [], {}
      self.initial_live, self.car_params, self.manifest, self.complete = None, None, None, False
      self.provenance = owner.provenance | {'mode': mode, 'epoch': str(epoch), 'openpilot_prefix': prefix,
                                             'daemon_command_json': json.dumps(command), 'params_snapshot': 'before-daemon-launch'}
      entries = sorted(params.iterdir())
      if len(entries) > 2048:
        raise ValueError('too many initial Params files')
      total = 0
      for path in entries:
        if (not path.name or not path.name.isascii() or not path.name.replace('_', '').isalnum()
            or path.name in ('CarParams', 'CarParams.pending')):
          raise ValueError('unexpected prelaunch Params entry')
        data = owner._read(path, owner.PARAM_LIMIT)
        total += len(data)
        if total > owner.PARAMS_LIMIT:
          raise ValueError('initial Params snapshot exceeds bound')
        self.params[path.name] = owner._write(self.directory, f'params/{path.name}', data, owner.PARAM_LIMIT, self)

    def pending(self, path):
      if self.car_params is not None:
        raise ValueError('duplicate pending CarParams capture')
      self.car_params = self.owner._write(self.directory, 'CarParams.pending.bin',
                                          self.owner._read(path, self.owner.PACKET_LIMIT), self.owner.PACKET_LIMIT, self)

    def send(self, publisher, service, packet, index=None):
      if self.complete or self.car_params is None:
        raise RuntimeError('capture run is not ready for input')
      if index is None:
        if self.events or service != 'liveTracks':
          raise ValueError('initial captured publication differs')
      else:
        if not 0 <= index < 40 or self.initial_live is None:
          raise ValueError('captured frame index differs')
        if index == len(self.frames):
          if self.frames and self.frames[-1]['live'] is None:
            raise ValueError('previous captured frame is incomplete')
          self.frames.append({'index': index, 'packets': [], 'live': None})
        if index != len(self.frames) - 1:
          raise ValueError('captured frames must be sequential')
        frame = self.frames[index]
        order = [name for name in SERVICES if name not in ('liveTracks', 'modelV2')] + ['modelV2']
        if service == 'liveTracks':
          if len(frame['packets']) != len(order) or frame['live'] is not None:
            raise ValueError('captured liveTracks order differs')
        elif len(frame['packets']) >= len(order) or service != order[len(frame['packets'])]:
          raise ValueError('captured model packet order differs')
      # PubMaster.send와 정확히 같은 한 번의 직렬화다. 그 동일 bytes 객체를 원래 publisher에 전달한다.
      data = packet if isinstance(packet, bytes) else packet.to_bytes()
      raw = self.owner._write(self.directory, f'events/{self.events:04}-{service}.bin', data, self.owner.PACKET_LIMIT, self)
      publisher.send(service, data)
      self.events += 1
      if index is None:
        self.initial_live = raw
      elif service == 'liveTracks':
        frame['live'] = raw
      else:
        frame['packets'].append({'service': service, 'raw': raw})

    def finish(self):
      if (self.complete or self.events != 401 or self.car_params is None or self.initial_live is None
          or len(self.frames) != 40 or any(len(frame['packets']) != 9 or frame['live'] is None for frame in self.frames)):
        raise RuntimeError('original IPC run capture is incomplete')
      self.owner._verify_sources()
      manifest = {'schema_version': 1, 'kind': 'independent-original-planner-ipc-inputs',
                  'source_commit': self.owner.commit, 'source_sha256': self.owner.source,
                  'capture_provenance': self.provenance, 'params': self.params, 'car_params': self.car_params,
                  'initial_live': self.initial_live, 'frames': self.frames}
      data = (json.dumps(manifest, indent=2, sort_keys=True) + '\n').encode()
      raw = self.owner._write(self.directory, 'manifest.json', data, self.owner.MANIFEST_LIMIT, self)
      self.manifest = raw | {'path': f'{self.mode}-{self.epoch}/manifest.json'}
      self.complete = True


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


def run(args, root, mode, epoch, prefix, capture=None):
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
  input_capture = capture.begin_run(mode, epoch, prefix, params, command) if capture is not None else None
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
      if input_capture is not None:
        input_capture.pending(pending)
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
      if input_capture is None:
        publisher.send('liveTracks', initial)
      else:
        input_capture.send(publisher, 'liveTracks', initial)
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
            if input_capture is None:
              publisher.send(name, packet)
            else:
              input_capture.send(publisher, name, packet, index)
        if input_capture is None:
          publisher.send('modelV2', packets['modelV2'])
        else:
          input_capture.send(publisher, 'modelV2', packets['modelV2'], index)
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
        if input_capture is None:
          publisher.send('liveTracks', live)
        else:
          input_capture.send(publisher, 'liveTracks', live, index)
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
  if input_capture is not None:
    input_capture.finish()
  return result


def main():
  parser = argparse.ArgumentParser()
  for key in ('python', 'binary', 'artifact', 'binding', 'source-native', 'output'):
    parser.add_argument('--' + key, type=Path, required=True)
  parser.add_argument('--frames', type=int, default=40)
  args = parser.parse_args()
  for key in ('python', 'binary', 'artifact', 'binding', 'source_native', 'output'):
    setattr(args, key, getattr(args, key).absolute())
  capture = InputCapture.from_environment(args.frames)
  args.output.mkdir(parents=True, exist_ok=False)
  results = []
  for mode in ('source', 'rust'):
    with tempfile.TemporaryDirectory(prefix='msgq_planner_', dir='/dev/shm') as shared:
      prefix = Path(shared).name.removeprefix('msgq_')
      for epoch in range(2):
        results.append(run(args, args.output, mode, epoch, prefix, capture=capture))

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
  if capture is not None:
    capture.finish(args.output / 'receipt.json')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()
