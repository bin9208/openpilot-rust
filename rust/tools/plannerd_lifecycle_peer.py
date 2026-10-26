from __future__ import annotations

from dataclasses import dataclass
import gc
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
from typing import Literal

from openpilot.cereal import messaging
from plannerd_ipc import OUTPUTS, peers_ready, policy
from plannerd_owner_fixtures import SERVICES, car_params, messages, parameters

ROOT = Path(__file__).resolve().parents[2]


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  phase: Literal["wait", "ready", "recover", "integer", "float", "malformed"]
  signal: int = signal.SIGTERM
  car_params: Literal["missing", "directory", "empty"] = "missing"
  directories: tuple[str, ...] = ()


class LifecycleCapture:
  """옵션 원본 관측. 별도 실행/expected 생성 없이 같은 송신 bytes를 보존한다."""

  LIMIT = 64 * 1024**2
  PACKET_LIMIT = 1024**2
  PARAM_LIMIT = 64 * 1024
  PARAMS_LIMIT = 4 * 1024**2
  MEMBER_LIMIT = 62000
  SEND_ORDER = tuple(name for name in SERVICES if name not in ('liveTracks', 'modelV2')) + ('modelV2',)
  SOURCES = (
    'rust/tools/check_plannerd_lifecycle.py', 'rust/tools/plannerd_lifecycle_peer.py',
    'rust/tools/plannerd_ipc.py', 'rust/tools/plannerd_owner_fixtures.py', 'rust/tools/plannerd_daemon_source.py',
    'rust/tools/plannerd_owner_loader.py',
    'openpilot/selfdrive/controls/plannerd.py', 'openpilot/common/params_keys.h',
    'openpilot/cereal/log.capnp', 'openpilot/cereal/custom.capnp', 'opendbc_repo/opendbc/car/car.capnp',
    'openpilot/cereal/services.py', 'openpilot/selfdrive/modeld/constants.py', 'openpilot/cereal/messaging/__init__.py',
  )

  @classmethod
  def from_environment(cls, args, cases):
    setting = os.environ.get('PLANNER_LIFECYCLE_CAPTURE_DIR')
    if setting is None:
      return None
    try:
      return cls(args, cases, setting)
    except Exception as error:
      raise RuntimeError(f'lifecycle capture setup failed: {error!r}') from error

  @staticmethod
  def identity(value):
    return (value.st_dev, value.st_ino, value.st_mode, value.st_uid, value.st_nlink,
            value.st_size, value.st_mtime_ns, value.st_ctime_ns)

  @classmethod
  def read(cls, path, limit):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    with os.fdopen(descriptor, 'rb') as stream:
      before = os.fstat(stream.fileno())
      if not stat.S_ISREG(before.st_mode) or before.st_size > limit:
        raise ValueError('capture input must be a bounded regular file')
      data = stream.read(limit + 1)
      if (len(data) != before.st_size or cls.identity(before) != cls.identity(os.fstat(stream.fileno()))
          or cls.identity(before) != cls.identity(path.lstat())):
        raise RuntimeError('capture input identity changed')
      return data

  @classmethod
  def hashed(cls, path):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    with os.fdopen(descriptor, 'rb') as stream:
      before = os.fstat(stream.fileno())
      if not stat.S_ISREG(before.st_mode) or before.st_size > 512 * 1024**2:
        raise ValueError('capture identity exceeds regular-file bound')
      digest = hashlib.sha256()
      size = 0
      while data := stream.read(65536):
        size += len(data)
        if size > 512 * 1024**2:
          raise ValueError('capture identity exceeds hash bound')
        digest.update(data)
      if (size != before.st_size or cls.identity(before) != cls.identity(os.fstat(stream.fileno()))
          or cls.identity(before) != cls.identity(path.lstat())):
        raise RuntimeError('capture identity changed while hashing')
      return {'path': str(path), 'bytes': size, 'sha256': digest.hexdigest()}, cls.identity(before)

  @staticmethod
  def owned_directory(path):
    value = path.lstat()
    if (not stat.S_ISDIR(value.st_mode) or stat.S_IMODE(value.st_mode) != 0o700
        or value.st_uid != os.getuid() or path.resolve(strict=True) != path):
      raise ValueError('capture requires a canonical owned mode-700 directory')
    return value.st_dev, value.st_ino

  def __init__(self, args, cases, setting):
    job = Path(os.environ['CI_JOB_TEMP'])
    suffix = job.name.removeprefix('openpilot-ci.')
    if (os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('GITHUB_REPOSITORY') != 'bin9208/openpilot-rust'
        or os.environ.get('GITHUB_JOB') != 'planner-runtime' or job.parent != Path('/tmp')
        or not job.name.startswith('openpilot-ci.') or len(suffix) != 6 or not suffix.isascii() or not suffix.isalnum()):
      raise ValueError('capture requires the current owned Planner Actions job')
    parent = job / 'planner-comparison'
    self.directory = parent / 'lifecycle-original-inputs'
    if setting != str(self.directory) or args.output != parent / 'lifecycle':
      raise ValueError('PLANNER_LIFECYCLE_CAPTURE_DIR must name the exact lifecycle sibling')
    self.parents = {path: self.owned_directory(path) for path in (job, parent)}
    owner = ':'.join(os.environ[key] for key in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
    if self.read(job / 'owner', 512) != owner.encode():
      raise ValueError('capture job ownership differs')
    commit = os.environ['GITHUB_SHA']
    if len(commit) != 40 or any(char not in '0123456789abcdef' for char in commit):
      raise ValueError('capture source SHA must be the actual Actions commit')
    if ROOT.resolve(strict=True) != Path(os.environ['CI296_SOURCE']) or Path.cwd() != ROOT:
      raise ValueError('capture producer must run in the sealed source root')
    free = shutil.disk_usage(parent).free
    if free < 128 * 1024**2 + self.LIMIT:
      raise ValueError('lifecycle capture requires the existing 128MiB plus 64MiB growth admission')
    self.expected = [(case.name, mode) for case in cases for mode in ('source', 'rust')]
    if len(self.expected) != 26 or len(set(self.expected)) != 26:
      raise ValueError('capture requires the original thirteen source/native cases')
    self.pins = {}
    self.pin(job / 'owner')
    source = {name: self.pin(ROOT / name)['sha256'] for name in self.SOURCES}
    module = sys.modules.get('msgq.ipc_pyx')
    if module is None or not getattr(module, '__file__', None):
      raise ValueError('original loaded msgq binding identity is required')
    self.python = args.python
    self.python_identity = self.identity(args.python.lstat())
    self.python_target = args.python.resolve(strict=True)
    identities = {name: self.pin(path) for name, path in (
      ('python_target', self.python_target), ('binary', args.binary), ('params_binding', args.binding),
      ('original_ipc_binding', Path(module.__file__).absolute()), ('artifact_manifest', args.artifact / 'manifest.json'),
    )}
    for kind in ('lateral', 'longitudinal'):
      identities[kind + '_source_solver'] = self.pin(args.source_native / kind / 'c_generated_code' / 'acados_ocp_solver_pyx.so')
    proof = job / 'proof' / 'planner-native-controller-523'
    sdk_proof = {name: self.pin(proof / name) for name in (
      'controller-source.txt', 'controller-profile.txt', 'controller.sha256', 'rustc.txt', 'sdk.before.sha256',
    )}
    if os.environ.get('OPENPILOT_CI_PREPARED_RUST_MANIFEST'):
      sdk_proof['prepared_manifest'] = self.pin(Path(os.environ['OPENPILOT_CI_PREPARED_RUST_MANIFEST']))
    environment = {key: os.environ[key] for key in (
      'GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB', 'GITHUB_REPOSITORY', 'GITHUB_SHA',
      'RUNNER_OS', 'RUNNER_ARCH', 'RUNNER_ENVIRONMENT', 'NATIVE_TOOLS_HOST', 'RUSTUP_HOME', 'RUSTUP_AUTO_INSTALL',
      'OPENPILOT_CI_PREPARED_RUST_MODE', 'OPENPILOT_CI_PREPARED_RUST_ROOT', 'OPENPILOT_CI_PREPARED_RUST_MANIFEST',
    ) if key in os.environ}
    self.provenance = {'source_commit': commit, 'source_sha256': source, 'owner': owner, 'environment': environment,
                       'identities': identities, 'sdk_before_proof': sdk_proof, 'source_native': str(args.source_native),
                       'python_argv': str(args.python), 'python_executable': sys.executable, 'python_version': sys.version,
                       'admission': {'free_bytes': free, 'existing_bytes': 128 * 1024**2, 'capture_bytes': self.LIMIT}}
    self.files, self.directories, self.runs = {}, {}, []
    self.logical, self.allocated, self.events = 0, 0, 0
    self.error = None
    self.first_case_failure = None
    self.mkdir(self.directory)

  def pin(self, path):
    if path.parent.resolve(strict=True) != path.parent:
      raise ValueError('capture identity parent must be canonical')
    record, identity = self.hashed(path)
    self.pins[path] = record, identity
    return record

  def checked(self, operation, *arguments):
    if self.error is None:
      try:
        return operation(*arguments)
      except Exception as error:
        self.error = f'lifecycle capture failed: {error!r}'
    context = f'first lifecycle failure: {self.first_case_failure}; ' if self.first_case_failure else ''
    raise RuntimeError(context + self.error)

  def mkdir(self, path):
    self.owned_directory(path.parent)
    path.mkdir(mode=0o700, exist_ok=False)
    self.directories[path] = self.owned_directory(path)

  def usage(self):
    directories = [path.lstat() for path in self.directories]
    apparent = self.logical + sum(value.st_size for value in directories)
    allocated = self.allocated + sum(value.st_blocks * 512 for value in directories)
    if apparent > self.LIMIT or allocated > self.LIMIT:
      raise ValueError('lifecycle capture logical/du usage exceeds 64MiB')
    return {'payload_bytes': self.logical, 'apparent_bytes': apparent, 'du_bytes': allocated}

  def write(self, path, data, limit):
    if (path.parent not in self.directories or len(self.files) >= self.MEMBER_LIMIT
        or len(data) > limit or self.logical + len(data) > self.LIMIT):
      raise ValueError('capture file exceeds its admitted count/size/path')
    self.owned_directory(path.parent)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    created = os.fstat(descriptor)
    try:
      with os.fdopen(descriptor, 'wb', buffering=0) as stream:
        remaining = memoryview(data)
        while remaining:
          count = stream.write(remaining)
          if not count:
            raise OSError('capture write made no progress')
          remaining = remaining[count:]
        value = os.fstat(stream.fileno())
        if (not stat.S_ISREG(value.st_mode) or stat.S_IMODE(value.st_mode) != 0o600 or value.st_uid != os.getuid()
            or value.st_nlink != 1 or value.st_size != len(data) or self.identity(value) != self.identity(path.lstat())):
          raise ValueError('capture output must be exclusive owned regular mode-600 bytes')
      record = {'path': str(path.relative_to(self.directory)), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
      self.files[path] = record, self.identity(value)
      self.logical += len(data)
      self.allocated += value.st_blocks * 512
      self.usage()
      return record
    except BaseException:
      # 실패한 자신의 파일만 지운다. 특히 초과한 COMPLETE marker를 남기지 않는다.
      try:
        value = path.lstat()
        if (value.st_dev, value.st_ino) == (created.st_dev, created.st_ino):
          path.unlink()
      except OSError:
        pass
      raise

  def snapshot(self, row, params, label):
    before = self.owned_directory(params)
    destination = self.directory / row['path'] / ('params-' + label)
    self.mkdir(destination)
    names = set(row.get('param_names', ())) | {'CarParams'}
    for path in params.iterdir():
      names.add(path.name)
      if len(names) > 2048:
        raise ValueError('too many Params entries')
    if len(names) > 2048 or label in row['params']:
      raise ValueError('unexpected Params snapshot count')
    state, total = {}, 0
    for name in sorted(names):
      if not name.isascii() or not name.replace('_', '').isalnum():
        raise ValueError('unexpected Params key')
      path = params / name
      try:
        value = path.lstat()
      except FileNotFoundError:
        state[name] = {'kind': 'absent'}
        continue
      if stat.S_ISDIR(value.st_mode):
        if any(path.iterdir()) or path.resolve(strict=True) != path:
          raise ValueError('Params fault directory must be empty and canonical')
        state[name] = {'kind': 'dir'}
      elif stat.S_ISREG(value.st_mode):
        data = self.read(path, self.PACKET_LIMIT if name == 'CarParams' else self.PARAM_LIMIT)
        total += len(data)
        if total > self.PARAMS_LIMIT:
          raise ValueError('Params snapshot exceeds 4MiB')
        state[name] = {'kind': 'file', 'raw': self.write(destination / name, data, self.PACKET_LIMIT)}
      else:
        raise ValueError('Params snapshot rejects symlink/special entry')
    row['param_names'] = sorted(names)
    if self.owned_directory(params) != before:
      raise RuntimeError('Params directory identity changed during snapshot')
    row['params'][label] = {'monotonic': time.monotonic(), 'entries': state}

  def begin(self, case, mode, prefix, params, command, environment):
    return self.checked(self._begin, case, mode, prefix, params, command, environment)

  def _begin(self, case, mode, prefix, params, command, environment):
    index = len(self.runs)
    if index >= 26 or (case.name, mode) != self.expected[index]:
      raise ValueError('lifecycle capture run order differs')
    row = {'case': case.name, 'mode': mode, 'epoch': 0, 'run_index': index, 'phase': case.phase,
           'signal': int(case.signal), 'car_params': case.car_params, 'directories': list(case.directories),
           'path': f'{index:02}-{case.name}-{mode}', 'prefix': prefix, 'argv': command,
           'environment': {key: environment[key] for key in ('PARAMS_ROOT', 'PWD', 'LOGPRINT', 'SIMULATION',
                           'PYTHONDONTWRITEBYTECODE', 'OPENBLAS_NUM_THREADS', 'PYTHONUNBUFFERED')},
           'frames': 3 if case.phase in ('ready', 'recover') else 1 if case.phase == 'float' else 0,
           'params': {}, 'inputs': []}
    directory = self.directory / row['path']
    self.mkdir(directory)
    self.mkdir(directory / 'inputs')
    self.snapshot(row, params, 'prelaunch')
    self.runs.append(row)
    return row

  def recovery(self, row, params, label):
    self.checked(self.snapshot, row, params, label)

  def send(self, row, publisher, service, packet, frame):
    self.checked(self._send, row, publisher, service, packet, frame)

  def _send(self, row, publisher, service, packet, frame):
    index = len(row['inputs'])
    if (self.events >= 234 or frame >= row['frames'] or frame != index // 9
        or service != self.SEND_ORDER[index % 9]):
      raise ValueError('lifecycle input count/order differs')
    data = packet if isinstance(packet, bytes) else packet.to_bytes()
    raw = self.write(self.directory / row['path'] / 'inputs' / f'{index:02}-{service}.bin', data, self.PACKET_LIMIT)
    publisher.send(service, data)
    row['inputs'].append({'frame': frame, 'service': service, 'raw': raw})
    self.events += 1

  def end(self, row, result, receipt):
    if not result['ok'] and self.first_case_failure is None:
      self.first_case_failure = repr(result)
    try:
      return self.checked(self._end, row, result, receipt)
    except RuntimeError as error:
      if result['ok']:
        raise
      print(str(error), file=sys.stderr, flush=True)

  def _end(self, row, result, receipt):
    row['result'] = result
    row['result_receipt_sha256'] = hashlib.sha256(self.read(receipt, self.PACKET_LIMIT)).hexdigest()
    payload = (json.dumps(row, indent=2, sort_keys=True, allow_nan=False) + '\n').encode()
    row['manifest'] = self.write(self.directory / row['path'] / 'manifest.json', payload, self.PACKET_LIMIT)

  def finish(self, results, receipt):
    return self.checked(self._finish, results, receipt)

  def verify(self):
    for path, expected in self.parents.items():
      if self.owned_directory(path) != expected:
        raise RuntimeError('capture owner directory identity changed')
    for path, (record, identity) in self.pins.items():
      if self.hashed(path) != (record, identity):
        raise RuntimeError('capture source/SDK/binding identity changed')
    if self.identity(self.python.lstat()) != self.python_identity or self.python.resolve(strict=True) != self.python_target:
      raise RuntimeError('capture Python executable alias changed')
    paths = set(self.directories) | set(self.files)
    actual = {self.directory}
    for directory, dirs, files in os.walk(self.directory, followlinks=False):
      actual.update(Path(directory) / name for name in dirs + files)
      if len(actual) > self.MEMBER_LIMIT:
        raise ValueError('lifecycle capture member count exceeds bound')
    if actual != paths:
      raise RuntimeError('unexpected lifecycle capture member/reappearance')
    for path, expected in self.directories.items():
      if self.owned_directory(path) != expected:
        raise RuntimeError('capture directory identity changed')
    allocated = 0
    for path, (record, identity) in self.files.items():
      value = path.lstat()
      if (self.identity(value) != identity or hashlib.sha256(self.read(path, self.PACKET_LIMIT)).hexdigest()
          != record['sha256']):
        raise RuntimeError('capture member bytes/identity changed')
      allocated += value.st_blocks * 512
    self.allocated = allocated
    return self.usage()

  def _finish(self, results, receipt):
    if (len(self.runs) != 26 or self.events != 234 or len(results) != 26
        or sum(len(row['params']) for row in self.runs) != 30
        or any(not result['ok'] for result in results)):
      raise ValueError('incomplete original lifecycle capture')
    for row, result in zip(self.runs, results):
      labels = {'prelaunch', 'recovery-before', 'recovery-after'} if row['phase'] == 'recover' else {'prelaunch'}
      if (row.get('result') != result or 'manifest' not in row or set(row['params']) != labels
          or len(row['inputs']) != row['frames'] * 9):
        raise ValueError('incomplete lifecycle run manifest')
    usage = self.verify()
    index = {'schema_version': 1, 'kind': 'independent-original-planner-lifecycle-inputs', 'status': 'COMPLETE',
             'provenance': self.provenance, 'runs': [{'case': row['case'], 'mode': row['mode'], 'epoch': row['epoch'],
               'run_index': row['run_index'], 'manifest': row['manifest']} for row in self.runs],
             'inputs': self.events, 'params_snapshots': sum(len(row['params']) for row in self.runs),
             'receipt_sha256': hashlib.sha256((json.dumps(receipt, indent=2) + '\n').encode()).hexdigest(),
             'usage_before_index': usage, 'capture_limit_bytes': self.LIMIT}
    marker = self.directory / 'index.json'
    self.write(marker, (json.dumps(index, indent=2, sort_keys=True) + '\n').encode(), self.PACKET_LIMIT)
    try:
      self.verify()
    except BaseException:
      try:
        if self.identity(marker.lstat()) == self.files[marker][1]:
          marker.unlink()
      except OSError:
        pass
      raise


def no_core() -> None:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def waiting(child: subprocess.Popen[bytes], paths: list[Path]) -> None:
  deadline = time.monotonic() + 15
  while time.monotonic() < deadline:
    assert child.poll() is None, ("exited before Params wait", child.returncode)
    if any("plannerd is waiting for CarParams" in path.read_text() for path in paths):
      time.sleep(.12)
      assert child.poll() is None, ("exited while waiting", child.returncode)
      return
    time.sleep(.01)
  raise TimeoutError("Params wait was not observed")


def frames(publisher, subscribers, child, output: Path, count: int, capture=None, captured=None) -> list[str]:
  seen = []
  for index in range(count):
    packets = messages(messaging.new_message, 45 + index, time.monotonic())
    packets["selfdriveState"].selfdriveState.experimentalMode = True
    for name, packet in packets.items():
      if name != "modelV2":
        if capture is None:
          publisher.send(name, packet)
        else:
          capture.send(captured, publisher, name, packet, index)
    if capture is None:
      publisher.send("modelV2", packets["modelV2"])
    else:
      capture.send(captured, publisher, "modelV2", packets["modelV2"], index)
    if not subscribers:
      return seen
    deadline = time.monotonic() + 3
    current = set()
    while time.monotonic() < deadline and set(OUTPUTS) != current:
      assert child.poll() is None, ("exited during publication", child.returncode)
      for name, stream in subscribers.items():
        while (raw := stream.receive(non_blocking=True)) is not None:
          path = output / f"publication-{index}-{len(seen)}-{name}.bin"
          path.write_bytes(raw)
          service, _valid, _value = policy(raw)
          assert service == name
          current.add(name)
          seen.append(name)
      time.sleep(.001)
    assert set(OUTPUTS) == current, ("missing publications", current)
    time.sleep(.05)
  return seen


def run(args, case: Case, mode: str, capture=None):
  output = args.output / case.name / mode
  output.mkdir(parents=True)
  result = {"case": case.name, "mode": mode, "ok": False}
  with tempfile.TemporaryDirectory(prefix="msgq_planner_fault_", dir="/dev/shm") as shared:
    prefix = Path(shared).name.removeprefix("msgq_")
    os.environ["OPENPILOT_PREFIX"] = prefix
    publisher = messaging.PubMaster(SERVICES)
    subscribers = {}
    params = output / "params" / prefix
    params.mkdir(parents=True)
    for key, value in (parameters() | {"EnableRadarTracks": "1", "MyDrivingModeAuto": "0"}).items():
      (params / key).write_text(value)
    for key in case.directories:
      (params / key).unlink(missing_ok=True)
      (params / key).mkdir()
    if case.phase == "integer":
      (params / "EnableRadarTracks").write_text("invalid")
    if case.phase == "float":
      (params / "LatMpcPathCost").write_text("invalid")
    cp = params / "CarParams"
    if case.car_params == "directory":
      cp.mkdir()
    elif case.car_params == "empty":
      cp.write_bytes(b"")
    if case.phase not in ["wait", "recover"]:
      cp.write_bytes(b"x" if case.phase == "malformed" else car_params().to_bytes())
    environment = dict(os.environ, PARAMS_ROOT=str(params.parent), PWD=str(ROOT), LOGPRINT="info",
                       PYTHONDONTWRITEBYTECODE="1", OPENBLAS_NUM_THREADS="1", PYTHONUNBUFFERED="1", SIMULATION="0")
    command = ([str(args.python), "-u", "-P", str(ROOT / "rust/tools/plannerd_daemon_source.py"),
                "--binding", str(args.binding), "--source-native", str(args.source_native), "--log-root", str(output / "logs")]
               if mode == "source" else [str(args.binary), "--solver", str(args.artifact)])
    (output / "invocation.json").write_text(json.dumps({"argv": command, "prefix": prefix}, indent=2) + "\n")
    paths = [output / "stdout.log", output / "stderr.log"]
    captured = capture.begin(case, mode, prefix, params, command, environment) if capture is not None else None
    with paths[0].open("wb") as stdout, paths[1].open("wb") as stderr:
      child = subprocess.Popen(command, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr, preexec_fn=no_core)
      try:
        if case.phase in ["wait", "recover"]:
          waiting(child, paths)
        if case.phase == "recover":
          if capture is not None:
            capture.recovery(captured, params, 'recovery-before')
          cp.rmdir()
          cp.write_bytes(car_params().to_bytes())
          if capture is not None:
            capture.recovery(captured, params, 'recovery-after')
        if case.phase in ["ready", "recover", "float"]:
          peers_ready(publisher, child)
          if case.phase != "float":
            subscribers = {name: messaging.sub_sock(name, conflate=False, timeout=0) for name in OUTPUTS}
          if capture is None:
            result["publications"] = frames(publisher, subscribers, child, output, 3 if subscribers else 1)
          else:
            result["publications"] = frames(publisher, subscribers, child, output, 3 if subscribers else 1, capture, captured)
        if case.phase in ["integer", "float", "malformed"]:
          expected = (1 if mode == "rust" else -signal.SIGABRT) if case.phase != "malformed" else 1
          result["returncode"] = child.wait(timeout=15)
        else:
          child.send_signal(case.signal)
          result["returncode"] = child.wait(timeout=5)
          expected = (-signal.SIGINT if mode == "source" else 0) if case.phase == "wait" or case.signal == signal.SIGINT else -signal.SIGTERM
        result["expected_returncode"] = expected
        assert result["returncode"] == expected, result
        assert not Path(f"/proc/{child.pid}").exists()
        result["ok"] = True
      except (AssertionError, RuntimeError, TimeoutError, subprocess.TimeoutExpired) as error:
        result["error"] = repr(error)
        result["returncode"] = child.poll()
      finally:
        if child.poll() is None:
          if capture is None:
            child.kill()
            child.wait(timeout=5)
          else:
            active_error = sys.exc_info()[1]
            try:
              child.kill()
              child.wait(timeout=5)
            except Exception as error:
              if active_error is None and 'error' not in result:
                raise
              print(f'lifecycle cleanup also failed: {error!r}', file=sys.stderr, flush=True)
    subscribers.clear()
    del publisher
    gc.collect()
  (output / "receipt.json").write_text(json.dumps(result, indent=2) + "\n")
  if capture is not None:
    capture.end(captured, result, output / 'receipt.json')
  return result
