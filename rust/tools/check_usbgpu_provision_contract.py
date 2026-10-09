"""Focused original provisioning composition and native package readiness controls."""

from __future__ import annotations

import argparse
from copy import deepcopy
from dataclasses import asdict
from hashlib import sha256
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import ssl
import subprocess
from threading import Thread
from typing import NotRequired, TypeAlias, TypedDict, assert_never
from urllib.parse import urljoin

from check_usbgpu_model_delivery import certificates, files

Json: TypeAlias = bool | int | float | str | None | list["Json"] | dict[str, "Json"]


class Native(TypedDict):
  result: NotRequired[dict[str, Json]]
  error: NotRequired[str]
  progress: list[list[int]]


def invoke(binary: Path, request: dict[str, Json]) -> Native:
  result = subprocess.run([str(binary)], input=json.dumps(request), text=True, capture_output=True, check=False, timeout=35)
  assert result.returncode == 0, (result.returncode, result.stderr)
  return json.loads(result.stdout)


def catalog(model: dict[str, Json]) -> dict[str, Json]:
  return {
    "protocol": 1,
    "format": "comma-generic-onnx",
    "model_sha256": model["sha256"],
    "gpu_arch": "gfx1200",
    "frame_skip": 4,
    "camera_resolutions": [[1928, 1208], [1344, 760]],
    "pickle": {"url": "model.pkl", "sha256": model["sha256"], "size": model["size"]},
    "runtime": {"url": "runtime.tar.gz", "sha256": "b" * 64, "size": 1},
  }


def authorities(binary: Path, root: Path) -> list[dict[str, Json]]:
  from openpilot.selfdrive.modeld.big_model import BigModelManifest
  from openpilot.selfdrive.modeld.precompiled_model import validate_catalog

  rows = []
  for authority in ("MODELS.test", "models.test:443"):
    model = {"model_id": "owned", "filename": "model.pkl", "size": 1, "sha256": "a" * 64, "url": f"https://{authority}/model.pkl"}
    value = catalog(model)
    original = BigModelManifest.from_dict(model)
    url = urljoin(original.url, "precompiled.json")
    expected = {"model": asdict(original), "url": url, "catalog": validate_catalog(deepcopy(value), original.sha256, url)}
    actual = invoke(binary, {"action": "catalog-model", "cache": str(root), "model": model, "value": {"model": model, "catalog": value}})
    rows.append({"name": authority, "original": expected, "native": actual, "equal": actual.get("result") == expected})
  return rows


def assets(binary: Path, root: Path, package: Path) -> list[dict[str, Json]]:
  manifest_path = package / "manifest.json"
  saved = manifest_path.read_bytes()
  manifest = json.loads(saved)
  model = {"model_id": "owned", "filename": "model.pkl", "size": 776634338, "sha256": manifest["model_sha256"], "url": "https://owned.invalid/model.pkl"}
  request = {"action": "assets", "cache": str(root), "model": model, "value": str(package)}
  probe = package / "probe-gfx1200.json"
  original_probe = probe.read_bytes()
  rows = []
  try:
    omitted = deepcopy(manifest)
    omitted["files"] = [entry for entry in omitted["files"] if not entry["path"].startswith("warp-qcom-")]
    manifest_path.write_text(json.dumps(omitted))
    actual = invoke(binary, request)
    rows.append({"name": "missing-QCOM-manifest-closure", "native": actual, "equal": "error" in actual})
    changed = json.loads(original_probe)
    changed["arch"] = "gfx1100"
    probe.write_text(json.dumps(changed))
    wrong_probe = deepcopy(manifest)
    entry = next(item for item in wrong_probe["files"] if item["path"] == "probe-gfx1200.json")
    entry.update(bytes=probe.stat().st_size, sha256=sha256(probe.read_bytes()).hexdigest())
    manifest_path.write_text(json.dumps(wrong_probe))
    actual = invoke(binary, request)
    rows.append({"name": "wrong-probe-arch-with-valid-integrity", "native": actual, "equal": "error" in actual})
  finally:
    probe.write_bytes(original_probe)
    manifest_path.write_bytes(saved)
  actual = invoke(binary, request)
  rows.append({"name": "restored-complete-package", "native": actual, "equal": "result" in actual})
  return rows


def readiness(binary: Path, root: Path, package: Path, cache: Path) -> list[dict[str, Json]]:
  from openpilot.selfdrive.modeld import big_model, precompiled_model

  marker = next((cache / "precompiled").glob("*/installed.json"))
  installed = json.loads(marker.read_text())
  model = big_model.BigModelManifest(
    "owned", "model.pkl", installed["pickle"]["size"], installed["pickle"]["sha256"], urljoin(installed["catalog_url"], "model.pkl")
  )
  (cache / "state.json").write_text(json.dumps({"active": asdict(model), "previous": None}))
  original = precompiled_model.installed(model, cache)
  assert original is not None
  request = {"action": "ready", "cache": str(cache), "model": asdict(model), "value": {"models": str(root / "models"), "assets": str(package)}}
  manifest_path = package / "manifest.json"
  saved = manifest_path.read_bytes()
  manifest = json.loads(saved)
  rows = []
  try:
    for name in ("ready", "missing", "corrupt"):
      request["value"]["assets"] = str(root / "absent") if name == "missing" else str(package)
      if name == "corrupt":
        manifest["files"][0]["sha256"] = "0" * 64
        manifest_path.write_text(json.dumps(manifest))
      actual = invoke(binary, request)
      expected = {"compiled": name == "ready", "compile_pending": name != "ready", "path": str(original) if name == "ready" else None}
      rows.append({"name": name, "source_installed": str(original), "native": actual, "equal": actual.get("result") == expected})
  finally:
    manifest_path.write_bytes(saved)
  return rows


class Server(ThreadingHTTPServer):
  """Own the immutable response bytes and request observation accumulator."""

  payload: bytes
  records: list[dict[str, Json]]


class Handler(BaseHTTPRequestHandler):
  protocol_version = "HTTP/1.1"
  server: Server

  def do_GET(self) -> None:
    self.server.records.append({"path": self.path, "encoding": self.headers.get("Accept-Encoding"), "agent": self.headers.get("User-Agent")})
    self.send_response(200)
    self.send_header("Content-Length", str(len(self.server.payload)))
    self.end_headers()
    self.wfile.write(self.server.payload)

  def log_message(self, _format: str, *_args: Json) -> None:
    return


def delivery(binary: Path, root: Path) -> list[dict[str, Json]]:
  from openpilot.selfdrive.modeld import big_model, precompiled_model

  ca, cert, key = certificates(root)
  os.environ["SSL_CERT_FILE"] = str(ca)
  rows = []
  with Server(("127.0.0.1", 0), Handler) as server:
    server.payload, server.records = b"owned phase payload", []
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert, key)
    server.socket = context.wrap_socket(server.socket, server_side=True)
    thread = Thread(target=server.serve_forever)
    thread.start()
    try:
      model = big_model.BigModelManifest(
        "owned", "model.pkl", len(server.payload), sha256(server.payload).hexdigest(), f"https://localhost:{server.server_port}/model.pkl"
      )
      for name in ("download", "final", "complete-partial"):
        events = []
        for side in ("source", "native"):
          cache = root / name / side
          cache.mkdir(parents=True)
          target = cache / model.cache_filename
          if name != "download":
            target = target if name == "final" else target.with_suffix(target.suffix + ".part")
            target.write_bytes(server.payload)
        big_model._download_model(
          model,
          root / name / "source",
          progress_callback=lambda _m, done, total, events=events: events.append(["progress", done, total]),
          phase_callback=lambda phase, _m, events=events: events.append([phase]),
        )
        source_requests = list(server.records)
        server.records.clear()
        actual = invoke(binary, {"action": "observed", "cache": str(root / name / "native"), "model": asdict(model)})
        same = (
          actual.get("result") == {"events": events} and source_requests == server.records and files(root / name / "source") == files(root / name / "native")
        )
        rows.append(
          {
            "name": name,
            "original_events": events,
            "native": actual,
            "source_requests": source_requests,
            "native_requests": list(server.records),
            "equal": same,
          }
        )
        server.records.clear()
      server.payload = json.dumps(catalog(asdict(model))).encode()
      for side in ("source", "native"):
        cache = root / "catalog" / side
        marker = cache / "precompiled" / model.sha256 / "rejected"
        marker.parent.mkdir(parents=True)
        marker.write_text(model.sha256)
      original = precompiled_model.ensure_precompiled(model, root / "catalog" / "source")
      source_requests = list(server.records)
      server.records.clear()
      actual = invoke(binary, {"action": "install", "cache": str(root / "catalog" / "native"), "model": asdict(model), "value": str(root / "unused-assets")})
      rows.append(
        {
          "name": "catalog-identity-OS-trust",
          "source_requests": source_requests,
          "native_requests": list(server.records),
          "native": actual,
          "equal": original is None and actual.get("result") == {"installed": False} and source_requests == server.records,
        }
      )
    finally:
      server.shutdown()
      thread.join(2)
      assert not thread.is_alive()
  return rows


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--evidence", type=Path, required=True)
  parser.add_argument("--package", type=Path, required=True)
  parser.add_argument("--only", choices=("authority", "assets", "delivery", "readiness"), required=True)
  parser.add_argument("--cache", type=Path)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  match args.only:
    case "authority":
      rows = authorities(args.binary, args.evidence)
    case "assets":
      rows = assets(args.binary, args.evidence, args.package)
    case "delivery":
      rows = delivery(args.binary, args.evidence)
    case "readiness":
      assert args.cache is not None
      rows = readiness(args.binary, args.evidence, args.package, args.cache)
    case unexpected:
      assert_never(unexpected)
  result = {
    "rows": rows,
    "differences": sum(not row["equal"] for row in rows),
    "argv": [str(args.binary)],
    "binary_sha256": sha256(args.binary.read_bytes()).hexdigest(),
  }
  (args.evidence / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  print(json.dumps({"cases": len(rows), "differences": result["differences"]}))
  assert result["differences"] == 0


if __name__ == "__main__":
  main()
