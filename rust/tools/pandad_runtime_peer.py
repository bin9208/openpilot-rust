from contextlib import ExitStack
from hashlib import sha256
import json
import os
from pathlib import Path
import signal
import select
import subprocess
import time

from openpilot.cereal import car, log, messaging
from logmessaged_native import Peer


def packet(address, source, data):
  assert len(data) <= 8
  result = bytes([(len(data) << 4) | (source << 1)]) + ((address << 3) | (4 if address >= 0x800 else 0)).to_bytes(4, 'little') + b'\0' + bytes(data)
  checksum = 0
  for byte in result:
    checksum ^= byte
  return result[:5] + bytes([checksum]) + result[6:]


class RuntimePeer:
  def __init__(self, binary: Path, collector: Path, fixture: Path, output: Path, *, onroad=False,
               count=1, flags=(), frames=None, skip_firmware=True, runner=()):
    self.output = output
    output.mkdir(parents=True)
    self.collector = Peer(collector, output / ('collector-' + output.name), False)
    self.process = None
    self.subscribers = {}
    self.publishers = {}
    self.messages = {name: [] for name in ('can', 'pandaStates', 'peripheralState', 'logMessage', 'errorLogMessage')}
    self.config = {'count': count, 'ignition': onroad, 'can_rx': list(packet(0x321, 0, [4, 5]))}
    self.config_path = output / 'usb-config.json'
    self.trace_path = output / 'usb-trace.jsonl'
    self.store_config()
    self.params = output / 'params' / self.collector.prefix
    self.params.mkdir(parents=True)
    for name, value in {'IsOnroad': onroad, 'IsDriverViewEnabled': True, 'FirmwareQueryDone': True, 'ControlsReady': True}.items():
      (self.params / name).write_bytes(b'1' if value else b'0')
    params = car.CarParams.new_message()
    params.alternativeExperience = 7
    params.init('safetyConfigs', count)
    for safety in params.safetyConfigs:
      safety.safetyModel = 'hyundai'
      safety.safetyParam = 42
    (self.params / 'CarParams').write_bytes(params.to_bytes())
    self.binary = binary
    self.fixture = fixture
    self.flags = flags
    self.count = count
    self.frames = frames
    self.skip_firmware = skip_firmware
    self.runner = runner

  def store_config(self):
    temporary = self.config_path.with_suffix('.new')
    temporary.write_text(json.dumps(self.config))
    temporary.replace(self.config_path)

  def start(self):
    self.collector.start()
    self.collector.synchronize()
    for name in ('selfdriveState', 'deviceState', 'driverCameraState', 'sendcan'):
      self.publishers[name] = messaging.pub_sock(name)
    for name in ('can', 'pandaStates', 'peripheralState'):
      self.subscribers[name] = messaging.sub_sock(name)
    environment = dict(os.environ, PARAMS_ROOT=str(self.params.parent), HOME=str(self.collector.output / 'home'),
                       OPENPILOT_PREFIX=self.collector.prefix, PANDA_FIXTURE_CONFIG=str(self.config_path),
                       PANDA_FIXTURE_TRACE=str(self.trace_path), LD_LIBRARY_PATH=str(self.fixture.parent), MANAGER_DAEMON='pandad')
    for flag in ('FAKESEND', 'STARTED', 'NO_FAN_CONTROL', 'PANDAD_MAXOUT', 'BOARDD_LOOPBACK', 'BOARDD_SKIP_FW_CHECK', 'LOG_TIMESTAMPS', 'SIMULATION'):
      environment.pop(flag, None)
    for flag in self.flags:
      environment[flag] = '1'
    if self.skip_firmware:
      environment['BOARDD_SKIP_FW_CHECK'] = '1'
    command = [*self.runner, str(self.binary), *(['--frames', str(self.frames)] if self.frames else []),
               *[f'PANDA-FIXTURE-{index}' for index in range(self.count)]]
    cwd = self.output / 'cwd'
    cwd.mkdir()
    with (self.output / 'stdout.log').open('wb') as stdout, (self.output / 'stderr.log').open('wb') as stderr:
      self.launch(command, environment, cwd, stdout, stderr)
    (self.output / 'invocation.json').write_text(json.dumps({'argv': command, 'cwd': str(cwd),
      'fixture': str(self.fixture), 'fixture_sha256': sha256(self.fixture.read_bytes()).hexdigest(),
      'binary_sha256': sha256(self.binary.read_bytes()).hexdigest(), 'prefix': self.collector.prefix}, indent=2))

  def launch(self, command, environment, cwd, stdout, stderr):
    if not self.skip_firmware:
      self.process = subprocess.Popen(command, env=environment, cwd=cwd, stdout=stdout, stderr=stderr)
      return
    with ExitStack() as stack:
      ready_read, ready_write = [stack.enter_context(os.fdopen(fd, mode, buffering=0))
                                 for fd, mode in zip(os.pipe(), ('rb', 'wb'), strict=True)]
      release_read, release_write = [stack.enter_context(os.fdopen(fd, mode, buffering=0))
                                     for fd, mode in zip(os.pipe(), ('rb', 'wb'), strict=True)]
      environment = dict(environment, PANDA_FIXTURE_READY_FD=str(ready_write.fileno()),
                         PANDA_FIXTURE_RELEASE_FD=str(release_read.fileno()))
      self.process = subprocess.Popen(command, env=environment, cwd=cwd, stdout=stdout, stderr=stderr,
                                      pass_fds=(ready_write.fileno(), release_read.fileno()))
      ready_write.close()
      release_read.close()
      assert select.select([ready_read], [], [], 5)[0], 'first health read did not reach the fixture gate'
      assert ready_read.read(1) == b'R'
      # Publisher initialization resets readers; synchronize before the first state publication.
      self.poll()
      assert not self.messages['pandaStates']
      release_write.write(b'G')

  def ready(self):
    for publisher in self.publishers.values():
      publisher.wait_for_readers(timeout=5)
    self.until(lambda: all(self.messages[name] for name in ('can', 'pandaStates', 'peripheralState')), 5)
    maps = Path(f'/proc/{self.process.pid}/maps').read_text()
    (self.output / 'maps.txt').write_text(maps)
    assert str(self.fixture) in maps and 'libpython' not in maps
    assert all(str(self.fixture) in line for line in maps.splitlines() if 'libusb-1.0.so' in line)

  def poll(self):
    for name, subscriber in {**self.subscribers, **self.collector.subscribers}.items():
      while (packet_bytes := subscriber.receive(non_blocking=True)) is not None:
        with (self.output / f'{name}.bin').open('ab') as stream:
          stream.write(packet_bytes)
        with log.Event.from_bytes(packet_bytes) as event:
          assert event.which() == name
          value = event.to_dict()
          if name in ('logMessage', 'errorLogMessage'):
            value[name] = json.loads(getattr(event, name))
          self.messages[name].append(value)

  def until(self, condition, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
      self.poll()
      if condition():
        return
      if self.process.poll() is not None:
        raise AssertionError(('Panda exited before condition', self.process.returncode, self.output))
      time.sleep(.002)
    raise TimeoutError(('Panda condition timed out', self.output))

  def inputs(self, frame, enabled=True, camera=True):
    state = messaging.new_message('selfdriveState', valid=True)
    state.selfdriveState.enabled = enabled
    self.publishers['selfdriveState'].send(state.to_bytes())
    device = messaging.new_message('deviceState', valid=True)
    device.deviceState.fanSpeedPercentDesired = 51
    self.publishers['deviceState'].send(device.to_bytes())
    if camera:
      image = messaging.new_message('driverCameraState', valid=True)
      image.driverCameraState.frameId = frame
      image.driverCameraState.integLines = 200000
      self.publishers['driverCameraState'].send(image.to_bytes())

  def send(self, address, timestamp=None):
    value = messaging.new_message('sendcan', 1, valid=False)
    if timestamp is not None:
      value.logMonoTime = timestamp
    value.sendcan[0].address = address
    value.sendcan[0].src = 0
    value.sendcan[0].dat = b'\x12\x34'
    self.publishers['sendcan'].send(value.to_bytes())

  def trace(self):
    return [json.loads(line) for line in self.trace_path.read_text().splitlines()]

  def finish(self, signal_number=signal.SIGINT):
    started = time.monotonic()
    if self.process.poll() is None and signal_number is not None:
      self.process.send_signal(signal_number)
    result = self.process.wait(timeout=5)
    self.poll()
    elapsed = time.monotonic() - started
    (self.output / 'exit.json').write_text(json.dumps({'code': result, 'elapsed': elapsed, 'signal': signal_number}))
    return result

  def close(self):
    if self.process is not None and self.process.poll() is None:
      self.process.kill()
      self.process.wait(timeout=5)
    self.subscribers.clear()
    self.publishers.clear()
    self.collector.stop()
    self.collector.close()
