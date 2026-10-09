"""Run unchanged source smoke inputs against an owned protocol worker."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
from pytest import MonkeyPatch


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("model", type=Path)
  parser.add_argument("--worker", type=Path, required=True)
  parser.add_argument("--camera", action="append", choices=("1928x1208", "1344x760"))
  args = parser.parse_args()
  from openpilot.selfdrive.modeld import precompiled_runner

  popen = subprocess.Popen

  def launch(command: list[str], *, stdin: int, stdout: int, bufsize: int) -> subprocess.Popen[bytes]:
    return popen([str(args.worker), *command[-4:]], stdin=stdin, stdout=stdout, bufsize=bufsize)

  sizes = tuple(tuple(int(part) for part in camera.split("x")) for camera in (args.camera or ("1928x1208", "1344x760")))
  with MonkeyPatch.context() as monkey:
    monkey.setattr(precompiled_runner.subprocess, "Popen", launch)
    reports = precompiled_runner.smoke_test(args.model, camera_sizes=sizes)
  print(json.dumps(reports))


if __name__ == "__main__":
  main()
