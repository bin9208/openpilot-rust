"""Compare original download/state behavior on an owned, trusted loopback TLS server."""

from __future__ import annotations

import argparse
from hashlib import sha256
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import ssl
import subprocess
from threading import Thread
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
  from openpilot.selfdrive.modeld.big_model import BigModelManifest


class Server(ThreadingHTTPServer):
  daemon_threads = True
  payload: bytes
  mode: str
  records: list[dict[str, str | None]]


class Handler(BaseHTTPRequestHandler):
  protocol_version = "HTTP/1.1"
  server: Server

  def do_GET(self) -> None:
    server = self.server
    received = self.headers.get("Range")
    server.records.append({"range": received, "agent": self.headers.get("User-Agent"), "encoding": self.headers.get("Accept-Encoding")})
    payload, status = server.payload, 200
    if received and server.mode != "ignore":
      offset = int(received.removeprefix("bytes=").removesuffix("-"))
      payload, status = payload[offset:], 206
    self.send_response(status)
    if status == 206:
      offset = 0 if server.mode == "bad-range" else int(str(received).split("=")[1].split("-")[0])
      self.send_header("Content-Range", f"bytes {offset}-{len(server.payload) - 1}/{len(server.payload)}")
    self.send_header("Content-Length", str(len(payload)))
    self.end_headers()
    self.wfile.write(payload)

  def log_message(self, *_args: str | int | bytes) -> None:
    return


def files(root: Path) -> dict[str, Any]:
  result: dict[str, Any] = {}
  for path in sorted(root.rglob("*")):
    if path.is_file():
      data = path.read_bytes()
      result[str(path.relative_to(root))] = json.loads(data) if path.name == "state.json" else {"size": len(data), "sha256": sha256(data).hexdigest()}
  return result


def certificates(root: Path) -> tuple[Path, Path, Path]:
  ca, ca_key = root / "ca.pem", root / "ca-key.pem"
  cert, key, csr, extensions = root / "server.pem", root / "key.pem", root / "server.csr", root / "extensions"
  extensions.write_text("basicConstraints=CA:FALSE\nsubjectAltName=DNS:localhost\n")
  commands = [
    ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", str(ca_key), "-out", str(ca), "-days", "1", "-subj", "/CN=Owned Model CA"],
    ["openssl", "req", "-newkey", "rsa:2048", "-nodes", "-keyout", str(key), "-out", str(csr), "-subj", "/CN=localhost"],
    [
      "openssl",
      "x509",
      "-req",
      "-in",
      str(csr),
      "-CA",
      str(ca),
      "-CAkey",
      str(ca_key),
      "-CAcreateserial",
      "-out",
      str(cert),
      "-days",
      "1",
      "-extfile",
      str(extensions),
    ],
  ]
  for argv in commands:
    subprocess.run(argv, check=True, capture_output=True)
  return ca, cert, key


def source(action: str, model: dict[str, Any], root: Path) -> dict[str, Any]:
  from openpilot.selfdrive.modeld import big_model, precompiled_model

  progress = []

  def observe(*values: BigModelManifest | int) -> None:
    progress.append(list(values[-2:]))

  try:
    manifest = big_model.BigModelManifest.from_dict(model)
    if action == "ensure":
      _, changed = big_model.ensure_big_model(model["manifest_url"], root, progress_callback=observe)
      outcome = {"changed": changed}
    elif action == "precompiled":
      precompiled_model.download({"url": manifest.url, "size": manifest.size, "sha256": manifest.sha256}, root / manifest.cache_filename, observe)
      outcome = {"ok": True}
    else:
      big_model._download_model(manifest, root, progress_callback=observe)
      outcome = {"ok": True}
    return {"result": outcome, "progress": progress}
  except (OSError, ValueError) as error:
    return {"error": str(error), "progress": progress}


def compare_case(name: str, action: str, initial: bytes | None, mode: str, payload: bytes, server: Server, args: argparse.Namespace) -> dict[str, Any]:
  server.mode, server.payload = mode, payload
  expected = b"owned model payload"
  model = {"model_id": "owned", "filename": "artifact.pkl", "size": len(expected), "sha256": sha256(expected).hexdigest(), "url": args.url}
  destination = f"artifact-{model['sha256'][:16]}.pkl"
  source_root, native_root = args.evidence / name / "source", args.evidence / name / "native"
  for root in (source_root, native_root):
    root.mkdir(parents=True)
    if initial is not None:
      (root / (destination + ".part")).write_bytes(initial)
  server.records = []
  original = source(action, model, source_root)
  source_records = server.records
  server.records = []
  request = {"action": action, "model": model, "cache": str(native_root), "ca": str(args.cert)}
  process = subprocess.run([str(args.binary)], input=json.dumps(request), text=True, capture_output=True, timeout=35, check=False)
  native = json.loads(process.stdout)
  equivalent = (
    process.returncode == 0
    and ("error" in original) == ("error" in native)
    and original.get("result") == native.get("result")
    and original["progress"] == native["progress"]
    and files(source_root) == files(native_root)
    and source_records == server.records
  )
  value = {
    "name": name,
    "equivalent": equivalent,
    "original": original,
    "native": native,
    "source_files": files(source_root),
    "native_files": files(native_root),
    "source_requests": source_records,
    "native_requests": server.records,
    "native_exit": process.returncode,
    "native_stderr": process.stderr,
  }
  (args.evidence / name / "result.json").write_text(json.dumps(value, indent=2) + "\n")
  return value


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--evidence", type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  args.cert, server_cert, key = certificates(args.evidence)
  os.environ["SSL_CERT_FILE"] = str(args.cert)
  server = Server(("127.0.0.1", 0), Handler)
  context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
  context.load_cert_chain(server_cert, key)
  server.socket = context.wrap_socket(server.socket, server_side=True)
  args.url = f"https://localhost:{server.server_port}/artifact.pkl"
  thread = Thread(target=server.serve_forever)
  thread.start()
  expected = b"owned model payload"
  cases = [
    ("full", None, "normal", expected),
    ("resume", expected[:5], "normal", expected),
    ("restart", expected[:5], "ignore", expected),
    ("range-error", expected[:5], "bad-range", expected),
    ("short", None, "normal", expected[:-1]),
    ("hash-error", None, "normal", b"x" * len(expected)),
    ("complete-partial", expected, "normal", expected),
    ("oversized-partial", expected + b"x", "normal", expected),
    ("bad-complete", b"x" * len(expected), "normal", expected),
  ]
  try:
    results = [
      compare_case(f"{action}-{name}", action, initial, mode, payload, server, args)
      for action in ("download", "precompiled")
      for name, initial, mode, payload in cases
    ]
    summary = {
      "cases": len(results),
      "differences": [case["name"] for case in results if not case["equivalent"]],
      "binary_sha256": sha256(args.binary.read_bytes()).hexdigest(),
      "scope": "Actual HTTPS payload/resume/verification/files/progress parity; diagnostic error wording excluded",
    }
    (args.evidence / "result.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary))
    if summary["differences"]:
      raise AssertionError(summary["differences"])
  finally:
    server.shutdown()
    server.server_close()
    thread.join(2)
    if thread.is_alive():
      raise AssertionError("owned TLS server did not stop")


if __name__ == "__main__":
  main()
