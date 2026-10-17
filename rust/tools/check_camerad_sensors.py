"""Shared comparison helpers retained for the remaining camera comparators."""

import struct

def f32(value: float) -> bytes:
  return struct.pack("<f", value)


def equal(source: object, native: object, path: str) -> None:
  if isinstance(source, dict):
    assert isinstance(native, dict), (path, source, native)
    assert source.keys() == native.keys(), (path, source.keys(), native.keys())
    for key, value in source.items():
      equal(value, native[key], f"{path}.{key}")
  elif isinstance(source, list):
    assert isinstance(native, list) and len(source) == len(native), (path, source, native)
    for index, (left, right) in enumerate(zip(source, native, strict=True)):
      equal(left, right, f"{path}[{index}]")
  elif isinstance(source, float):
    assert isinstance(native, (float, int)), (path, source, native)
    assert f32(source) == f32(native), (path, source, native, f32(source).hex(), f32(native).hex())
  else:
    assert source == native, (path, source, native)
