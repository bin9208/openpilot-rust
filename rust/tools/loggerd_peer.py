"""Native msgq drive and complete full-schema route capture."""
from __future__ import annotations

from dataclasses import dataclass
from hashlib import sha256
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time

import openpilot.cereal.messaging as messaging
from openpilot.cereal import log
import zstandard
import zmq


@dataclass(frozen=True, slots=True)
class PeerSettings:
  services: tuple[str, ...]
  audio: bool = False
  test: bool = True
  parameters: tuple[tuple[str, str], ...] = ()
  runner: tuple[str, ...] = ()


class Peer:
  def __init__(self, binary: Path, root: Path, settings: PeerSettings):
    self.root = root
    root.mkdir(parents=True)
    self.prefix = f'logger-qa-{os.getpid()}-{root.name}'
    self.shm = Path('/dev/shm') / ('msgq_' + self.prefix)
    self.shm.mkdir()
    self.params = root / 'params' / self.prefix
    self.params.mkdir(parents=True)
    values = {'RecordRoadCam': '2', 'RecordFront': '1', 'RecordAudio': str(int(settings.audio)),
                       'RouteCount': '42', 'DongleId': 'host-qa-fixture', 'AccessToken': 'redact-this-fixture',
                       'GitCommit': '1' * 40, 'GitCommitDate': 'source-fixture-date', 'GitBranch': 'host-qa',
                       'GitRemote': 'source-fixture-remote', 'AthenadRecentlyViewedRoutes': 'old-route'}
    values.update(settings.parameters)
    for key, value in values.items():
      (self.params / key).write_text(value)
    os.environ['OPENPILOT_PREFIX'] = self.prefix
    self.publisher = messaging.PubMaster(sorted(set(settings.services) | {'logMessage'}))
    self.diagnostic_context = zmq.Context()
    self.diagnostic_socket = self.diagnostic_context.socket(zmq.PULL)
    self.diagnostic_socket.setsockopt(zmq.RCVHWM, 10000)
    self.diagnostic_path = Path('/tmp/logmessage' + self.prefix)
    self.diagnostic_socket.bind('ipc://' + str(self.diagnostic_path))
    self.diagnostics: list[dict] = []
    environment = dict(os.environ, PARAMS_ROOT=str(root / 'params'), LOG_ROOT=str(root / 'logs'))
    if settings.test:
      environment.update(LOGGERD_TEST='1', LOGGERD_SEGMENT_LENGTH='60')
    else:
      environment.pop('LOGGERD_TEST', None)
      environment.pop('LOGGERD_SEGMENT_LENGTH', None)
    command = [*settings.runner, str(binary)]
    self.invocation = {'argv': command, 'binary_sha256': sha256(binary.read_bytes()).hexdigest(),
                       'environment': {key: environment[key] for key in ('OPENPILOT_PREFIX', 'PARAMS_ROOT', 'LOG_ROOT')},
                       'test_mode': settings.test, 'record_audio': settings.audio}
    (root / 'invocation.json').write_text(json.dumps(self.invocation, indent=2) + '\n')
    self.started = time.monotonic()
    with (root / 'stdout.log').open('wb') as stdout, (root / 'stderr.log').open('wb') as stderr:
      self.process = subprocess.Popen(command, env=environment, stdout=stdout, stderr=stderr)
    self.inputs: list[tuple[str, bytes]] = []
    try:
      self.await_file(self.params / 'CurrentRoute')
      self.route = (self.params / 'CurrentRoute').read_text()
      self.barrier()
    except BaseException:
      self.cleanup()
      raise

  def await_file(self, path: Path, timeout: float = 20) -> None:
    deadline = time.monotonic() + timeout
    while not path.exists():
      if self.process.poll() is not None:
        raise RuntimeError((self.invocation, self.process.returncode, (self.root / 'stderr.log').read_text()))
      if time.monotonic() >= deadline:
        raise TimeoutError(str(path))
      time.sleep(.005)

  def send(self, service: str, data: bytes) -> None:
    self.publisher.send(service, data)
    assert self.publisher.wait_for_readers_to_update(service, timeout=10, dt=.001), service
    self.inputs.append((service, data))
    self.await_idle()

  def await_idle(self, timeout: float = 10) -> None:
    # The msgq read pointer advances before processing. A subsequent SIGUSR2 can
    # interrupt that input's best-effort ZMQ diagnostic, so pace this exact oracle.
    deadline = time.monotonic() + timeout
    channel = Path(f'/proc/{self.process.pid}/wchan')
    ppoll_number = {'x86_64': '271', 'aarch64': '73'}.get(os.uname().machine)
    while True:
      if self.process.poll() is not None:
        raise RuntimeError((self.process.returncode, (self.root / 'stderr.log').read_text()))
      wait_channel = channel.read_text().strip()
      if wait_channel == 'hrtimer_nanosleep':
        return
      if wait_channel not in ('', '0'):
        syscall = channel.with_name('syscall').read_text().split()
        if len(syscall) >= 3 and syscall[0] == ppoll_number and syscall[2] == '0x0':
          return
      if time.monotonic() >= deadline:
        raise TimeoutError(f'logger did not finish processing before its next input: {self.root}')
      time.sleep(.0001)

  def barrier(self) -> None:
    message = messaging.new_message(None)
    message.logMessage = 'logger-qa-barrier'
    self.send('logMessage', message.to_bytes())

  def segment(self, part: int) -> Path:
    return self.root / 'logs' / f'{self.route}--{part}'

  def stop(self, received_signal: int = signal.SIGTERM) -> list[dict]:
    self.barrier()
    self.process.send_signal(received_signal)
    assert self.process.wait(timeout=20) == 0, (self.root / 'stderr.log').read_text()
    assert not list((self.root / 'logs').glob('*/*.lock'))
    result = self.logs()
    self.cleanup()
    return result

  def logs(self) -> list[dict]:
    inputs = self.root / 'inputs'
    inputs.mkdir(exist_ok=True)
    index = []
    for number, (service, data) in enumerate(self.inputs):
      path = inputs / f'{number:04d}-{service}.capnp'
      path.write_bytes(data)
      index.append({'service': service, 'path': path.name, 'sha256': sha256(data).hexdigest()})
    (inputs / 'index.json').write_text(json.dumps(index, indent=2) + '\n')
    result = []
    for directory in sorted((self.root / 'logs').iterdir(), key=lambda directory: int(directory.name.rsplit('--', 1)[1])):
      segment = {}
      for name in ('rlog', 'qlog'):
        with (directory / (name + '.zst')).open('rb') as compressed:
          data = zstandard.ZstdDecompressor().stream_reader(compressed).read()
        (directory / (name + '.capnp')).write_bytes(data)
        segment[name] = [message.to_dict() for message in log.Event.read_multiple_bytes(data)]
      segment['files'] = sorted(path.name for path in directory.iterdir() if path.suffix not in {'.capnp'})
      result.append(segment)
    (self.root / 'decoded.json').write_text(json.dumps(result, indent=2, default=lambda value: {'hex': value.hex()}) + '\n')
    return result

  def cleanup(self) -> None:
    if self.process.poll() is None:
      self.process.kill()
      self.process.wait()
    if not self.diagnostic_socket.closed:
      packets = []
      while self.diagnostic_socket.poll(20):
        packet = self.diagnostic_socket.recv()
        record = json.loads(packet[1:])
        assert packet[0] == record['levelnum']
        self.diagnostics.append(record)
        packets.append(packet.hex())
      (self.root / 'diagnostics.json').write_text(json.dumps(self.diagnostics, indent=2) + '\n')
      (self.root / 'diagnostic-packets.json').write_text(json.dumps(packets, indent=2) + '\n')
      self.diagnostic_socket.close(linger=0)
      self.diagnostic_context.term()
      self.diagnostic_path.unlink(missing_ok=True)
    self.publisher = None
    try:
      shutil.rmtree(self.shm)
    except FileNotFoundError:
      pass
