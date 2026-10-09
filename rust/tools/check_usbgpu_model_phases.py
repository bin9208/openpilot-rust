"""Original status throttle and boot integrity on owned files and HTTPS recipients."""

from __future__ import annotations

import argparse
from copy import deepcopy
from dataclasses import asdict
import json
import os
from pathlib import Path
import ssl
from threading import Thread
from urllib.error import HTTPError

from pytest import MonkeyPatch

from check_usbgpu_model_delivery import certificates
from check_usbgpu_provision_contract import Handler, Json, Server, invoke


def status(binary: Path, root: Path) -> dict[str, Json]:
  from openpilot.selfdrive.modeld import big_model_status

  model = {"model_id": "owned", "filename": "model.pkl", "size": 100, "sha256": "a" * 64, "url": "https://owned.invalid/model.pkl"}
  phases = ("checking", "downloading", "downloading", "downloading", "verifying", "ready", "waiting_for_ignition", "compiling", "compiled", "error")
  events = []
  for index, phase in enumerate(phases):
    events.append(
      {
        "phase": phase,
        "wall": 10.0 + index,
        "monotonic": 5.0 + (index * 0.1 if index < 2 else index),
        "force": index not in (1, 2),
        "downloaded": 100 if index >= 3 else 1,
      }
    )
  source_root, native_root = root / "source", root / "native"
  clock = {"wall": 10.0, "monotonic": 0.0}
  rows = []
  with MonkeyPatch.context() as monkey:
    monkey.setattr(big_model_status.time, "time", lambda: clock["wall"])
    monkey.setattr(big_model_status.time, "monotonic", lambda: clock["monotonic"])
    reporter = big_model_status.BigModelStatusReporter(source_root)
    for event in events:
      clock.update(wall=event["wall"], monotonic=event["monotonic"])
      rows.append(
        reporter.update(
          event["phase"],
          force=event["force"],
          model_id=model["model_id"],
          sha256=model["sha256"],
          downloaded_bytes=event["downloaded"],
          total_bytes=model["size"],
        )
      )
  original = {"rows": rows, "last": big_model_status.read_big_model_status(source_root)}
  actual = invoke(binary, {"action": "status", "cache": str(native_root), "model": model, "value": {"initial_wall": 10.0, "events": events}})
  return {"events": events, "original": original, "native": actual, "equal": actual.get("result") == original}


class Failure(Handler):
  def do_GET(self) -> None:
    self.server.records.append({"path": self.path, "encoding": self.headers.get("Accept-Encoding"), "agent": self.headers.get("User-Agent")})
    self.send_response(500)
    self.send_header("Content-Length", "0")
    self.end_headers()


def rehash(binary: Path, root: Path, package: Path, cache: Path) -> dict[str, Json]:
  from openpilot.selfdrive.modeld import big_model, precompiled_model

  root.mkdir()
  ca, cert, key = certificates(root)
  os.environ["SSL_CERT_FILE"] = str(ca)
  existing = next((cache / "precompiled").glob("*/installed.json"))
  marker = json.loads(existing.read_text())
  with Server(("127.0.0.1", 0), Failure) as server:
    server.records = []
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert, key)
    server.socket = context.wrap_socket(server.socket, server_side=True)
    thread = Thread(target=server.serve_forever)
    thread.start()
    try:
      url = f"https://localhost:{server.server_port}/model.pkl"
      model = big_model.BigModelManifest("owned", "model.pkl", marker["pickle"]["size"], marker["pickle"]["sha256"], url)
      for side in ("source", "native"):
        directory = root / side / "precompiled" / model.sha256
        runtime = directory / marker["runtime_directory"]
        (runtime / "tinygrad").mkdir(parents=True)
        (runtime / "examples/openpilot").mkdir(parents=True)
        (runtime / "tinygrad/__init__.py").touch()
        (runtime / "examples/openpilot/compile_warp.py").touch()
        changed = deepcopy(marker)
        changed["catalog_url"] = url.replace("model.pkl", "precompiled.json")
        changed["pickle"]["url"] = url
        changed["runtime"]["url"] = url.replace("model.pkl", "runtime.tar.gz")
        (directory / "installed.json").write_text(json.dumps(changed))
        # Sparse, same-size corrupt storage; never mutate the shared pinned PKL inode.
        with (directory / "model.pkl").open("wb") as target:
          target.truncate(model.size)
        with (root / side / model.cache_filename).open("wb") as target:
          target.truncate(model.size)
        os.link(existing.parent / "runtime.tar.gz", directory / "runtime.tar.gz")
        (root / side / "state.json").write_text(json.dumps({"active": asdict(model), "previous": None}))
      source_ready = precompiled_model.installed(model, root / "source") is not None
      native_ready = invoke(
        binary, {"action": "ready", "cache": str(root / "native"), "model": asdict(model), "value": {"models": str(root / "models"), "assets": str(package)}}
      )
      (root / "before-boot.json").write_text(json.dumps({"source_ready": source_ready, "native_ready": native_ready}, indent=2) + "\n")
      assert source_ready and native_ready["result"]["compiled"]
      try:
        precompiled_model.ensure_precompiled(model, root / "source")
      except HTTPError as error:
        original_error = error.code
      source_requests = list(server.records)
      server.records.clear()
      actual = invoke(binary, {"action": "install", "cache": str(root / "native"), "model": asdict(model), "value": str(package)})
      return {
        "source_ready_before_boot_hash": source_ready,
        "native_ready_before_boot_hash": native_ready,
        "original_http_error": original_error,
        "native": actual,
        "source_requests": source_requests,
        "native_requests": server.records,
        "equal": original_error == 500 and "error" in actual and source_requests == server.records,
        "scope": "Owned sparse corruption forces artifact rehash before retry; pinned model stays untouched.",
      }
    finally:
      server.shutdown()
      thread.join(2)
      assert not thread.is_alive()


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--evidence", type=Path, required=True)
  parser.add_argument("--package", type=Path, required=True)
  parser.add_argument("--cache", type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  rows = [status(args.binary, args.evidence / "status"), rehash(args.binary, args.evidence / "rehash", args.package, args.cache)]
  result = {"rows": rows, "differences": sum(not row["equal"] for row in rows)}
  (args.evidence / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  print(json.dumps({"cases": len(rows), "differences": result["differences"]}))
  assert result["differences"] == 0


if __name__ == "__main__":
  main()
