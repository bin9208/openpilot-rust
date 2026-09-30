#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["pyzmq==27.2.0", "sentry-sdk==2.55.0"]
# ///
# How to run: uv run --no-project --python 3.12 rust/tools/check_crash_sdk_transport.py PROBE PARAMS_BINDING OUTPUT [RUNNER ARGS...]
"""Compare project-owned Sentry fields at real native HTTP and pinned Python SDK capture boundaries."""
from __future__ import annotations

import copy
import gzip
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
import select
from pathlib import Path
from queue import Queue
import subprocess
import sys
from threading import Thread

import sentry_sdk
from sentry_sdk.transport import Transport

from check_tombstoned_reference import ROOT, pair


class Receiver:
  def __init__(self):
    self.events = Queue()
    owner = self
    class Handler(BaseHTTPRequestHandler):
      def do_POST(self):
        raw = self.rfile.read(int(self.headers["Content-Length"]))
        body = gzip.decompress(raw) if self.headers.get("Content-Encoding") == "gzip" else raw
        lines = body.split(b"\n", 2)
        header, item = json.loads(lines[0]), json.loads(lines[1])
        payload = lines[2][:item["length"]]
        assert item["type"] == "event" and self.path == "/api/1/envelope/"
        owner.events.put({"event": json.loads(payload), "raw": body.decode(), "path": self.path, "header": header, "auth": self.headers.get("X-Sentry-Auth")})
        response = b"{}"
        self.send_response(200)
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)
      def log_message(self, *args): return
    self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    self.thread = Thread(target=self.server.serve_forever)
    self.thread.start()
    self.dsn = f"http://fixture@127.0.0.1:{self.server.server_port}/1"

  def close(self):
    self.server.shutdown()
    self.thread.join(timeout=5)
    self.server.server_close()


class PythonSdk:
  def __init__(self):
    self.events = []
    owner = self
    class Capture(Transport):
      def capture_envelope(self, envelope):
        for item in envelope.items:
          if item.headers.get("type") == "event": owner.events.append(copy.deepcopy(item.payload.json))
    self.transport = Capture

  def replay(self, calls):
    for call in calls:
      fields = call["fields"]
      match call["op"]:
        case "init":
          sentry_sdk.init("http://fixture@127.0.0.1:9/1", transport=self.transport,
                          default_integrations=fields["default_integrations"], integrations=[],
                          release=fields["release"], environment=fields["environment"],
                          traces_sample_rate=fields["traces_sample_rate"], max_value_length=fields["max_value_length"])
        case "set_user": sentry_sdk.set_user(fields)
        case "set_tag": sentry_sdk.set_tag(fields["key"], json.loads(fields["value_json"]))
        case "set_extra": sentry_sdk.set_extra(fields["key"], json.loads(fields["value_json"]))
        case "capture_message": sentry_sdk.capture_message(fields["message"])
        case "flush": sentry_sdk.flush()
        case _: raise AssertionError(call)


def source_git_fixture(path):
  environment = {**os.environ, "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1", "GIT_AUTHOR_DATE": "2020-01-02T03:04:05+00:00", "GIT_COMMITTER_DATE": "2020-01-02T03:04:05+00:00"}
  def git(*arguments): subprocess.run(["git", *arguments], cwd=path, env=environment, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
  (path / "build.json").unlink()
  (path / "RELEASES.md").write_text("fixture notes\n\nolder notes\n")
  git("init", "-b", "main")
  git("config", "user.name", "Fixture")
  git("config", "user.email", "fixture@example.invalid")
  git("config", "core.hooksPath", "/dev/null")
  git("config", "commit.gpgsign", "false")
  git("add", ".")
  git("commit", "-m", "fixture")
  git("remote", "add", "origin", "git@github.com:commaai/openpilot.git")


def main():
  binary, binding, output = map(lambda value: Path(value).resolve(), sys.argv[1:4])
  output.mkdir(parents=True, exist_ok=True)
  command = [*sys.argv[4:], str(binary)]
  receiver = Receiver()
  original = PythonSdk()
  comparisons = []
  try:
    with pair(command, binding, output / "policy") as p:
      p.write("base/build.json", json.dumps({"channel": ["nightly", True], "openpilot": {"git_origin": "github.com/commaai/openpilot", "git_commit": {"opaque": 7, "nested": [None, False]}, "version": "ignored-in-release"}}))
      p.call("configure", base="$FIXTURE/base", params="$FIXTURE/params", pc=False, device="tici", dsn=receiver.dsn)
      def call(op, **fields):
        result = p.call(op, **fields)
        original.replay(p.rows[-1]["expected"]["calls"])
        return result
      call("init", project="selfdrive_native")
      values = [True, None, 10**100, float("nan"), float("inf"), [True, 17.5, None], {"key": "value", "list": list(range(20))}, "A"*8192, "A"*8193, "한"*2731, "A"*8188+"😀"+"tail", "\ud800", "\ud800"+"X"*8192]
      for index, value in enumerate(values):
        call("tag", key="unusual", value=json.dumps(value))
        contents = "한"*3000 if index % 2 else "contents\n" + "B"*9000
        call("tombstone", filename="fixture.crash", message="message-"+str(index), contents=contents)
        expected = original.events.pop(0)
        capture = receiver.events.get(timeout=10)
        actual = capture["event"]
        keys = ["message", "tags", "user", "extra", "release", "environment", "_meta"]
        assert {key: actual.get(key) for key in keys} == {key: expected.get(key) for key in keys}, (index, actual, expected)
        assert type(actual["tags"]["dirty"]) is bool
        assert actual["platform"] == "native" and expected["platform"] == "python"
        assert actual["sdk"]["name"] == "sentry.rust" and expected["sdk"]["name"] == "sentry.python"
        assert "sentry_key=fixture" in capture["auth"]
        (output / f"envelope-{index}.txt").write_text(capture["raw"])
        comparisons.append({"case": index, "expected": expected, "actual": actual, "passed": True})
      # A source-tree metadata path proves the dirty tag remains a JSON boolean true.
      for path in p.paths("base"): source_git_fixture(path)
      call("init", project="selfdrive_native")
      call("tombstone", filename="dirty.crash", message="dirty-metadata", contents="short")
      expected = original.events.pop(0)
      capture = receiver.events.get(timeout=10)
      actual = capture["event"]
      assert actual["tags"]["dirty"] is True and expected["tags"]["dirty"] is True
      assert actual["tags"] == expected["tags"]
      (output / "envelope-dirty.txt").write_text(capture["raw"])
      comparisons.append({"case": "dirty-metadata", "expected": expected, "actual": actual, "passed": True})
      process = p.processes[1]
      request = {"op": "capture_io", "path": str(p.root / "native/actual-missing-file")}
      process.stdin.write(json.dumps(request) + "\n")
      process.stdin.flush()
      assert select.select([process.stdout], [], [], 10)[0]
      response = json.loads(process.stdout.readline())
      assert response["result"] == {"value": None}
      logs = []
      while True:
        packet = p.socket.recv()
        record = json.loads(packet[1:])
        if record["msg"] == "barrier-" + str(p.index): break
        logs.append(record)
      capture = receiver.events.get(timeout=10)
      native_error = capture["event"]
      (output / "native-error.json").write_text(json.dumps({"request": request, "response": response, "event": native_error, "logs": logs}, indent=2) + "\n")
      (output / "envelope-native-error.txt").write_text(capture["raw"])
      error = native_error["exception"]["values"][-1]
      assert error["type"].startswith("std::io::") and "os error 2" in error["value"], error
      assert "rust_backtrace" in native_error["extra"] and "tombstone" in native_error["extra"]
      assert [record["msg"] for record in logs] == ["crash"]
      assert (p.root / "native/params/fixture/CarrotException").read_text() == "exception"

    (output / "comparisons.json").write_text(json.dumps(comparisons, indent=2) + "\n")
    manifest = {"result": "PASS", "events": len(comparisons), "native_io_exception_events": 1, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "transport": "native sentry-rust0.49.3 HTTP to loopback; Python sentry-sdk2.55.0 in-memory capture", "preserved": ["tag JSON types", "original user/release/environment", "extra values", "max_value_length byte/codepoint fallback", "_meta len/rem annotations"], "excluded_sdk_internals": ["SDK identity/platform", "thread integration", "native exception representation", "retry/rate-limit/grouping internals"], "command": command}
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest, indent=2))
  finally:
    receiver.close()


if __name__ == "__main__": main()
