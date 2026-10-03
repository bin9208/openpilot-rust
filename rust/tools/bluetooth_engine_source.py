import os
import importlib
from pathlib import Path
from types import SimpleNamespace
from pytest import MonkeyPatch

from bluetooth_engine_scenarios import Step
from openpilot.selfdrive.carrot.bluetooth import model
import openpilot.cereal


def unconfigured_state(_services: list[str]) -> None:
  raise RuntimeError('source IPC boundary must be configured before daemon.main')


with MonkeyPatch.context() as import_patch:
  import_patch.setattr(openpilot.cereal, 'messaging', SimpleNamespace(SubMaster=unconfigured_state), raising=False)
  daemon = importlib.import_module('openpilot.selfdrive.carrot.bluetooth.daemon')


class Done(Exception):
  pass


def capture(root: Path) -> dict[str, str | None]:
  return {name: (root / f'{name}.json').read_text() if (root / f'{name}.json').exists() else None for name in ('cruise', 'lane', 'status')}


class Source:
  def __init__(self, root: Path, steps: list[Step]):
    self.root = root
    self.steps = steps
    self.index = -1
    self.next_clock = True
    self.current = steps[0]
    self.pipes: dict[str, tuple[int, int | None]] = {}
    self.changes: list[str] = []
    self.results: list[dict[str, dict[str, str | None] | list[str]]] = []
    self.state = State(self)
    self.config_path = root / 'fixture-config.json'
    self.config_path.write_text(self.current['config'])

  def advance(self) -> None:
    if self.index >= 0:
      self.results.append({'files': capture(self.root), 'changes': self.changes})
      self.changes = []
    self.index += 1
    if self.index == len(self.steps):
      raise Done
    self.current = self.steps[self.index]
    self.config_path.write_text(self.current['config'])
    (self.root / 'learn.json').write_text(self.current['learning'])
    self.state.replace(self.current)

  def clock(self) -> float:
    if self.next_clock:
      self.next_clock = False
      return self.steps[min(self.index + 1, len(self.steps) - 1)]['now']
    return self.current['now']

  def open_input(self, path: str) -> int:
    self.changes.append(f'open:{path}')
    if path in self.current['errors']:
      raise OSError(self.current['errors'][path])
    reader, writer = os.pipe()
    self.pipes[path] = reader, writer
    return reader

  def close(self, fd: int) -> None:
    path = next(path for path, (reader, _) in self.pipes.items() if reader == fd)
    reader, writer = self.pipes.pop(path)
    os.close(reader)
    if writer is not None:
      os.close(writer)
    self.changes.append(f'close:{path}')

  def select(self, fds: list[int], _write: list[int], _error: list[int], _timeout: float) -> tuple[list[int], list[int], list[int]]:
    ready = []
    for path, events in self.current['reads'].items():
      if path not in self.pipes:
        continue
      reader, writer = self.pipes[path]
      assert reader in fds and writer is not None
      if events:
        payload = bytearray()
        for event in events:
          sec = int(event['at'])
          usec = round((event['at'] - sec) * 1e6)
          payload.extend(daemon.EVENT.pack(sec, usec, event['kind'], event['code'], event['value']))
        assert os.write(writer, payload) == len(payload)
      else:
        os.close(writer)
        self.pipes[path] = reader, None
      ready.append(reader)
    return ready, [], []

  def run(self) -> list[dict[str, dict[str, str | None] | list[str]]]:
    source = self

    class Writer(model.CommandWriter):
      def prune(self, addresses: set[str], now: float, active_holds: set[str] | None = None) -> None:
        super().prune(addresses, now, active_holds)
        source.next_clock = True

    with MonkeyPatch.context() as patch:
      patch.setattr(daemon, 'RUNTIME', self.root)
      patch.setattr(daemon, 'config', lambda: model.config(self.config_path))
      patch.setattr(daemon, 'CommandWriter', lambda: Writer(self.root))
      patch.setattr(daemon, 'devices', lambda: {path: (mac, 'fixture') for path, mac in self.current['available'].items()})
      patch.setattr(daemon, 'open_input', self.open_input)
      patch.setattr(daemon, 'os', SimpleNamespace(read=os.read, close=self.close))
      patch.setattr(daemon.select, 'select', self.select)
      patch.setattr(daemon.time, 'monotonic', self.clock)
      patch.setattr(daemon.messaging, 'SubMaster', lambda _: self.state)
      try:
        daemon.main()
      except Done:
        assert self.index == len(self.steps)
    assert not self.pipes
    self.results.append({'stopped': capture(self.root)})
    return self.results


class State(dict[str, SimpleNamespace]):
  def __init__(self, source: Source):
    super().__init__()
    self.source = source
    self.alive: dict[str, bool] = {}
    self.valid: dict[str, bool] = {}

  def update(self, _timeout: int) -> None:
    self.source.advance()

  def replace(self, step: Step) -> None:
    state = step['snapshot']
    self.alive = {'carState': state['car_alive'], 'deviceState': state['device_alive'], 'selfdriveState': state['controls_alive']}
    self.valid = {'carState': state['car_valid']}
    self['deviceState'] = SimpleNamespace(started=state['started'])
    self['selfdriveState'] = SimpleNamespace(enabled=state['enabled'])
    self['carState'] = SimpleNamespace(
      canValid=state['can_valid'],
      brakePressed=state['brake'],
      gasPressed=state['gas'],
      gearShifter='drive' if state['gear_drive'] else 'park',
      vEgo=state['v_ego'],
      buttonEvents=[1] if state['physical_buttons'] else [],
    )
