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
import time
import uuid

import httpx2

from xiaoge_qa.http_cases import cases
from xiaoge_qa.settings_phase import observed_settings, requested_settings, wait_settings_reload
from xiaoge_qa.sockets import available_port, client, raw_http, telemetry, wait_ready

ROOT = Path(__file__).resolve().parents[2]


def run_peer(kind: str, arguments: argparse.Namespace):
  output = arguments.output / kind
  output.mkdir()
  prefix = "xiaoge_" + uuid.uuid4().hex
  environment = dict(os.environ, OPENPILOT_PREFIX=prefix, PARAMS_ROOT=str(output / "params"),
                     LD_LIBRARY_PATH=str(arguments.opencv / "install/lib"), PWD=str(output))
  queue = Path("/dev/shm") / ("msgq_" + prefix)
  queue.mkdir()
  tcp_port, http_port = available_port(), available_port()
  while tcp_port == http_port:
    http_port = available_port()
  common = ["--assets", str(arguments.assets), "--config", str(output / "config.json"),
            "--tcp-port", str(tcp_port), "--http-port", str(http_port)]
  if kind == "source":
    command = [sys.executable, "-u", "-P", str(ROOT / "rust/tools/xiaoge_runtime_source.py"),
               "--binding", str(arguments.binding), "--log-root", str(output / "logs"), *common]
  else:
    command = [str(arguments.binary), "--root", str(ROOT), "--device-ip", "127.0.0.9", *common]
  (output / "invocation.json").write_text(json.dumps({"argv": command, "prefix": prefix, "params": environment["PARAMS_ROOT"]}, indent=2) + "\n")
  records = []
  result = {}
  with (output / "stdout.log").open("wb") as stdout, (output / "stderr.log").open("wb") as stderr:
    process = subprocess.Popen(command, cwd=output, env=environment, stdout=stdout, stderr=stderr)
    try:
      with client(http_port) as connection:
        wait_ready(connection, process)
        result["tcp"] = telemetry(tcp_port)
        for case in cases():
          if case.name == "settings-valid":
            prime = connection.request(case.method, case.path, content=case.body)
            (output / "settings-prime.json").write_text(json.dumps({"status": prime.status_code, "body": prime.text}, indent=2) + "\n")
            prime.raise_for_status()
            wait_settings_reload(connection, case.body, output / "settings-prime-reload.json")
          start = time.monotonic()
          try:
            response = (raw_http(http_port, case.raw) if case.raw is not None else
                        connection.request(case.method, case.path, content=case.body))
            body = response.content
            record = {"name": case.name, "status": response.status_code, "content_type": response.headers.get("Content-Type"),
                      "cache_control": response.headers.get("Cache-Control")}
            if "application/json" in response.headers.get("Content-Type", ""):
              record["json"] = response.json()
            else:
              record["body"] = body.decode()
            assert int(response.headers["Content-Length"]) == len(body), case.name
          except httpx2.RemoteProtocolError:
            record = {"name": case.name, "closed": True}
          elapsed = time.monotonic() - start
          if case.name.endswith("-timeout"):
            assert 4.8 <= elapsed < 7.0, (case.name, elapsed)
          records.append(record)
          (output / "responses.json").write_text(json.dumps(records, indent=2) + "\n")
          with (output / "timings.jsonl").open("a") as timings:
            timings.write(json.dumps({"name": case.name, "seconds": elapsed}) + "\n")
          print(kind, case.name, record.get("status", "closed"), flush=True)
          if case.name == "settings-valid":
            assert observed_settings(body) == requested_settings(case.body), (kind, case.name, record)
            wait_settings_reload(connection, case.body, output / "settings-compared-reload.json")
      assert process.poll() is None, process.returncode
      process.send_signal(signal.SIGINT)
      result["exit"] = process.wait(timeout=5)
      assert result["exit"] == 0
      settings = output / "params" / prefix
      result["params"] = {path.name: path.read_text() for path in settings.iterdir() if path.is_file() and path.name.startswith("Onnx")}
      result["responses"] = records
      (output / "receipt.json").write_text(json.dumps(result, indent=2) + "\n")
      return result
    finally:
      if process.poll() is None:
        process.kill()
        process.wait(timeout=5)
      if queue.exists():
        shutil.rmtree(queue)


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare real original and Rust Xiaoge startup, HTTP, TCP and shutdown without a vehicle.")
  for name in ["binary", "binding", "assets", "opencv", "output"]:
    parser.add_argument("--" + name, type=Path, required=True)
  arguments = parser.parse_args()
  for name in ["binary", "binding", "assets", "opencv", "output"]:
    setattr(arguments, name, getattr(arguments, name).resolve())
  assert shutil.disk_usage(arguments.output.parent).free > 25 * 2**30 + 128 * 2**20
  arguments.output.mkdir(parents=True, exist_ok=False)
  source = run_peer("source", arguments)
  native = run_peer("native", arguments)
  failures = []
  for wanted, actual in zip(source["responses"], native["responses"], strict=True):
    if wanted != actual:
      failures.append({"source": wanted, "native": actual})
  if source["params"] != native["params"]:
    failures.append({"source_params": source["params"], "native_params": native["params"]})
  result = {"status": "FAIL" if failures else "PASS", "cases": len(source["responses"]), "failures": failures,
    "binary_sha256": hashlib.sha256(arguments.binary.read_bytes()).hexdigest(),
    "source_sha256": {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in
      ["openpilot/selfdrive/carrot/xiaoge_data.py", "openpilot/selfdrive/carrot/xiaoge/v_asm_server.py"]},
    "scope": "original and native real standalone process HTTP/TCP startup, no-camera snapshots and SIGINT; excludes camera/Cereal IPC verification"}
  (arguments.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  assert not failures, json.dumps(failures)
  print(json.dumps(result))


if __name__ == "__main__":
  main()
