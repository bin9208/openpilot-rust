from dataclasses import asdict, dataclass
import json
from pathlib import Path
import threading
import time
from typing import Protocol

from openpilot.cereal import messaging


class VehicleInputs(Protocol):
  network: str
  can_error: bool


@dataclass(frozen=True, slots=True)
class Publication:
  started: float
  finished: float
  network: str
  can_error: bool


class InputPublisher:
  def __init__(self, publisher: messaging.PubMaster, state: VehicleInputs, root: Path) -> None:
    self.state = state
    self.root = root
    self.rows: list[Publication] = []
    self.stopped = threading.Event()
    self.failure: Exception | None = None
    self.thread = threading.Thread(target=self.run, args=(publisher,), name='carrot-owned-inputs', daemon=True)
    self.thread.start()

  def run(self, publisher: messaging.PubMaster) -> None:
    try:
      while not self.stopped.is_set():
        started = time.monotonic()
        network, can_error = self.state.network, self.state.can_error
        values = {
          'deviceState': {'networkType': network},
          'carState': {'vEgo': 0., 'vEgoCluster': 0., 'vCluRatio': 1., 'canValid': not can_error, 'canTimeout': can_error},
          'selfdriveState': {'active': False, 'distanceTraveled': 0.},
          'carControl': {},
          'gpsLocationExternal': {'hasFix': True, 'latitude': 37., 'longitude': 127., 'bearingDeg': 45.},
          'modelV2': {'position': {'x': [float(i) for i in range(33)], 'y': [0.] * 33, 'z': [0.] * 33},
            'velocity': {'x': [10.] * 33}, 'orientationRate': {'z': [0.] * 33}},
        }
        for name, value in values.items():
          message = messaging.new_message(name, valid=True)
          setattr(message, name, value)
          publisher.send(name, message)
        self.rows.append(Publication(started, time.monotonic(), network, can_error))
        self.stopped.wait(.01)
    except Exception as error:
      self.failure = error.with_traceback(None)
      self.stopped.set()

  def check(self) -> None:
    if self.failure is not None:
      raise RuntimeError('owned input publisher failed') from self.failure

  def close(self) -> None:
    self.stopped.set()
    self.thread.join(timeout=2)
    (self.root / 'input-publications.json').write_text(json.dumps({
      'publications': [asdict(row) for row in self.rows], 'joined': not self.thread.is_alive(),
      'failure': str(self.failure) if self.failure is not None else None}, indent=2)+'\n')
    if self.thread.is_alive():
      raise TimeoutError('owned input publisher did not stop')
    self.check()
