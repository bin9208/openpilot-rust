# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from __future__ import annotations

import ctypes
import json
from pathlib import Path
import sys
from typing import TypedDict

from openpilot.selfdrive.carrot.cluster import cluster_gles_dmabuf as source


class Record(TypedDict, total=False):
  constructor_error: str
  import_error: str
  wait_error: str
  wait: bool
  dimensions: list[int]
  changed_dimensions: list[int]
  cache_reused: bool
  new_target: bool
  fd_delta: int


def fd_count() -> int:
  return len(list(Path("/proc/self/fd").iterdir()))


def main() -> None:
  provider_path, output = sys.argv[1:]
  output = Path(output)
  output.mkdir(parents=True, exist_ok=True)
  before = fd_count()
  host = provider_path == "HOST"
  if host:
    source.rl.set_trace_log_level(4)
    source.rl.set_config_flags(128)
    source.rl.init_window(32, 24, "Owned source DMA host fallback")
    before = fd_count()
  else:
    provider = ctypes.CDLL(provider_path)
    source.ctypes.CDLL = lambda _path: provider
  records: list[Record] = []
  try:
    pool = source.GlesDmabufRenderTargetPool()
  except source.DirectNv12DmabufError as error:
    records.append({"constructor_error": str(error)})
  else:
    try:
      with (output / "owned-buffer").open("w+b") as file:
        file.truncate(32)
        try:
          first = pool.target_for(file.fileno(), 8, 16)
        except source.DirectNv12DmabufError as error:
          records.append({"import_error": str(error)})
        else:
          repeated = pool.target_for(file.fileno(), 8, 16)
          changed = pool.target_for(file.fileno(), 16, 32)
          records.append({"dimensions": [first.texture.width, first.texture.height], "cache_reused": first.id == repeated.id, "changed_dimensions": [changed.texture.width, changed.texture.height], "new_target": first.id != changed.id})
          try:
            pool.wait_for_gpu()
          except source.DirectNv12DmabufError as error:
            records.append({"wait_error": str(error)})
          else:
            records.append({"wait": True})
      pool.close()
      pool.close()
    finally:
      pool.close()
  records.append({"fd_delta": fd_count() - before})
  print(json.dumps(records))
  if host:
    source.rl.close_window()


if __name__ == "__main__":
  main()
