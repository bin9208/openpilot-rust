# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from __future__ import annotations

from collections.abc import Callable
import ctypes
from enum import StrEnum
import json
import os
import sys
from types import SimpleNamespace
from typing import TypedDict, assert_never

from openpilot.selfdrive.carrot.cluster import cluster_gles_readback as source


class Scenario(StrEnum):
  DIRECT_ERROR = "direct-error"
  DIRECT_SMALL = "direct-small"
  DIRECT_INVALID = "direct-invalid"
  UNCLEARED = "uncleared"
  ALLOCATION_ZERO = "allocation-zero"
  ALLOCATION_ERROR = "allocation-error"
  FENCE_ZERO = "fence-zero"
  READ_ERROR = "read-error"
  TIMEOUT = "timeout"
  WAIT_ERROR = "wait-error"
  UNEXPECTED = "unexpected"
  MAP_NULL = "map-null"
  MAP_CORRUPT = "map-corrupt"
  COPY_ERROR = "copy-error"
  COPY_SMALL = "copy-small"
  SUCCESS = "success"
  MISSING_ASYNC = "missing-async"


class Record(TypedDict, total=False):
  supported: bool
  can_enqueue: bool
  ready: bool
  bytes: list[int]
  value: bool | None
  error: str


def outcome(callback: Callable[[], bool | None]) -> Record:
  try:
    return {"value": callback()}
  except source.DirectNv12ReadbackError as error:
    return {"error": str(error)}


def main() -> None:
  provider = ctypes.CDLL(sys.argv[1])
  source.ctypes.CDLL = lambda _path: provider
  scenario = Scenario(os.environ["OWNED_GL_SCENARIO"])
  clock = 100.0
  source.time = SimpleNamespace(monotonic=lambda: clock)
  readback = source.GlesDirectReadback()
  data = bytearray([0xA5]) * 23
  view = (ctypes.c_ubyte * len(data)).from_buffer(data)
  address = ctypes.addressof(view)
  records: list[Record] = [{"supported": readback.async_supported, "can_enqueue": readback.async_can_enqueue()}]
  match scenario:
    case Scenario.DIRECT_ERROR:
      records.append(outcome(lambda: readback.read_rgba(7, 2, 2, address, 23)))
    case Scenario.DIRECT_SMALL:
      records.append(outcome(lambda: readback.read_rgba(7, 2, 2, address, 15)))
    case Scenario.DIRECT_INVALID:
      records.append(outcome(lambda: readback.read_rgba(0, 2, 2, address, 23)))
    case Scenario.UNCLEARED | Scenario.ALLOCATION_ZERO | Scenario.ALLOCATION_ERROR | Scenario.FENCE_ZERO | Scenario.READ_ERROR | Scenario.TIMEOUT | Scenario.WAIT_ERROR | Scenario.UNEXPECTED | Scenario.MAP_NULL | Scenario.MAP_CORRUPT | Scenario.COPY_ERROR | Scenario.COPY_SMALL | Scenario.SUCCESS | Scenario.MISSING_ASYNC:
      records.append(outcome(lambda: readback.enqueue_rgba(7, 2, 2)))
      match scenario:
        case Scenario.TIMEOUT:
          clock = 101.0
          records.append(outcome(readback.async_ready))
          clock = 101.0001
          records.append(outcome(readback.async_ready))
        case Scenario.WAIT_ERROR | Scenario.UNEXPECTED:
          records.append(outcome(readback.async_ready))
        case Scenario.MAP_NULL | Scenario.MAP_CORRUPT | Scenario.COPY_ERROR | Scenario.SUCCESS:
          records.append(outcome(lambda: readback.copy_ready(address, 23)))
        case Scenario.COPY_SMALL:
          records.append(outcome(lambda: readback.copy_ready(address, 15)))
        case Scenario.UNCLEARED | Scenario.ALLOCATION_ZERO | Scenario.ALLOCATION_ERROR | Scenario.FENCE_ZERO | Scenario.READ_ERROR | Scenario.MISSING_ASYNC:
          pass
        case unreachable:
          assert_never(unreachable)
    case unreachable:
      assert_never(unreachable)
  readback.close()
  records.append({"ready": readback.async_ready(), "can_enqueue": readback.async_can_enqueue(), "bytes": list(data)})
  print(json.dumps(records))


if __name__ == "__main__":
  main()
