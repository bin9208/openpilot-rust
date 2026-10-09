from __future__ import annotations

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path


class Server(ThreadingHTTPServer):
  daemon_threads = True
  manifest: dict
  payload: bytes
  mode: str
  cache: Path
  records: list[dict]


class Handler(BaseHTTPRequestHandler):
  protocol_version = "HTTP/1.1"
  server: Server

  def do_GET(self) -> None:
    server = self.server
    server.records.append({"path": self.path, "range": self.headers.get("Range"), "agent": self.headers.get("User-Agent")})
    manifest = self.path == "/manifest.json"
    payload = (b"invalid" if server.mode == "invalid" else json.dumps(server.manifest).encode()) if manifest else server.payload
    if server.mode == "status-io" and not manifest:
      (server.cache / "status.json").unlink()
      (server.cache / "status.json").mkdir()
    self.send_response(200)
    if server.mode == "chunked" and not manifest:
      self.send_header("Transfer-Encoding", "chunked")
    else:
      self.send_header("Content-Length", str(len(payload)))
    self.end_headers()
    if server.mode == "chunked" and not manifest:
      self.wfile.write(b"3\r\nown\r\n")
      self.close_connection = True
    elif server.mode == "short" and not manifest:
      self.wfile.write(payload[:3])
      self.close_connection = True
    else:
      self.wfile.write(payload)

  def log_message(self, *_args: str | int | bytes) -> None:
    return None
