"""Read a tinygrad artifact as inert records, without importing or executing its globals."""

from __future__ import annotations

import hashlib
import io
import mmap
import pickle
import pickletools
import re
import struct
from pathlib import Path
from typing import ClassVar, TypeAlias

Value: TypeAlias = "None | bool | int | float | str | bytes | memoryview | tuple[Value, ...] | list[Value] | dict[Value, Value] | Record"
Json: TypeAlias = "None | bool | int | float | str | list[Json] | dict[str, Json]"

ALLOWED = frozenset(
  {
    'tinygrad.engine.jit._TinyJit',
    'tinygrad.engine.jit.CapturedJit',
    'tinygrad.uop.ops.UOp',
    'tinygrad.uop.Ops',
    'tinygrad.uop.ops.ParamArg',
    'tinygrad.dtype.DType',
    'tinygrad.dtype.AddrSpace',
    'tinygrad.uop.ops.CallInfo',
    'tinygrad.uop.ops.AxisType',
    'tinygrad.uop.ops.KernelInfo',
    'tinygrad.renderer.Estimates',
    'tinygrad.uop.ops.ProgramInfo',
    'tinygrad.helpers.Target',
    'tinygrad.runtime.support.hcq2.HCQInfo',
    'tinygrad.device.Buffer',
    'tinygrad.device.BufferSpec',
  }
)


class ArtifactError(ValueError):
  pass


class Record:
  global_name: ClassVar[str]
  instances: ClassVar[list[Record]]
  arguments: tuple[Value, ...]
  state: Value

  def __new__(cls, *arguments: Value) -> Record:
    result = super().__new__(cls)
    result.arguments, result.state = arguments, None
    cls.instances.append(result)
    return result

  def __setstate__(self, state: Value) -> None:
    self.state = state


class DataUnpickler(pickle.Unpickler):
  def __init__(self, stream: io.BytesIO, buffers: list[memoryview]):
    super().__init__(stream, buffers=buffers)
    self.records: list[Record] = []

  def find_class(self, module: str, name: str) -> type[Record]:
    qualified = f'{module}.{name}'
    if qualified not in ALLOWED:
      raise ArtifactError(f'unsupported artifact global: {qualified}')
    return type(name, (Record,), {'global_name': qualified, 'instances': self.records})


def walk(root: Value) -> list[Value]:
  pending, seen, result = [root], set(), []
  while pending:
    value = pending.pop()
    if id(value) in seen:
      continue
    seen.add(id(value))
    result.append(value)
    match value:
      case Record():
        pending.extend((value.arguments, value.state))
      case dict():
        pending.extend(value.keys())
        pending.extend(value.values())
      case list() | tuple():
        pending.extend(value)
  return result


def record(value: Value, suffix: str) -> Record:
  if not isinstance(value, Record) or not value.global_name.endswith(suffix):
    raise ArtifactError(f'expected {suffix} record')
  return value


def state(value: Value) -> dict[Value, Value]:
  if not isinstance(value, Record) or not isinstance(value.state, dict):
    raise ArtifactError('expected record state dictionary')
  return value.state


class Artifact:
  def __init__(self, path: Path, runtime: Path):
    self.path, self.runtime = path, runtime
    source = (runtime / 'tinygrad/uop/__init__.py').read_text()
    names = re.findall(r'\b(\w+)\s*=\s*auto\(\)', source.split('class Ops')[1].split('class ')[0])
    self.ops = dict(enumerate(names, 1))
    self._file = path.open('rb')
    self._mapped = mmap.mmap(self._file.fileno(), 0, access=mmap.ACCESS_READ)
    self._views: list[memoryview] = []
    try:
      self._load()
    except (ArtifactError, ValueError, struct.error, pickle.UnpicklingError, EOFError):
      self.close()
      raise

  def _load(self) -> None:
    length = struct.unpack_from('<q', self._mapped)[0]
    if not 0 < length <= min(len(self._mapped) - 8, 64 << 20):
      raise ArtifactError('invalid artifact pickle length')
    self.opcodes = self._mapped[8 : 8 + length]
    if any(op.name in {'EXT1', 'EXT2', 'EXT4', 'PERSID', 'BINPERSID'} for op, _, _ in pickletools.genops(self.opcodes)):
      raise ArtifactError('external pickle references are unsupported')
    self.blobs: dict[tuple[int, str], int] = {}
    offset = 8 + length
    while offset < len(self._mapped):
      count = struct.unpack_from('<q', self._mapped, offset)[0]
      offset += 8
      if not 0 <= count <= len(self._mapped) - offset:
        raise ArtifactError('invalid out-of-band artifact range')
      view = memoryview(self._mapped)[offset : offset + count]
      self._views.append(view)
      self.blobs[(count, hashlib.sha256(view).hexdigest())] = offset
      offset += count
    unpickler = DataUnpickler(io.BytesIO(self.opcodes), buffers=self._views)
    self.value: Value = unpickler.load()
    self.records = unpickler.records
    self.values = walk(self.value)
    self._views.extend(v for v in self.values if isinstance(v, memoryview) and all(v is not old for old in self._views))
    self.sha256 = hashlib.sha256(self._mapped).hexdigest()

  def close(self) -> None:
    for view in self._views:
      view.release()
    self._mapped.close()
    self._file.close()

  def __enter__(self) -> Artifact:
    return self

  def __exit__(self, *_exc: Value) -> None:
    self.close()

  def op(self, value: Value) -> str:
    number = record(record(value, '.UOp').arguments[0], '.Ops').arguments[0]
    if not isinstance(number, int) or number not in self.ops:
      raise ArtifactError('unsupported UOp number')
    return self.ops[number]

  def blob(self, value: Value) -> dict[str, Json]:
    if not isinstance(value, (bytes, memoryview)):
      raise ArtifactError('expected artifact byte data')
    digest = hashlib.sha256(value).hexdigest()
    offset = self.blobs.get((len(value), digest))
    if offset is None:
      if not isinstance(value, bytes):
        raise ArtifactError('unknown out-of-band data')
      found = self.opcodes.find(value)
      if found < 0:
        raise ArtifactError('inline bytes are not contiguous in artifact')
      offset = found + 8
    return {'offset': offset, 'bytes': len(value), 'sha256': digest}
