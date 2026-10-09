from __future__ import annotations

import argparse
from enum import StrEnum
from hashlib import sha256
import json
import os
from pathlib import Path
import shutil
import socket
import ssl
import subprocess
import sys
from threading import Thread
import time
from typing import TypedDict, assert_never

from check_usbgpu_model_delivery import certificates
from usbgpu_background_http import Handler, Server
from usbgpu_boot_fixture import Json, setup


class Case(StrEnum):
  UNKNOWN = "unknown-hardware"
  HISTORY = "remembered-hardware"
  PRESENT = "present-hardware"
  USB2 = "usb2"
  BOOL_IO = "history-read-write-io"
  COMPILED = "genuine-compiled"
  ACTIVE = "active-state-skips-network-wait"
  INVALID = "invalid-manifest"
  SHORT = "short-payload"
  NETWORK = "network-wait-failure"
  STATUS = "status-error"
  PROGRESS = "status-during-download"
  PARAMS = "params-constructor-error"
  STATE = "state-read-error"
  CHUNKED = "truncated-chunked-body"


class Partial(TypedDict):
  bytes: int
  sha256: str


class Observable(TypedDict):
  status: dict[str, Json] | None
  state: dict[str, Json] | None
  partials: dict[str, Partial]
  final_files: dict[str, int]
  history: str | None


def observable(root: Path) -> Observable:
  cache, history = root / "cache", root / "params/d/UsbGpuHardwareSeen"
  status = json.loads((cache / "status.json").read_text()) if (cache / "status.json").is_file() else None
  if status:
    status.pop("started_at")
    status.pop("updated_at")
    status.pop("detail", None)
  state = json.loads((cache / "state.json").read_text()) if (cache / "state.json").is_file() else None
  partials = {path.name: {"bytes": path.stat().st_size, "sha256": sha256(path.read_bytes()).hexdigest()} for path in cache.glob("*.part")}
  final_files = {path.name: path.stat().st_size for path in cache.glob("*.pkl")}
  return {
    "status": status,
    "state": state,
    "partials": partials,
    "final_files": final_files,
    "history": history.read_text() if history.is_file() else "directory" if history.is_dir() else None,
  }


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare original optional updater and native command on owned TLS/Params/files")
  for name in ("binary", "binding", "installed", "metadata", "worker", "package", "evidence"):
    parser.add_argument(f"--{name}", type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  ca, cert, key = certificates(args.evidence)
  server = Server(("localhost", 0), Handler)
  context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
  context.load_cert_chain(cert, key)
  server.socket = context.wrap_socket(server.socket, server_side=True)
  url = f"https://localhost:{server.server_port}"
  thread = Thread(target=server.serve_forever)
  thread.start()
  binary_sha = sha256(args.binary.read_bytes()).hexdigest()
  rows = []
  try:
    for case in Case:
      pair = {}
      for side in ("source", "native"):
        root = args.evidence / case / side
        root.mkdir(parents=True)
        cache, params, devices = root / "cache", root / "params/d", root / "devices"
        cache.mkdir()
        params.mkdir(parents=True)
        devices.mkdir()
        payload = b"owned optional model"
        model = {"model_id": "owned", "filename": "model.pkl", "size": len(payload), "sha256": sha256(payload).hexdigest(), "url": url + "/model.pkl"}
        wait, manifest_url = 0.05, url + "/manifest.json"
        server.mode = "ordinary"
        match case:
          case Case.COMPILED:
            cache.rmdir()
            devices.rmdir()
            fixture = setup(root, args.installed, args.worker, args.metadata)
            shutil.rmtree(fixture.devices / "owned-usb")
            model = json.loads((cache / "state.json").read_text())["active"]
          case Case.PRESENT | Case.BOOL_IO:
            device = devices / "owned-usb"
            device.mkdir()
            for name, value in {"idVendor": "add1", "idProduct": "0001", "speed": "5000"}.items():
              (device / name).write_text(value)
            if case == Case.BOOL_IO:
              (params / "UsbGpuHardwareSeen").mkdir()
          case Case.USB2:
            device = devices / "owned-usb"
            device.mkdir()
            for name, value in {"idVendor": "add1", "idProduct": "0001", "speed": "480"}.items():
              (device / name).write_text(value)
          case Case.UNKNOWN:
            pass
          case Case.STATE:
            (cache / "state.json").mkdir()
          case Case.PARAMS:
            params.rmdir()
            params.parent.rmdir()
            params.parent.write_bytes(b"blocked")
          case Case.HISTORY | Case.ACTIVE | Case.INVALID | Case.SHORT | Case.NETWORK | Case.STATUS | Case.PROGRESS | Case.CHUNKED:
            (params / "UsbGpuHardwareSeen").write_text("1")
          case unreachable:
            assert_never(unreachable)
        if case in (Case.ACTIVE, Case.INVALID):
          (cache / f"model-{model['sha256'][:16]}.pkl").write_bytes(payload)
          (cache / "state.json").write_text(json.dumps({"active": model, "previous": None}))
        if case == Case.STATUS:
          (cache / "status.json").mkdir()
        if case in (Case.ACTIVE, Case.NETWORK):
          with socket.socket() as reserved:
            reserved.bind(("localhost", 0))
            manifest_url = f"https://localhost:{reserved.getsockname()[1]}/manifest.json"
          if case == Case.ACTIVE:
            wait = 3.0
        server.mode = {Case.INVALID: "invalid", Case.SHORT: "short", Case.PROGRESS: "status-io", Case.CHUNKED: "chunked"}.get(case, "ordinary")
        server.manifest, server.payload, server.cache, server.records = model, payload, cache, []
        env = dict(os.environ, PARAMS_ROOT=str(params.parent), OPENPILOT_PREFIX="d", CARROT_BIG_MODEL_DIR=str(cache), SSL_CERT_FILE=str(ca))
        if side == "source":
          argv = [sys.executable, "-P", str(Path(__file__).with_name("usbgpu_background_source.py"))]
          request = json.dumps({"root": str(root), "binding": str(args.binding), "url": manifest_url, "wait": wait})
        else:
          argv = [
            str(args.binary),
            "--ensure-if-egpu",
            "--root",
            str(root),
            "--devices",
            str(devices),
            "--assets",
            str(args.package),
            "--manifest-url",
            manifest_url,
            "--network-wait-seconds",
            str(wait),
          ]
          request = None
        started = time.monotonic()
        result = subprocess.run(argv, input=request, env=env, text=True, capture_output=True, check=False, timeout=40)
        pair[side] = {
          "argv": argv,
          "request": request,
          "exit": result.returncode,
          "stdout": result.stdout,
          "stderr": result.stderr,
          "seconds": time.monotonic() - started,
          "requests": server.records,
          "observable": observable(root),
        }
      expected, actual = pair["source"], pair["native"]
      equal = expected["exit"] == actual["exit"] and expected["observable"] == actual["observable"] and expected["requests"] == actual["requests"]
      rows.append({"name": case, **pair, "equal": equal})
      (args.evidence / "result.json").write_text(
        json.dumps({"rows": rows, "binary_sha256": binary_sha, "differences": sum(not row["equal"] for row in rows)}, indent=2) + "\n"
      )
      assert equal, rows[-1]
      assert actual["exit"] == (1 if case in (Case.STATUS, Case.PROGRESS, Case.PARAMS, Case.STATE) else 0)
      assert "panicked" not in actual["stderr"]
      if case in (Case.STATUS, Case.PROGRESS, Case.STATE):
        assert "IsADirectoryError: [Errno 21]" in expected["stderr"]
        assert actual["stderr"].strip() == "Is a directory (os error 21)"
      if case == Case.PARAMS:
        assert "RuntimeError: Failed to ensure params path, errno=20" in expected["stderr"]
        assert actual["stderr"].strip() == "File exists (os error 17)"
      if case == Case.SHORT:
        assert "model size mismatch: expected 20, got 3" in expected["stderr"] and expected["stderr"] == actual["stderr"]
        assert actual["observable"]["final_files"] == {}
      if case == Case.ACTIVE:
        assert actual["seconds"] < 2.0 and actual["observable"]["state"]
  finally:
    server.shutdown()
    server.server_close()
    thread.join(timeout=2)
  assert sha256(args.binary.read_bytes()).hexdigest() == binary_sha
  print(json.dumps({"cases": len(rows), "differences": 0, "scope": "owned TLS/real Cython Params; error wording and wall-clock timestamps excluded"}))


if __name__ == "__main__":
  main()
