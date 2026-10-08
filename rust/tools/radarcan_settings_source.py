from __future__ import annotations

from contextlib import contextmanager
from collections.abc import Iterator
from pathlib import Path
import tempfile
from typing import Protocol, TypedDict

from pytest import MonkeyPatch


class ParamsLike(Protocol):
  def get_int(self, key: str) -> int: ...


class Read(TypedDict):
  key: str
  value: int


class ObservedParams:
  def __init__(self, original: ParamsLike, reads: list[Read]) -> None:
    self.original = original
    self.reads = reads

  def get_int(self, key: str) -> int:
    value = self.original.get_int(key)
    self.reads.append({'key': key, 'value': value})
    return value


@contextmanager
def settings(values: dict[str, str] | None) -> Iterator[list[Read]]:
  reads: list[Read] = []
  if values is None:
    yield reads
    return
  from openpilot.common.params import Params
  with tempfile.TemporaryDirectory(prefix='radarcan-params-') as directory, MonkeyPatch.context() as fixture:
    fixture.setenv('OPENPILOT_PREFIX', 'radarcan-qa')
    initial = Params(directory)
    for key, value in values.items():
      Path(initial.get_param_path(key)).write_bytes(value.encode())

    def instance() -> ObservedParams:
      return ObservedParams(Params(directory), reads)

    fixture.setattr('opendbc.car.hyundai.radar_interface.Params', instance)
    fixture.setattr('opendbc.car.hyundai.hyundaicanfd.Params', instance)
    yield reads
