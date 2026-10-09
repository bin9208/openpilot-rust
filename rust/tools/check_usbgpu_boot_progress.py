"""A real status-write failure aborts original/native precompiled installation."""

from __future__ import annotations

import argparse
from dataclasses import asdict
from hashlib import sha256
import json
import os
from pathlib import Path
import ssl
import subprocess
from threading import Thread

from check_usbgpu_model_delivery import certificates, files
from check_usbgpu_provision_contract import Handler, Json, Server, catalog


class Payload(Handler):
  def do_GET(self) -> None:
    self.server.records.append({"path": self.path, "encoding": self.headers.get("Accept-Encoding"), "agent": self.headers.get("User-Agent")})
    body = self.server.manifest if self.path == "/precompiled.json" else self.server.payload
    self.send_response(200)
    self.send_header("Content-Length", str(len(body)))
    self.end_headers()
    self.wfile.write(body)


class Owned(Server):
  manifest: bytes


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--evidence", type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  from openpilot.selfdrive.modeld import big_model, big_model_status, precompiled_model

  ca, cert, key = certificates(args.evidence)
  os.environ["SSL_CERT_FILE"] = str(ca)
  with Owned(("127.0.0.1", 0), Payload) as server:
    server.payload, server.records = b"a" * (2 * 1024**2), []
    model = big_model.BigModelManifest(
      "owned", "model.pkl", len(server.payload), sha256(server.payload).hexdigest(), f"https://localhost:{server.server_port}/model.pkl"
    )
    server.manifest = json.dumps(catalog(asdict(model))).encode()
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert, key)
    server.socket = context.wrap_socket(server.socket, server_side=True)
    thread = Thread(target=server.serve_forever)
    thread.start()
    try:
      source, native = args.evidence / "source", args.evidence / "native"
      for root in (source, native):
        (root / "status.json").mkdir(parents=True)
      progress: list[list[int]] = []

      def update(done: int, total: int) -> None:
        progress.append([done, total])
        big_model_status.write_big_model_status(source, "downloading", downloaded_bytes=done, total_bytes=total, detail="precompiled model")

      try:
        precompiled_model.ensure_precompiled(model, source, progress=update)
      except IsADirectoryError as error:
        original_error = str(error)
      else:
        raise AssertionError("owned status directory did not interrupt the source callback")
      requests = list(server.records)
      server.records.clear()
      request = {"model": asdict(model), "cache": str(native), "assets": str(args.evidence / "unused-assets")}
      argv = [str(args.binary)]
      actual = subprocess.run(argv, input=json.dumps(request), text=True, capture_output=True, check=False, timeout=30)
      response = json.loads(actual.stdout)
      expected_files, native_files = files(source), files(native)
      result: dict[str, Json] = {
        "argv": argv,
        "request": request,
        "source_error": original_error,
        "native": response,
        "source_progress": progress,
        "source_requests": requests,
        "native_requests": server.records,
        "source_files": expected_files,
        "native_files": native_files,
        "exit": actual.returncode,
        "stderr": actual.stderr,
      }
      (args.evidence / "result.json").write_text(json.dumps(result, indent=2) + "\n")
      assert actual.returncode == 0 and "error" in response
      assert response["progress"] == progress == [[1024**2, len(server.payload)]]
      assert requests == server.records
      assert expected_files == native_files
      assert len(expected_files) == 1 and next(iter(expected_files)).endswith("model.pkl.part")
      print(json.dumps({"cases": 1, "differences": 0, "partial_bytes": 1024**2}))
    finally:
      server.shutdown()
      thread.join(2)
      assert not thread.is_alive()


if __name__ == "__main__":
  main()
