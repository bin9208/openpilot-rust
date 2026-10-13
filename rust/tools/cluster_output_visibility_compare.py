# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import time
from typing import BinaryIO, TypedDict

from cluster_gles_readback_compare import ComparisonFailure, ProcessFailure
from cluster_output_visibility_source import Stage


class StageMessage(TypedDict):
  stage: str
  profile_names: list[str]


def read_stage(process: subprocess.Popen[bytes], log: BinaryIO) -> StageMessage:
  assert process.stdout is not None
  deadline = time.monotonic() + 10
  data = b""
  with selectors.DefaultSelector() as selector:
    selector.register(process.stdout, selectors.EVENT_READ)
    while time.monotonic() < deadline:
      if not selector.select(deadline - time.monotonic()):
        raise ProcessFailure("protocol timeout", Path(log.name))
      chunk = os.read(process.stdout.fileno(), 65536)
      if not chunk:
        raise ProcessFailure("protocol EOF", Path(log.name))
      log.write(chunk)
      log.flush()
      data += chunk
      for line in data.splitlines():
        if line.startswith(b'{'):
          return json.loads(line)
  raise ProcessFailure("protocol timeout", Path(log.name))


def main() -> None:
  root, output, native = (Path(value).resolve() for value in sys.argv[1:4])
  output.mkdir(parents=True, exist_ok=True)
  input_path = output / "input.json"
  source_command = [sys.executable, str(Path(__file__).with_name("cluster_output_visibility_source.py")), str(root), str(input_path)]
  if len(sys.argv) == 5:
    source_command.append(sys.argv[4])
  native_command = [str(native), str(root), str(input_path)]
  commands, observations = [], []
  for name, command in (("source", source_command), ("native", native_command)):
    states = []
    with (output / f"{name}.log").open("wb") as log:
      with subprocess.Popen(command, cwd=root, env=os.environ.copy(), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log) as process:
        assert process.stdin is not None
        try:
          for expected in Stage:
            message = read_stage(process, log)
            stage = Stage(message["stage"])
            if stage != expected:
              raise ProcessFailure("stage ordering", output / f"{name}.log")
            invocation = ["xwininfo", "-name", "Owned output visibility"]
            query = subprocess.run(invocation, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=5, check=False)
            (output / f"{name}-{stage.value}.xwininfo").write_text(query.stdout)
            if query.returncode:
              raise ProcessFailure("xwininfo", output / f"{name}-{stage.value}.xwininfo")
            state = next(line.split(":", 1)[1].strip() for line in query.stdout.splitlines() if "Map State:" in line)
            states.append({"stage": stage.value, "map_state": state,"profile_names":message["profile_names"]})
            process.stdin.write(b"next\n")
            process.stdin.flush()
          result = process.wait(timeout=10)
        finally:
          if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)
    commands.append({"command": command, "returncode": result})
    if result:
      raise ProcessFailure(name, output / f"{name}.log")
    observations.append(states)
  report = {"commands": commands, "native_sha256": hashlib.sha256(native.read_bytes()).hexdigest(), "source": observations[0], "native": observations[1], "exact": observations[0] == observations[1]}
  path = output / "comparison.json"
  path.write_text(json.dumps(report, indent=2) + "\n")
  if not report["exact"]:
    raise ComparisonFailure(path)
  print(json.dumps(report))


if __name__ == "__main__":
  main()
