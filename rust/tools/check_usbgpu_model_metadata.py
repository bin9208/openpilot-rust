"""Original manifest/state boundaries and atomic model selection on owned files."""

from __future__ import annotations

import argparse
from dataclasses import asdict
from copy import deepcopy
from hashlib import sha256
import json
import os
from pathlib import Path
import ssl
import subprocess
from threading import Thread
from typing import TYPE_CHECKING
from pytest import MonkeyPatch

from check_usbgpu_model_delivery import Handler, Server, certificates, files
from check_usbgpu_provision_contract import Json, Native

if TYPE_CHECKING:
  from openpilot.selfdrive.modeld.big_model import BigModelManifest


def native(args: argparse.Namespace, action: str, root: Path, model: dict[str, Json], value: Json = None) -> Native:
  request = {"action": action, "cache": str(root), "model": model, "ca": str(args.cert), "value": value}
  result = subprocess.run([str(args.binary)], input=json.dumps(request), text=True, capture_output=True, check=False, timeout=35)
  if result.returncode != 0:
    raise AssertionError({"exit": result.returncode, "stderr": result.stderr})
  return json.loads(result.stdout)


def selection(args: argparse.Namespace, server: Server) -> list[dict]:
  from openpilot.selfdrive.modeld import big_model

  cases = []
  for filename in (".pkl", ".onnx"):
    model = {"model_id": "owned", "filename": filename, "size": len(server.payload), "sha256": sha256(server.payload).hexdigest(), "url": args.url}
    source_root, native_root = args.evidence / filename[1:] / "source", args.evidence / filename[1:] / "native"
    for root in (source_root, native_root):
      root.mkdir(parents=True)
    manifest = big_model.BigModelManifest.from_dict(model)
    progress = []

    def observe(_model: BigModelManifest, done: int, total: int, progress: list = progress) -> None:
      progress.append([done, total])

    with MonkeyPatch.context() as monkey:
      monkey.setattr(big_model, "fetch_manifest", lambda _url, manifest=manifest: manifest)
      _, changed = big_model.ensure_big_model(cache_dir=source_root, progress_callback=observe)
    original = {"result": {"changed": changed}, "progress": progress}
    actual = native(args, "ensure", native_root, model)
    row = {"name": filename, "original": original, "native": actual, "source_files": files(source_root), "native_files": files(native_root)}
    row["equal"] = original == actual and row["source_files"] == row["native_files"]
    cases.append(row)
  return cases


def metadata(args: argparse.Namespace, model: dict) -> list[dict]:
  from openpilot.selfdrive.modeld import big_model

  values = [
    model,
    {**model, "url": "weights.pkl"},
    {**model, "filename": ".pkl"},
    {**model, "filename": ".onnx"},
    {**model, "url": ""},
    {**model, "size": True},
    {**model, "sha256": "A" * 64},
    {**model, "model_id": ""},
    {**model, "url": "http://models.test/a"},
    {key: value for key, value in model.items() if key != "filename"},
    [],
    None,
    list(model.values()),
  ]
  rows = []
  for index, value in enumerate(values):
    try:
      manifest = big_model.BigModelManifest.from_dict(value)
      original = {"manifest": asdict(manifest), "cache_filename": manifest.cache_filename}
    except (TypeError, ValueError):
      original = {"error": True}
    actual = native(args, "inspect", args.evidence / "unused", model, value)
    actual = {"error": True} if "error" in actual else actual["result"]
    rows.append({"kind": "manifest", "case": index, "source": original, "native": actual, "equal": original == actual})
  states = [
    {},
    {"active": None},
    {"active": model},
    {"active": {**model, "url": "weights.pkl"}},
    {"active": model, "previous": {}},
    {"active": {**model, "size": True}},
    {"previous": model},
    [],
    [model, None],
    {"active": list(model.values())},
  ]
  for index, value in enumerate(states):
    root = args.evidence / f"state-{index}"
    root.mkdir()
    (root / "state.json").write_text(json.dumps(value))
    original = {key: asdict(value) if value is not None else None for key, value in big_model.read_state(root).items()}
    actual = native(args, "state", root, model)["result"]["state"]
    rows.append({"kind": "state", "case": index, "source": original, "native": actual, "equal": original == actual})
  return rows


def catalogs(args: argparse.Namespace, model: dict) -> list[dict]:
  from openpilot.selfdrive.modeld.precompiled_model import validate_catalog

  base = {
    "protocol": 1,
    "format": "comma-generic-onnx",
    "model_sha256": model["sha256"],
    "gpu_arch": "gfx1200",
    "frame_skip": 4,
    "camera_resolutions": [[1928, 1208], [1344, 760]],
    "pickle": {"url": "model.pkl", "sha256": model["sha256"], "size": 1},
    "runtime": {"url": "runtime.tar.gz", "sha256": "b" * 64, "size": 1},
  }
  rows = []
  for name, host, artifact, float_camera in [
    ("ordinary", "models.test", None, False),
    ("float-camera", "models.test", None, True),
    ("host-case", "models.test", "https://MODELS.test/model.pkl", False),
    ("explicit443", "models.test", "https://models.test:443/model.pkl", False),
    ("base443", "models.test:443", None, False),
    ("base-case", "MODELS.test", None, False),
  ]:
    value = deepcopy(base)
    if artifact is not None:
      value["pickle"]["url"] = artifact
    if float_camera:
      value["camera_resolutions"] = [[1928.0, 1208.0], [1344.0, 760.0]]
    url = f"https://{host}/precompiled.json"
    try:
      original = {"catalog": validate_catalog(deepcopy(value), model["sha256"], url)}
    except ValueError:
      original = {"error": True}
    actual = native(args, "catalog", args.evidence / "unused", model, {"catalog": value, "url": url})
    actual = {"error": True} if "error" in actual else actual["result"]
    rows.append({"kind": "catalog", "case": name, "source": original, "native": actual, "equal": original == actual})
  return rows


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--evidence", type=Path, required=True)
  parser.add_argument("--expect-dotfile-diff", action="store_true")
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  args.cert, cert, key = certificates(args.evidence)
  os.environ["SSL_CERT_FILE"] = str(args.cert)
  server = Server(("127.0.0.1", 0), Handler)
  server.mode, server.payload, server.records = "normal", b"owned metadata model", []
  context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
  context.load_cert_chain(cert, key)
  server.socket = context.wrap_socket(server.socket, server_side=True)
  args.url = f"https://localhost:{server.server_port}/weights.pkl"
  thread = Thread(target=server.serve_forever)
  thread.start()
  try:
    cases = selection(args, server)
    if not args.expect_dotfile_diff:
      model = {"model_id": "owned", "filename": "weights.pkl", "size": 1, "sha256": "a" * 64, "url": big_model_default_url()}
      cases.extend(metadata(args, model))
      cases.extend(catalogs(args, model))
    failed = [row for row in cases if not row["equal"]]
    summary = {"cases": len(cases), "differences": len(failed), "expected_dotfile_red": args.expect_dotfile_diff, "rows": cases}
    (args.evidence / "result.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({key: value for key, value in summary.items() if key != "rows"}))
    assert (len(failed) == 2) if args.expect_dotfile_diff else not failed
  finally:
    server.shutdown()
    server.server_close()
    thread.join(2)
    assert not thread.is_alive()


def big_model_default_url() -> str:
  from openpilot.selfdrive.modeld.big_model import DEFAULT_MANIFEST_URL

  return DEFAULT_MANIFEST_URL


if __name__ == "__main__":
  main()
