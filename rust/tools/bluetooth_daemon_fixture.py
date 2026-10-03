from dataclasses import dataclass, replace
import json
import os
from pathlib import Path
import signal
import struct
import subprocess
import sys
import threading
import time
import uuid
from collections.abc import Callable

from pytest import MonkeyPatch
from openpilot.cereal import messaging

MAC = 'AA:BB:CC:DD:EE:FF'
WIRE = struct.Struct('@llHHi')


def wait_for(predicate: Callable[[], bool], timeout: float = 5) -> None:
  deadline = time.monotonic() + timeout
  while not predicate():
    if time.monotonic() >= deadline:
      raise TimeoutError('owned Bluetooth fixture readiness')
    time.sleep(0.005)


@dataclass(frozen=True)
class Program:
  binary: Path | None
  shim: Path
  permission_wait: bool = False


@dataclass(frozen=True)
class State:
  started: bool = False
  car_valid: bool = True
  can_valid: bool = True
  enabled: bool = False
  brake: bool = False
  gas: bool = False
  gear: str = 'drive'
  buttons: bool = False
  car_publish: bool = True


class Fixture:
  def __init__(self, directory: Path, program: Program):
    self.root = directory.resolve()
    self.root.mkdir(parents=True)
    self.runtime, self.config, self.sysfs = self.root / 'runtime', self.root / 'config.json', self.root / 'sysfs'
    self.runtime.mkdir()
    self.node = self.sysfs / 'event99999/device'
    (self.node / 'id').mkdir(parents=True)
    (self.node / 'id/bustype').write_text('0005')
    (self.node / 'uniq').write_text(MAC)
    (self.node / 'name').write_text('owned host remote')
    self.settings = {'version': 1, 'devices': {MAC: {'profile': 'generic', 'enabled': True, 'mapping':
                     {'key:115': 'accelCruise', 'key:115@long': 'accelCruise', 'key:114': 'laneLeft'}}}}
    self.write(self.config, self.settings)
    fifo = self.root / 'input-fifo'
    os.mkfifo(fifo, 0o600)
    self.keeper = os.open(fifo, os.O_RDWR | os.O_NONBLOCK)
    prefix = 'bt155_' + uuid.uuid4().hex[:20]
    self.shm = Path('/dev/shm') / ('msgq_' + prefix)
    self.shm.mkdir()
    self.env = dict(os.environ, OPENPILOT_PREFIX=prefix, LD_PRELOAD=str(program.shim.resolve()),
                    INPUT_FIXTURE_PATH='/dev/input/event99999', INPUT_FIXTURE_FIFO=str(fifo),
                    INPUT_FIXTURE_LOG=str(self.root / 'calls.log'))
    if program.permission_wait:
      commands = self.root / 'commands'
      commands.mkdir()
      sudo = commands / 'sudo'
      sudo.write_bytes(Path(__file__).with_name('bluetooth_sudo_fixture.py').read_bytes())
      sudo.chmod(0o700)
      self.env.update(PATH=str(commands), INPUT_FIXTURE_DENIED='1', INPUT_FIXTURE_SUDO_MODE='timeout',
                      INPUT_FIXTURE_SUDO_LOG=str(self.root / 'sudo.jsonl'))
    self.command = ([sys.executable, str(Path(__file__).with_name('bluetooth_daemon_source.py')),
                     str(self.runtime), str(self.config), str(self.sysfs)] if program.binary is None else
                    [str(program.binary.resolve()), '--runtime-root', str(self.runtime), '--config', str(self.config), '--sysfs', str(self.sysfs)])
    self.state = State()
    self.stop = threading.Event()
    self.errors = []
    with MonkeyPatch.context() as patch:
      patch.setenv('OPENPILOT_PREFIX', prefix)
      self.publisher = messaging.PubMaster(['carState', 'deviceState', 'selfdriveState'])
    self.thread = threading.Thread(target=self.publish)
    self.thread.start()
    self.log = (self.root / 'daemon.log').open('w')
    self.child = subprocess.Popen(self.command, env=self.env, stdout=self.log, stderr=subprocess.STDOUT)

  def publish(self) -> None:
    try:
      while not self.stop.is_set():
        state = self.state
        device = messaging.new_message('deviceState')
        device.valid = True
        device.deviceState.started = state.started
        self.publisher.send('deviceState', device)
        if state.car_publish:
          car = messaging.new_message('carState')
          car.valid = state.car_valid
          car.carState.canValid = state.can_valid
          car.carState.vEgo = 0
          car.carState.brakePressed = state.brake
          car.carState.gasPressed = state.gas
          car.carState.gearShifter = state.gear
          car.carState.buttonEvents = [{'type': 'accelCruise', 'pressed': True}] if state.buttons else []
          self.publisher.send('carState', car)
        controls = messaging.new_message('selfdriveState')
        controls.valid = True
        controls.selfdriveState.enabled = state.enabled
        self.publisher.send('selfdriveState', controls)
        self.stop.wait(0.01)
    except (OSError, RuntimeError) as error:
      self.errors.append(str(error))

  @staticmethod
  def write(path: Path, value: dict) -> None:
    temporary = path.with_suffix('.fixture-tmp')
    temporary.write_text(json.dumps(value))
    temporary.replace(path)

  def status(self) -> dict:
    path = self.runtime / 'status.json'
    return json.loads(path.read_text()) if path.exists() else {}

  def events(self) -> list[dict]:
    return [item for channel in ('cruise', 'lane') for item in json.loads((self.runtime / f'{channel}.json').read_text())['events']]

  def update(self, **fields) -> None:
    self.state = replace(self.state, **fields)

  def input(self, key: int, value: int) -> None:
    now = time.monotonic_ns()
    seconds, nanos = divmod(now, 10**9)
    packet = WIRE.pack(seconds, nanos // 1000, 1, key, value) + WIRE.pack(seconds, nanos // 1000, 0, 0, 0)
    assert os.write(self.keeper, packet) == len(packet)

  def click(self, key: int = 115, blocked: bool = False) -> dict:
    before = self.status().get('last_event')
    started = time.monotonic()
    self.input(key, 1)
    self.input(key, 0)
    if blocked:
      wait_for(lambda: self.status().get('time', 0) > started + 0.25)
      assert self.status().get('last_event') == before and self.events() == []
      return {'blocked': True}
    wait_for(lambda: bool(self.status().get('last_event')) and self.status()['last_event'] != before)
    record = self.status()['last_event']
    return {field: record[field] for field in ['address', 'button', 'action', 'emitted', 'reason']}

  def close(self) -> None:
    if self.child.poll() is None:
      self.child.send_signal(signal.SIGINT)
      try:
        self.child.wait(timeout=3)
      except subprocess.TimeoutExpired:
        self.child.kill()
        self.child.wait(timeout=3)
        self.errors.append('native/source failed to stop within three seconds')
    self.stop.set()
    self.thread.join(timeout=3)
    self.log.close()
    os.close(self.keeper)
    for path in self.shm.iterdir():
      path.unlink()
    self.shm.rmdir()
    assert not self.errors, self.errors
