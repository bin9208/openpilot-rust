# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run: python rust/tools/check_carrot_recovery_upload.py --binary <upload example> --output <owned directory>
from __future__ import annotations

import argparse
from email import policy
from email.parser import BytesParser
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import threading
from types import ModuleType
from typing import Final, TypeAlias

import carrot_recovery_actions_fixtures as fixture

Json: TypeAlias = None | bool | int | float | str | list["Json"] | dict[str, "Json"]


class Receiver:
  def __init__(self) -> None:
    self.mode = "success"
    self.requests: list[Json] = []


def normalize(body: bytes, content_type: str) -> Json:
  if content_type.startswith("application/json"):
    return json.loads(body)
  message = BytesParser(policy=policy.default).parsebytes(f"Content-Type: {content_type}\r\n\r\n".encode() + body)
  parts: list[Json] = []
  for part in message.iter_parts():
    raw = part.get_payload(decode=True)
    name = part.get_param("name", header="Content-Disposition")
    filename = part.get_filename()
    if filename:
      parts.append({"name": name, "filename": re.sub(r"\d{8}-\d{6}", "TIMESTAMP", filename), "size": len(raw), "sha256": hashlib.sha256(raw).hexdigest()})
    elif name == "payload_json":
      payload = json.loads(raw)
      payload["content"] = re.sub(r"- Time: \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}", "- Time: TIMESTAMP", payload["content"])
      parts.append({"name": name, "json": payload})
    else:
      parts.append({"name": name, "text": raw.decode()})
  return parts


def handler(receiver: Receiver) -> type[BaseHTTPRequestHandler]:
  class Handler(BaseHTTPRequestHandler):
    def do_POST(self) -> None:
      body = self.rfile.read(int(self.headers["Content-Length"]))
      receiver.requests.append(
        {
          "path": self.path,
          "authorization": self.headers.get("Authorization"),
          "agent": self.headers.get("User-Agent"),
          "accept": self.headers.get("Accept"),
          "body": normalize(body, self.headers["Content-Type"]),
        }
      )
      status, response = 200, b'{"ok":true,"token":"owned-token"}'
      if receiver.mode == "session-denied" and self.path.endswith("/session"):
        status, response = 403, b"session denied"
      if receiver.mode == "missing-token" and self.path.endswith("/session"):
        response = b'{"ok":true}'
      if receiver.mode == "non-json":
        response = b"accepted"
      if receiver.mode == "partial" and self.path == "/logs":
        status, response = 503, b"logs unavailable"
      if receiver.mode == "all-failed":
        status, response = 500, "\uac70\ubd80".encode() * 600
      if receiver.mode == "empty-error":
        status, response = 502, b""
      if receiver.mode in {"redirect301", "redirect307"}:
        status, response = (301 if receiver.mode == "redirect301" else 307), b""
      if receiver.mode == "large-response":
        response = b'{"ok":true,"token":"' + b'x' * (1024 * 1024) + b'"}'
      if receiver.mode == "invalid-utf8-response":
        response = b'\xff'
      self.send_response(status)
      if status in {301, 307}:
        self.send_header("Location", "/redirect" + self.path)
      self.send_header("Content-Length", str(len(response)))
      self.end_headers()
      self.wfile.write(response)

    def do_GET(self) -> None:
      receiver.requests.append(
        {
          "path": self.path,
          "authorization": self.headers.get("Authorization"),
          "agent": self.headers.get("User-Agent"),
          "accept": self.headers.get("Accept"),
          "body": None,
        }
      )
      response = b'{"ok":true,"token":"owned-token"}'
      self.send_response(200)
      self.send_header("Content-Length", str(len(response)))
      self.end_headers()
      self.wfile.write(response)

    def log_message(self, fmt: str, *args: str | int) -> None:
      return

  return Handler


SCENARIOS: Final = (
  "success",
  "token",
  "session-denied",
  "missing-token",
  "non-json",
  "partial",
  "all-failed",
  "empty-error",
  "disabled",
  "invalid-urls",
  "large",
  "missing-file",
  "redirect301",
  "redirect307",
  "large-response",
  "invalid-utf8-response",
)


def http_actions(binary: Path, root: Path, module: ModuleType, receiver: Receiver) -> list[Json]:
  tools = root / "tools"
  tools.mkdir()
  tmux = tools / "tmux"
  tmux.write_text("#!/bin/sh\nprintf 'owned capture\\377\\r\\n'\n")
  tmux.chmod(0o755)
  (root / "fixture-params").symlink_to(root / "params", target_is_directory=True)
  evidence = root / "http"
  evidence.mkdir()
  results: list[Json] = []
  with fixture.environment({"PATH": str(tools) + os.pathsep + os.environ["PATH"]}):
    with fixture.native(binary, root, root / "tmux.log", evidence) as port:
      for mode in ("success", "partial", "all-failed"):
        receiver.mode = mode
        receiver.requests.clear()
        expected = module._tool_action("server_tmux_log", {})
        source_requests = list(receiver.requests)
        source_capture = (root / "tmux.log").read_bytes()
        receiver.requests.clear()
        actual = fixture.request(port, {"action": "server_tmux_log"})
        row = {"scenario": mode, "source": expected, "native": actual, "source_requests": source_requests, "native_requests": list(receiver.requests)}
        results.append(row)
        (evidence / "comparisons.json").write_text(json.dumps(results, indent=2) + "\n")
        assert actual == expected, row
        assert receiver.requests == source_requests, row
        assert (root / "tmux.log").read_bytes() == source_capture == b"owned capture\xef\xbf\xbd\n"
  return results


def run(binary: Path, root: Path, http_binary: Path | None) -> None:
  root.mkdir(parents=True, exist_ok=False)
  params = root / "params" / "d"
  params.mkdir(parents=True)
  for key, value in {
    "CarName": "owned \ucc28\ub7c9",
    "GitBranch": "feature/owned",
    "GithubUsername": "owned",
    "GitRemote": "git@github.com:owned/repo.git",
    "GitCommit": "0123456789abcdef",
    "DongleId": "owned-device",
    "HardwareSerial": "owned-serial",
  }.items():
    (params / key).write_text(value)
  tmux_log = root / "tmux.log"
  receiver = Receiver()
  with ThreadingHTTPServer(("127.0.0.1", 0), handler(receiver)) as server:
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    base = f"http://127.0.0.1:{server.server_address[1]}"
    previous_env = os.environ.copy()
    try:
      source = Path("openpilot/selfdrive/carrot/recovery/server.py").resolve()
      spec = importlib.util.spec_from_file_location("recovery_upload_oracle", source)
      if spec is None or spec.loader is None:
        raise RuntimeError(source)
      module = importlib.util.module_from_spec(spec)
      spec.loader.exec_module(module)
      module.PARAMS_DIR = str(root / "params")
      module.TMUX_LOG_PATH = str(tmux_log)
      results: list[Json] = []
      for mode in SCENARIOS:
        receiver.mode = mode
        tmux_log.write_bytes(b"owned-log\x00\xff\n" if mode != "large" else bytes(range(256)) * 33000)
        if mode == "missing-file":
          tmux_log.unlink()
        os.environ.update(
          {
            "CARROT_WEB_UPLOAD_URL": base,
            "CARROT_TMUX_WEB_UPLOAD_URL": base + "/logs",
            "CARROT_EXCEPTION_DISCORD_WEBHOOK_URL": base + "/discord",
            "CARROT_EXCEPTION_DISCORD_WEBHOOK_DISABLE": "1" if mode == "disabled" else "0",
            "CARROT_WEB_UPLOAD_TOKEN": "owned-token" if mode == "token" else "",
            "CARROT_DATA_DIR": str(root / "data"),
          }
        )
        if mode == "invalid-urls":
          os.environ.update({"CARROT_WEB_UPLOAD_URL": "invalid", "CARROT_TMUX_WEB_UPLOAD_URL": "invalid", "CARROT_EXCEPTION_DISCORD_WEBHOOK_URL": "invalid"})
        receiver.requests.clear()
        expected = module._send_tmux_destinations("owned-recovery")
        source_requests = list(receiver.requests)
        receiver.requests.clear()
        process = subprocess.run([str(binary), str(root), "owned-recovery"], check=True, capture_output=True, timeout=45)
        actual = json.loads(process.stdout)
        row = {"scenario": mode, "source": expected, "native": actual, "source_requests": source_requests, "native_requests": list(receiver.requests)}
        results.append(row)
        (root / "comparisons.json").write_text(json.dumps(results, indent=2) + "\n")
        assert actual == expected, row
        assert receiver.requests == source_requests, row
      http_results = http_actions(http_binary, root, module, receiver) if http_binary else []
      (root / "result.json").write_text(
        json.dumps(
          {
            "pairs": len(results),
            "http_action_pairs": len(http_results),
            "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "owned_recipients_only": True,
          }
        )
        + "\n"
      )
      print(f"PASS: {len(results)} upload result and wire comparisons, {len(http_results)} HTTP action comparisons")
    finally:
      os.environ.clear()
      os.environ.update(previous_env)
      server.shutdown()
      thread.join()


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description="Compare recovery uploads using owned HTTP recipients")
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--http-binary", type=Path)
  args = parser.parse_args()
  run(args.binary.resolve(), args.output.resolve(), args.http_binary.resolve() if args.http_binary else None)
