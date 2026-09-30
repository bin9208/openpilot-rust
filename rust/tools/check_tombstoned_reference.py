#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["pyzmq==27.2.0", "sentry-sdk==2.55.0"]
# ///
# How to run: uv run --no-project --python 3.12 rust/tools/check_tombstoned_reference.py PROBE PARAMS_BINDING OUTPUT [RUNNER ARGS...]
"""Original-source comparisons using real temporary crash files, Params and shell processes."""
from __future__ import annotations

from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import random
import select
import signal
import subprocess
import sys
import tempfile
import time
import uuid

import zmq

ROOT = Path(__file__).resolve().parents[2]
STAMP = "2020-01-02--03-04-05"
TRACE = "header\n#0 0xdead system (p=0x1) at sys.c\n#1 0xbeef model (a=1) at selfdrive/modeld.cc:7\n"
BODY = "ExecutablePath: /data/openpilot/selfdrive/modeld/modeld\nSignal: 11\nProcMaps:\n omitted\nProcStatus:\n kept\nCoreDump: omitted\n"


def normalize(value, root):
  match value:
    case str(): return value.replace(str(root / "source"), "$FIXTURE").replace(str(root / "native"), "$FIXTURE")
    case list(): return [normalize(item, root) for item in value]
    case dict():
      result = {key: normalize(item, root) for key, item in value.items() if key != "created"}
      if "value_json" in result:
        result["value_json"] = normalize(json.loads(result["value_json"]), root)
      return result
    case _: return value


def snapshot(root):
  result = {}
  for folder in ["apport", "logs"]:
    if not (root / folder).exists(): continue
    for path in sorted((root / folder).rglob("*")):
      name = str(path.relative_to(root))
      mode = path.stat().st_mode & 0o7777
      if path.is_dir(): result[name] = {"directory": True, "mode": mode}
      elif path.is_symlink(): result[name] = {"symlink": path.readlink().name, "mode": mode}
      else:
        digest = hashlib.sha256()
        with path.open("rb") as stream:
          for data in iter(lambda: stream.read(1024 * 1024), b""): digest.update(data)
        result[name] = {"size": path.stat().st_size, "sha256": digest.hexdigest(), "mode": mode}
  params = root / "params/fixture/CarrotException"
  result["CarrotException"] = params.read_bytes().hex() if params.is_file() else None
  return result


class Pair:
  def __init__(self, binary, binding, output, temporary):
    self.root = Path(temporary)
    self.output = output
    self.index = 0
    (output / "comparisons.jsonl").unlink(missing_ok=True)
    self.rows = []
    self.context = zmq.Context()
    self.socket = self.context.socket(zmq.PULL)
    self.socket.setsockopt(zmq.RCVTIMEO, 40000)
    self.prefix = "tombstone-" + uuid.uuid4().hex
    self.endpoint = Path("/tmp/logmessage" + self.prefix)
    self.socket.bind("ipc://" + str(self.endpoint))
    self.streams = []
    self.processes = []
    self.binary = binary
    self.binding = binding
    for kind in ["source", "native"]:
      directory = self.root / kind
      for name in ["apport", "logs", "base/openpilot/common", "params/fixture", "bin"]:
        (directory / name).mkdir(parents=True, exist_ok=True)
      (directory / "base/openpilot/common/version.h").write_text('#define VERSION "fixture-1.2"\n')
      (directory / "base/build.json").write_text(json.dumps({"channel": "nightly", "openpilot": {"git_origin": "git@github.com:commaai/openpilot.git", "git_commit": "abcdefghijk", "version": "metadata-version"}}))
      (directory / "params/fixture/DongleId").write_text("fixture-device")
      (directory / "trace").write_text(TRACE)
      (directory / "mode").write_text("normal")
      helper = directory / "bin/apport-retrace"
      helper.write_text('''#!/bin/bash
[[ "$1" == -s && "$#" == 2 ]] || exit 51
head -c 4096 "$2" > "$FIXTURE_INPUT"
case "$(cat "$FIXTURE_MODE")" in
normal) cat "$FIXTURE_TRACE";;
exit) printf 'failed\\r\\n'; exit 7;;
invalid) printf '\\377';;
invalid-exit) printf '\\377'; exit 8;;
timeout) echo $$ > "$FIXTURE_PID"; exec sleep 45;;
flood) while true; do printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'; done;;
esac
''')
      helper.chmod(0o755)
      env = {**os.environ, "OPENPILOT_PREFIX": self.prefix, "PATH": str(directory / "bin") + ":/usr/bin:/bin", "FIXTURE_INPUT": str(directory / "retrace.input"), "FIXTURE_MODE": str(directory / "mode"), "FIXTURE_TRACE": str(directory / "trace"), "FIXTURE_PID": str(directory / "helper.pid"), "PYTHONDONTWRITEBYTECODE": "1"}
      command = [sys.executable, str(ROOT / "rust/tools/tombstoned_reference.py"), str(binding)] if kind == "source" else binary
      stream = (output / f"{kind}.stderr").open("w")
      self.streams.append(stream)
      self.processes.append(subprocess.Popen(command, cwd=directory, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stream, text=True))
    self.call("configure", base="$FIXTURE/base", params="$FIXTURE/params", pc=False, device="tici")

  def paths(self, relative): return [self.root / kind / relative for kind in ["source", "native"]]

  def write(self, relative, data, mode=None):
    for path in self.paths(relative):
      path.parent.mkdir(parents=True, exist_ok=True)
      if isinstance(data, bytes): path.write_bytes(data)
      else: path.write_text(data)
      if mode is not None: path.chmod(mode)

  def expand(self, value, kind):
    match value:
      case str(): return value.replace("$FIXTURE", str(self.root / kind))
      case list(): return [self.expand(item, kind) for item in value]
      case dict(): return {key: self.expand(item, kind) for key, item in value.items()}
      case _: return value

  def call(self, op, *, compare_files=False, **fields):
    request = {"op": op, **fields}
    for process, kind in zip(self.processes, ["source", "native"], strict=True):
      assert process.stdin is not None
      process.stdin.write(json.dumps(self.expand(request, kind)) + "\n")
      process.stdin.flush()
    responses = []
    for process in self.processes:
      assert process.stdout is not None
      assert select.select([process.stdout], [], [], 40)[0], (request, "timeout")
      line = process.stdout.readline()
      assert line, (request, process.poll())
      responses.append(json.loads(line))
    expected, actual = responses
    expected_logs = expected.pop("logs")
    marker = "barrier-" + str(self.index)
    records, barrier = [], False
    while not barrier or len(records) < len(expected_logs):
      packet = self.socket.recv()
      record = json.loads(packet[1:])
      assert packet[0] == record["levelnum"]
      if record["msg"] == marker: barrier = True
      else: records.append(record)
    actual_logs = [{"msg": record["msg"], "levelnum": record["levelnum"], "exception": "exc_info" in record, "created": record["created"]} for record in records]
    source_value, native_value = normalize(expected, self.root), normalize(actual, self.root)
    source_logs, native_logs = normalize(expected_logs, self.root), normalize(actual_logs, self.root)
    if op == "cycle":
      source_logs.sort(key=lambda item: json.dumps(item, sort_keys=True))
      native_logs.sort(key=lambda item: json.dumps(item, sort_keys=True))
    passed = source_value == native_value and source_logs == native_logs
    files = None
    if compare_files:
      files = [snapshot(self.root / kind) for kind in ["source", "native"]]
      passed = passed and files[0] == files[1]
    row = {"index": self.index, "request": request, "expected": expected, "actual": actual, "source_logs": expected_logs, "native_logs": records, "files": files, "passed": passed}
    self.rows.append(row)
    with (self.output / "comparisons.jsonl").open("a") as stream: stream.write(json.dumps(row) + "\n")
    emulation_limit = bool(os.environ.get("TOMBSTONED_EMULATED_SPAWN")) and request.get("shell") == "$FIXTURE/missing-bash" and source_value == {"result": {"error": "FileNotFoundError"}, "calls": []} and native_value == {"result": {"value": "Error getting stacktrace"}, "calls": []} and source_logs == native_logs == []
    assert passed or emulation_limit, (request, source_value, native_value, source_logs, native_logs, files)
    self.index += 1
    return actual

  def stop_helpers(self):
    for path in self.paths("helper.pid"):
      if not path.exists(): continue
      pid = int(path.read_text())
      try:
        environ = Path(f"/proc/{pid}/environ").read_bytes()
        if ("FIXTURE_PID=" + str(path)).encode() in environ.split(b"\0"):
          os.kill(pid, signal.SIGTERM)
      except FileNotFoundError:
        continue

  def close(self):
    self.stop_helpers()
    for process in self.processes:
      if process.poll() is None:
        process.stdin.close()
        try: code = process.wait(timeout=10)
        except subprocess.TimeoutExpired:
          process.kill(); process.wait(); raise
        assert code == 0
    self.socket.close()
    self.context.term()
    self.endpoint.unlink(missing_ok=True)
    for stream in self.streams: stream.close()


@contextmanager
def pair(binary, binding, output):
  output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix="tombstone-oracle-") as temporary:
    value = Pair(binary, binding, output, temporary)
    try: yield value
    finally: value.close()


def gate_cases(p):
  cases = [
    ({}, False, "fixture"),
    ({"openpilot": {"git_origin": "https://github.com/fork/openpilot"}}, False, "fixture"),
    ({"openpilot": {"git_origin": "github.com/comgit@maai/openpilot"}}, False, "fixture"),
    ({"openpilot": {"git_origin": "https://github.com/commaai/openpilot"}}, False, None),
    ({"openpilot": {"git_origin": "https://github.com/commaai/openpilot"}}, False, "UnregisteredDevice"),
    ({"openpilot": {"git_origin": "https://github.com/commaai/openpilot"}}, True, "fixture"),
    ({"openpilot": {"git_origin": "https://github.com/commaai/openpilot"}}, False, "fixture"),
    ({"channel": "devel-staging", "openpilot": {"git_origin": "git@github.com:commaai/openpilot.git", "git_commit": 42}}, False, "fixture"),
    ({"channel": ["nightly"], "openpilot": {"git_origin": "github.com/commaai/openpilot", "git_commit": [True, None]}}, False, "fixture"),
    ({"openpilot": {"git_origin": None}}, False, "fixture"),
  ]
  for build, pc, dongle in cases:
    p.write("base/build.json", json.dumps(build))
    for path in p.paths("params/fixture/DongleId"):
      path.unlink(missing_ok=True)
      if dongle is not None: path.write_text(dongle)
    p.call("configure", base="$FIXTURE/base", params="$FIXTURE/params", pc=pc, device="tici")
    for project in ["selfdrive", "selfdrive_native"]: p.call("init", project=project)
  p.write("base/build.json", json.dumps({"channel": "nightly", "openpilot": {"git_origin": "github.com/commaai/openpilot", "git_commit": "abcdefghijk"}}))
  p.write("params/fixture/DongleId", b"\xff")
  p.call("init", project="selfdrive_native")
  p.write("params/fixture/DongleId", "fixture")
  p.call("init", project="selfdrive_native")
  exception = {"kind": "FixtureIo", "message": "fixture failure", "causes": [], "backtrace": None}
  for sent in ["0", "1", "junk"]:
    p.write("params/fixture/CarrotExceptionSent", sent)
    for path in p.paths("params/fixture/CarrotException"): path.unlink(missing_ok=True)
    p.call("capture", exception=exception, compare_files=True)
  p.write("params/fixture/CarrotExceptionSent", "0")
  for path in p.paths("params/fixture/CarrotException"): path.unlink(missing_ok=True)
  for path in p.paths("params"): path.chmod(0o500)
  try:
    p.call("capture", exception=exception, compare_files=True)
  finally:
    for path in p.paths("params"): path.chmod(0o700)
  p.write("params/fixture/CarrotExceptionSent", "1", 0o000)
  try:
    p.call("capture", exception=exception, compare_files=True)
  finally:
    for path in p.paths("params/fixture/CarrotExceptionSent"): path.chmod(0o600)
  for operation in ["capture_exception", "flush"]:
    p.call("fail", operation=operation)
    p.call("capture", exception=exception, log_exception=False, compare_files=True)
  p.call("fail", operation=None)
  p.call("tombstone", filename="fixture.crash", message="message", contents="contents")
  p.call("fail", operation="set_extra")
  p.call("tombstone", filename="fixture.crash", message="message", contents="contents")
  p.call("fail", operation="init")
  p.call("init", project="selfdrive_native")
  p.call("fail", operation=None)
  for path in p.paths("base/openpilot/common/version.h"): path.unlink()
  p.call("init", project="selfdrive_native")
  p.write("base/openpilot/common/version.h", '#define VERSION "fixture-1.2"\n')


def file_cases(p):
  p.call("init", project="selfdrive_native")
  for text in ["#single", "header\n", TRACE, "header\n#0 0xa fun (one(two)) tail\n", "first\nwithoutnumber\n", "a\n#0 0XFF fun(a)(b) at elsewhere\n"]:
    p.write("trace", text)
    p.write("apport/test.crash", BODY, 0o640)
    p.call("report", path="$FIXTURE/apport/test.crash", root="$FIXTURE/logs", stamp=STAMP, compare_files=True)
  p.write("trace", TRACE)
  signals = ["11", "+11", "1_1", "١١", "34", "64", "35", "0", "-1", "9999999999999999999999", "broken"]
  for index, signum in enumerate(signals):
    p.write("apport/signal.crash", f"ExecutablePath: /data/openpilot/a{index}\nSignal: {signum}\nCoreDump: data\n", 0o640)
    p.call("report", path="$FIXTURE/apport/signal.crash", root="$FIXTURE/logs", stamp=STAMP, compare_files=True)
  for index, contents in enumerate([b"bad\xff\nCoreDump:\n", b"CoreDump:\n\xff", b"CoreDump:\n" + b" " * 8192 + b"\xff", b"ExecutablePath: /data/openpilot/x\r\nSignal: 6\rProcMaps:\rhide\rProcStatus:\rshow\rCoreDump:\r"]):
    p.write(f"apport/utf8-{index}.crash", contents, 0o640)
    p.call("report", path=f"$FIXTURE/apport/utf8-{index}.crash", root="$FIXTURE/logs", stamp=STAMP, compare_files=True)
  for mode in ["exit", "invalid", "invalid-exit", "normal"]:
    p.write("mode", mode)
    p.write("apport/command.crash", BODY, 0o640)
    p.call("report", path="$FIXTURE/apport/command.crash", root="$FIXTURE/logs", stamp=STAMP, compare_files=True)
  p.write("apport/permission.crash", BODY, 0o640)
  for path in p.paths("apport"): path.chmod(0o500)
  p.call("report", path="$FIXTURE/apport/permission.crash", root="$FIXTURE/logs", stamp=STAMP, compare_files=True)
  for path in p.paths("apport"): path.chmod(0o700)
  p.write("apport/copy-error.crash", BODY, 0o640)
  for path in p.paths("logs/crash"): path.chmod(0o500)
  p.call("report", path="$FIXTURE/apport/copy-error.crash", root="$FIXTURE/logs", stamp="2020-01-03--03-04-05", compare_files=True)
  for path in p.paths("logs/crash"): path.chmod(0o700)
  # shutil.copy appends the source basename when its destination already is a directory.
  nested_name = "2020-01-04--03-04-05_abcdefgh_selfdrive_modeld_modeld"
  for path in p.paths("logs/crash/" + nested_name): path.mkdir()
  p.write("apport/nested.crash", BODY, 0o640)
  p.call("report", path="$FIXTURE/apport/nested.crash", root="$FIXTURE/logs", stamp="2020-01-04--03-04-05", compare_files=True)
  same = "logs/crash/2020-01-05--03-04-05_abcdefgh_selfdrive_modeld_modeld"
  p.write(same, BODY, 0o640)
  p.call("report", path="$FIXTURE/" + same, root="$FIXTURE/logs", stamp="2020-01-05--03-04-05", compare_files=True)
  for size in [100_000_001, 100_000_000]:
    for path in p.paths("apport/large.crash"):
      with path.open("wb") as stream:
        stream.write(b"CoreDump:\n")
        stream.truncate(size)
      path.chmod(0o640)
    p.call("report", path="$FIXTURE/apport/large.crash", root="$FIXTURE/logs", stamp=STAMP, compare_files=True)


def scan_cases(p):
  # Shared readonly input gives both scanners the exact same directory order and ctime values.
  shared = p.root / "shared"
  shared.mkdir()
  for name, mode in [("tombstone-dir", None), ("tombstone-wrong-mode", 0o600), ("valid.crash", 0o640), ("wrong.crash", 0o600), ("special.crash", 0o4640), (".hidden.crash", 0o640), ("ordinary", 0o640)]:
    path = shared / name
    if mode is None: path.mkdir()
    else: path.write_text("fixture"); path.chmod(mode)
  (shared / "linked.crash").symlink_to("valid.crash")
  p.call("scan", path=str(shared))
  (shared / "tombstone-broken").symlink_to("absent")
  p.call("scan", path=str(shared))
  (shared / "tombstone-broken").unlink()
  for index in range(1005): (shared / f"tombstone-{index:04}").touch()
  p.call("scan", path=str(shared))
  for relative, contents in [("apport/ordinary", "x"), ("apport/.hidden", "hidden"), ("apport/keep/child", "child")]: p.write(relative, contents)
  p.call("clear", path="$FIXTURE/apport", compare_files=True)
  p.call("scan", path=str(shared / "absent"))


def loop_cases(p, reporting):
  if not reporting: p.write("base/build.json", "{}")
  p.write("apport/old.crash", BODY, 0o640)
  p.write("apport/.old.crash", BODY, 0o640)
  p.call("start", apport="$FIXTURE/apport", root="$FIXTURE/logs", stamp=STAMP, compare_files=True)
  p.write("apport/new.crash", BODY, 0o640)
  p.write("apport/tombstone-unknown", "unknown", 0o600)
  p.write("apport/invalid.crash", b"bad\xff", 0o640)
  p.call("cycle", compare_files=True)
  p.call("cycle", compare_files=True)
  for name, contents in [("raw-\udcff-한-\udce2\udc82.crash", BODY.replace("modeld", "raw")), ("tombstone-\udc80-raw", "unknown"), ("invalid-\udcfe.crash", b"bad\xff")]:
    p.write("apport/" + name, contents, 0o640)
  p.call("cycle", compare_files=True)
  p.call("cycle", compare_files=True)
  p.write("apport/newer.crash", BODY.replace("modeld", "newer"), 0o640)
  p.call("cycle", compare_files=True)


def main():
  binary = Path(sys.argv[1]).resolve()
  binding = Path(sys.argv[2]).resolve()
  output = Path(sys.argv[3]).resolve()
  output.mkdir(parents=True, exist_ok=True)
  command = [*sys.argv[4:], str(binary)]
  counts = {}
  for name, scenario in [("gates", gate_cases), ("files", file_cases), ("scan", scan_cases), ("reporting-loop", lambda p: loop_cases(p, True)), ("disabled-loop", lambda p: loop_cases(p, False))]:
    with pair(command, binding, output / name) as p:
      scenario(p)
      counts[name] = p.index
  with pair(command, binding, output / "text") as p:
    rng = random.Random(76)
    for text in ["a/b-한글_Ⅻ²١e\u0301\u0345\u200b", "", "\t.\n", *["".join(chr(point if not 0xd800 <= point <= 0xdfff else point + 0x800) for point in [rng.randrange(0x110000) for _ in range(20)]) for _ in range(100)]]:
      p.call("safe", text=text)
    for commit in ["abcdefghijk", "", None, False, 0, [], [1, True, "한글"], 4, True, {}]:
      for path in ["selfdrive/modeld", "한" * 100, "a-b/.x"]:
        p.call("filename", stamp=STAMP, commit=json.dumps(commit), path=path)
    counts["text"] = p.index
  with pair(command, binding, output / "subprocess") as p:
    p.write("apport/subprocess.crash", BODY)
    p.call("retrace", path="$FIXTURE/apport/subprocess.crash", shell="$FIXTURE/missing-bash")
    for mode, milliseconds in [("timeout", 100), ("flood", 100), ("timeout", 30000)]:
      p.write("mode", mode)
      before = time.monotonic()
      p.call("retrace", path="$FIXTURE/apport/subprocess.crash", timeout_ms=milliseconds)
      elapsed = time.monotonic() - before
      assert elapsed >= milliseconds / 1000 - .1
      p.stop_helpers()
      (output / "subprocess" / f"{mode}-{milliseconds}-time.json").write_text(json.dumps({"seconds": elapsed, "configured_ms": milliseconds}) + "\n")
    counts["subprocess"] = p.index
  sources = ["openpilot/system/tombstoned.py", "openpilot/system/sentry.py", "openpilot/system/athena/registration.py", "openpilot/common/params_pyx.pyx"]
  divergences = [json.loads(line) for name in counts for line in (output / name / "comparisons.jsonl").read_text().splitlines() if not json.loads(line)["passed"]]
  manifest = {"result": "EMULATION_LIMITS" if divergences else "PASS", "divergences": divergences, "matching_comparisons": sum(counts.values()) - len(divergences), "counts": counts, "comparisons": sum(counts.values()), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "params_binding_sha256": hashlib.sha256(binding.read_bytes()).hexdigest(), "source_sha256": {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in sources}, "command": command}
  (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
  print(json.dumps(manifest, indent=2))


if __name__ == "__main__": main()
