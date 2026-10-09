from __future__ import annotations

import json
import os
from pathlib import Path
import sys
import time
import types

from openpilot.cereal import messaging
from paramsd_source import load


class Finished(Exception):
  pass


class Store:
  def __init__(self, directory: Path) -> None:
    self.directory = directory

  def get(self, key: str, block: bool = False):
    while True:
      try:
        data = (self.directory / key).read_bytes()
      except FileNotFoundError:
        if not block:
          return None
        time.sleep(.01)
        continue
      if key == 'LiveParameters':
        try:
          return json.loads(data)
        except (ValueError, UnicodeError):
          return None
      return data

  def put(self, key: str, data: bytes) -> None:
    (self.directory / key).write_bytes(data)

  def put_nonblocking(self, key: str, data: bytes | str) -> None:
    self.put(key, data.encode() if isinstance(data, str) else data)

  def remove(self, key: str) -> None:
    (self.directory / key).unlink(missing_ok=True)


def main() -> None:
  root, oracle, output = (Path(value).resolve() for value in sys.argv[1:4])
  prefix = os.environ['OPENPILOT_PREFIX']
  assert prefix.startswith('rust-probe-params-')
  params = Store(root / 'params' / prefix)
  memory = Store(root / 'memory' / prefix)
  source, logs = load(oracle)

  class SubMaster(messaging.SubMaster):
    def update(self) -> None:
      super().update()
      with (output / 'source-updates.jsonl').open('a') as stream:
        stream.write(json.dumps({'frame': self.frame, 'updated': self.updated}) + '\n')

  class PubMaster(messaging.PubMaster):
    def send(self, service: str, packet: bytes) -> None:
      super().send(service, packet)
      (output / 'source-publication.bin').write_bytes(packet)
      raise Finished()

  source.update(
    config_realtime_process=lambda _cores, _priority: None,
    os=os,
    Params=lambda directory=None: memory if directory == '/dev/shm/params' else params,
    get_gps_location_service=lambda _params: 'gpsLocationExternal',
    messaging=types.SimpleNamespace(SubMaster=SubMaster, PubMaster=PubMaster,
      new_message=messaging.new_message, log_from_bytes=messaging.log_from_bytes),
  )
  try:
    source['main']()
  except Finished:
    (output / 'source-logs.json').write_text(json.dumps(logs))


if __name__ == '__main__':
  main()
