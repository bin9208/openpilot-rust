#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["httpx2[http2,brotli,zstd]==2.13.1"]
# ///
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
from typing import Final
import uuid

ROOT: Final = Path(__file__).resolve().parents[2]


def peer(arguments: argparse.Namespace):
  from card_runtime_source import load_binding
  load_binding(arguments.binding)
  from xiaoge_qa.live_cases import CONFIG, exercise
  from xiaoge_qa.live_input import Inputs
  from xiaoge_qa.live_probe import Probe
  from xiaoge_qa.sockets import available_port, client
  output = arguments.output
  (output / "config.json").write_text(json.dumps(CONFIG) + "\n")
  tcp_port, http_port = available_port(), available_port()
  while tcp_port == http_port:
    http_port = available_port()
  common = ["--assets", str(arguments.assets), "--config", str(output / "config.json"),
            "--tcp-port", str(tcp_port), "--http-port", str(http_port)]
  if arguments.peer == "source":
    command = [sys.executable, "-u", "-P", str(ROOT / "rust/tools/xiaoge_runtime_source.py"),
               "--binding", str(arguments.binding), "--log-root", str(output / "logs"), *common]
  else:
    command = [str(arguments.binary), "--root", str(ROOT), "--device-ip", "127.0.0.9", *common]
  (output / "invocation.json").write_text(json.dumps({"argv": command, "prefix": os.environ["OPENPILOT_PREFIX"]}, indent=2) + "\n")
  inputs = Inputs(output)
  try:
    from openpilot.common.params import Params
    car_params = Path(Params().get_param_path("CarParams"))
    car_params.unlink()
    car_params.mkdir()
    with (output / "stdout.log").open("wb") as stdout, (output / "stderr.log").open("wb") as stderr:
      process = subprocess.Popen(command, cwd=output, stdout=stdout, stderr=stderr)
      try:
        with client(http_port) as connection:
          probe = Probe(output, connection, process, inputs)
          result = exercise(probe, inputs, tcp_port, output)
          mappings = Path(f"/proc/{process.pid}/maps").read_text()
          (output / "maps.txt").write_text(mappings)
          if arguments.peer == "native":
            assert "libpython" not in mappings
            assert hashlib.sha256(Path(f"/proc/{process.pid}/exe").read_bytes()).digest() == hashlib.sha256(arguments.binary.read_bytes()).digest()
          probe.drain()
          process.send_signal(signal.SIGINT)
          exit_code = process.wait(timeout=5)
          assert exit_code == 0, exit_code
          (output / "receipt.json").write_text(json.dumps({"cases": result, "publications": len(probe.publications), "exit": exit_code}, indent=2) + "\n")
      finally:
        if process.poll() is None:
          process.kill()
          process.wait(timeout=5)
  finally:
    inputs.close()


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare original and Rust Xiaoge with real Cereal, Tesla CAN and VisionIPC inputs.")
  for name in ["binary", "binding", "assets", "opencv", "output"]:
    parser.add_argument("--" + name, type=Path, required=True)
  parser.add_argument("--peer", choices=["source", "native"])
  args = parser.parse_args()
  for name in ["binary", "binding", "assets", "opencv", "output"]:
    setattr(args, name, getattr(args, name).resolve())
  if args.peer:
    peer(args)
    return
  assert shutil.disk_usage(args.output.parent).free > 25 * 2**30 + 128 * 2**20
  args.output.mkdir(parents=True, exist_ok=False)
  for kind in ["source", "native"]:
    output = args.output / kind
    output.mkdir()
    prefix = "xiaoge_live_" + uuid.uuid4().hex
    queue = Path("/dev/shm") / ("msgq_" + prefix)
    queue.mkdir()
    environment = dict(os.environ, OPENPILOT_PREFIX=prefix, PARAMS_ROOT=str(output / "params"),
                       LD_LIBRARY_PATH=str(args.opencv / "install/lib"), PWD=str(output))
    command = [sys.executable, "-u", "-P", str(Path(__file__).resolve()), "--peer", kind]
    for name in ["binary", "binding", "assets", "opencv"]:
      command.extend(["--" + name, str(getattr(args, name))])
    command.extend(["--output", str(output)])
    try:
      run = subprocess.run(command, env=environment, cwd=output, check=False)
      run.check_returncode()
    finally:
      shutil.rmtree(queue)
  source, native = [json.loads((args.output / kind / "receipt.json").read_text()) for kind in ["source", "native"]]
  failures = [{"source": wanted, "native": got} for wanted, got in zip(source["cases"], native["cases"], strict=True) if wanted != got]
  result = {"status": "FAIL" if failures else "PASS", "cases": len(source["cases"]), "failures": failures,
    "publications": {"source": source["publications"], "native": native["publications"]},
    "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    "scope": "real source and Rust processes; source Cereal/CAN/VisionIPC peers, actual original ONNX models, snapshots and normal shutdown; host only"}
  (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  assert not failures, json.dumps(failures)
  print(json.dumps(result))


if __name__ == "__main__":
  main()
