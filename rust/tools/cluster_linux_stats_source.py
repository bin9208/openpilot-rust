# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from __future__ import annotations

from dataclasses import asdict
import json
from pathlib import Path
import sys
from typing import TypedDict

from openpilot.selfdrive.carrot.cluster import cluster_system_monitor as source


class Step(TypedDict):
  memory: list[int] | None
  cpu: list[int] | None


def main() -> None:
  input_path, output = (Path(value).resolve() for value in sys.argv[1:])
  output.mkdir(parents=True, exist_ok=True)
  source.PROC_MEMINFO_PATH = output / "meminfo"
  source.PROC_STAT_PATH = output / "stat"
  request = json.loads(input_path.read_text())
  sampler = source.SystemStatsSampler()
  snapshots = []
  try:
    for step in request["steps"]:
      for name, data in (("meminfo", step["memory"]), ("stat", step["cpu"])):
        path = output / name
        if data is None:
          path.unlink(missing_ok=True)
        else:
          path.write_bytes(bytes(data))
      sample = sampler._sample_linux()
      result = asdict(sample) if sample is not None else None
      if result is not None:
        result["disk_used_percent"] = None
      snapshots.append(result)
  finally:
    sampler.close()
  print(json.dumps(snapshots))


if __name__ == "__main__":
  main()
