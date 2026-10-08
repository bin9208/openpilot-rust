#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["httpx2[http2,brotli,zstd]==2.13.1"]
# ///
from __future__ import annotations

import argparse
from contextlib import ExitStack
from enum import StrEnum
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import time
from typing import Final, assert_never
import uuid

import httpx2

from xiaoge_qa.sockets import available_port, client, telemetry

ROOT: Final = Path(__file__).resolve().parents[2]


class Case(StrEnum):
  TCP_OCCUPIED = "tcp-occupied"
  HTTP_OCCUPIED = "http-occupied"
  MODELS_MISSING = "models-missing"
  CONFIG_MALFORMED = "config-malformed"
  CONFIG_OVERFLOW = "config-overflow"
  PARAMS_FILE = "params-file"
  PARTIAL_TCP = "partial-tcp"
  PARTIAL_HTTP = "partial-http"
  RECONNECT = "reconnect"
  SIGTERM = "sigterm"


def await_http(connection: httpx2.Client, process: subprocess.Popen[bytes]):
  deadline = time.monotonic() + 10
  while time.monotonic() < deadline:
    assert process.poll() is None, process.returncode
    try:
      response = connection.get("/api/status")
    except httpx2.ConnectError:
      time.sleep(.02)
      continue
    response.raise_for_status()
    status = response.json()
    if "unavailable" in status["camera"]["error"] and "unavailable" in status["lane"]["cameraError"]:
      return status
    time.sleep(.02)
  raise TimeoutError("Xiaoge lifecycle HTTP startup did not settle")


def run_peer(kind: str, case: Case, arguments: argparse.Namespace):
  output = arguments.output / case.value / kind
  output.mkdir(parents=True)
  prefix = "xf_" + uuid.uuid4().hex
  queue = Path("/dev/shm") / ("msgq_" + prefix)
  queue.mkdir()
  params = output / "params"
  config = output / "config.json"
  assets = arguments.assets
  tcp_port, http_port = available_port(), available_port()
  while tcp_port == http_port:
    http_port = available_port()
  environment = dict(os.environ, OPENPILOT_PREFIX=prefix, PARAMS_ROOT=str(params),
                     LD_LIBRARY_PATH=str(arguments.opencv / "install/lib"), PWD=str(output))
  try:
    with ExitStack() as stack:
      match case:
        case Case.TCP_OCCUPIED | Case.HTTP_OCCUPIED:
          occupied = stack.enter_context(socket.socket())
          occupied.bind(("127.0.0.1", tcp_port if case == Case.TCP_OCCUPIED else http_port))
          occupied.listen()
        case Case.MODELS_MISSING:
          assets = output / "missing-models"
          assets.mkdir()
        case Case.CONFIG_MALFORMED:
          config.write_text("{")
        case Case.CONFIG_OVERFLOW:
          config.write_text('{"width": Infinity}')
        case Case.PARAMS_FILE:
          params.write_text("not a directory")
        case Case.PARTIAL_TCP | Case.PARTIAL_HTTP | Case.RECONNECT | Case.SIGTERM:
          pass
        case unreachable:
          assert_never(unreachable)
      common = ["--assets", str(assets), "--config", str(config), "--tcp-port", str(tcp_port), "--http-port", str(http_port)]
      if kind == "source":
        command = [sys.executable, "-u", "-P", str(ROOT / "rust/tools/xiaoge_runtime_source.py"),
                   "--binding", str(arguments.binding), "--log-root", str(output / "logs"), *common]
      else:
        command = [str(arguments.binary), "--root", str(ROOT), "--device-ip", "127.0.0.9", *common]
      (output / "invocation.json").write_text(json.dumps({"argv": command, "prefix": prefix}, indent=2) + "\n")
      stdout = stack.enter_context((output / "stdout.log").open("wb"))
      stderr = stack.enter_context((output / "stderr.log").open("wb"))
      process = subprocess.Popen(command, cwd=output, env=environment, stdout=stdout, stderr=stderr)
      try:
        result = {"case": case.value}
        if case == Case.PARAMS_FILE:
          result["exit"] = process.wait(timeout=10)
          assert result["exit"] == 1
        else:
          if case not in [Case.HTTP_OCCUPIED, Case.CONFIG_OVERFLOW]:
            connection = stack.enter_context(client(http_port))
            status = await_http(connection, process)
            (output / "status.json").write_text(json.dumps(status, indent=2) + "\n")
            loaded = case != Case.MODELS_MISSING
            assert status["model"]["loaded"] == loaded and status["lane"]["loaded"] == loaded
            assert bool(status["model"]["error"]) != loaded and bool(status["lane"]["error"]) != loaded
            result["models_loaded"] = loaded
            if case == Case.CONFIG_MALFORMED:
              configuration = connection.get("/api/config").json()
              assert configuration["width"] == 1928 and configuration["height"] == 1208
              result["default_config"] = configuration
          if case != Case.TCP_OCCUPIED:
            deadline = time.monotonic() + 10
            while True:
              assert process.poll() is None
              try:
                packet = telemetry(tcp_port)
                break
              except ConnectionRefusedError:
                assert time.monotonic() < deadline
                time.sleep(.02)
            (output / "telemetry.json").write_text(json.dumps(packet, indent=2) + "\n")
            result["heartbeat"] = packet["heartbeat"]
          match case:
            case Case.PARTIAL_TCP:
              pending = stack.enter_context(socket.create_connection(("127.0.0.1", tcp_port), timeout=5))
              pending.sendall(b"\x00\x00")
            case Case.PARTIAL_HTTP:
              pending = stack.enter_context(socket.create_connection(("127.0.0.1", http_port), timeout=5))
              pending.sendall(b"POST /api/settings HTTP/1.1\r\nContent-Length: 16\r\n\r\n{")
            case Case.RECONNECT:
              for _ in range(32):
                with socket.create_connection(("127.0.0.1", tcp_port), timeout=5) as transient:
                  transient.sendall(b"\x00")
              telemetry(tcp_port)
              result["reconnects"] = 32
            case Case.HTTP_OCCUPIED | Case.CONFIG_OVERFLOW:
              deadline = time.monotonic() + 10
              while not (output / "stderr.log").stat().st_size:
                assert process.poll() is None
                assert time.monotonic() < deadline
                time.sleep(.02)
            case Case.TCP_OCCUPIED | Case.MODELS_MISSING | Case.CONFIG_MALFORMED | Case.SIGTERM | Case.PARAMS_FILE:
              pass
            case unreachable:
              assert_never(unreachable)
          assert process.poll() is None
          process.send_signal(signal.SIGTERM if case == Case.SIGTERM else signal.SIGINT)
          result["exit"] = process.wait(timeout=5)
          assert result["exit"] == (-signal.SIGTERM if case == Case.SIGTERM else 0), result
        assert not Path(f"/proc/{process.pid}").exists()
        (output / "receipt.json").write_text(json.dumps(result, indent=2) + "\n")
        return result
      finally:
        if process.poll() is None:
          process.kill()
          process.wait(timeout=5)
  finally:
    shutil.rmtree(queue)


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare original and Rust Xiaoge real startup failure, connected-client shutdown and reconnect behavior.")
  for name in ["binary", "binding", "assets", "opencv", "output"]:
    parser.add_argument("--" + name, type=Path, required=True)
  args = parser.parse_args()
  for name in ["binary", "binding", "assets", "opencv", "output"]:
    setattr(args, name, getattr(args, name).resolve())
  assert shutil.disk_usage(args.output.parent).free > 25 * 2**30 + 128 * 2**20
  args.output.mkdir(parents=True, exist_ok=False)
  rows = []
  for case in Case:
    source, native = [run_peer(kind, case, args) for kind in ["source", "native"]]
    row = {"case": case.value, "source": source, "native": native, "equal": source == native}
    rows.append(row)
    (args.output / "cases.json").write_text(json.dumps(rows, indent=2) + "\n")
    print(case.value, "PASS" if row["equal"] else "FAIL", flush=True)
  result = {"status": "PASS" if all(row["equal"] for row in rows) else "FAIL", "cases": len(rows),
            "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(), "scope": "host process/socket lifecycle; no vehicle"}
  (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  assert result["status"] == "PASS"
  print(json.dumps(result))


if __name__ == "__main__":
  main()
