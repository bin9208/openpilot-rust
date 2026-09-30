#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["pyzmq==27.2.0", "pycapnp==2.1.0", "sentry-sdk==2.55.0", "zstandard==0.25.0", "numpy==2.5.3"]
# ///
# How to run: PYTHONPATH=<built-msgq>:.:rust/tools python rust/tools/check_tombstoned_runtime.py DAEMON COLLECTOR PARAMS_BINDING OUTPUT
"""Run the actual daemon, native collector, original cereal/msgq readers and local HTTP capture."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

from openpilot.cereal import log
from check_crash_sdk_transport import Receiver
from check_tombstoned_reference import BODY, ROOT, TRACE, STAMP
from logmessaged_native import Peer


def wait_sleep(process, stderr):
  deadline = time.monotonic() + 20
  while time.monotonic() < deadline:
    assert process.poll() is None, stderr.read_text()
    if "tombstoned: ready" in stderr.read_text() and Path(f"/proc/{process.pid}/wchan").read_text().strip() == "hrtimer_nanosleep":
      return time.monotonic()
    time.sleep(.01)
  raise AssertionError("daemon did not reach its first scan sleep")


def read_event(subscriber, expected, output):
  packet = subscriber.receive()
  assert packet is not None, expected
  with output.open("ab") as stream: stream.write(packet)
  with log.Event.from_bytes(packet) as event:
    assert event.valid and event.which() == expected and event.logMonoTime > 0
    result = json.loads(getattr(event, expected))
  return result


def fixture(root, namespace):
  for name in ["apport", "logs", "base/openpilot/common", "params/" + namespace, "source-params/fixture", "bin"]:
    (root / name).mkdir(parents=True, exist_ok=True)
  (root / "base/build.json").write_text(json.dumps({"channel": "nightly", "openpilot": {"git_origin": "git@github.com:commaai/openpilot.git", "git_commit": "abcdefghijk"}}))
  (root / "base/openpilot/common/version.h").write_text('#define VERSION "fixture-runtime"\n')
  (root / "params" / namespace / "DongleId").write_text("fixture-device")
  (root / "source-params/fixture/DongleId").write_text("fixture-device")
  (root / "trace").write_text(TRACE)
  helper = root / "bin/apport-retrace"
  helper.write_text('#!/bin/bash\nhead -c 4096 "$2" > "$FIXTURE_INPUT"\ncat "$FIXTURE_TRACE"\n')
  helper.chmod(0o755)
  for name in ["old.crash", ".hidden.crash"]:
    (root / "apport" / name).write_text(BODY)
    (root / "apport" / name).chmod(0o640)


def run_case(daemon, collector, binding, output, enabled):
  output.mkdir(parents=True)
  receiver = Receiver()
  peer = Peer(collector, output / "collector", original=False)
  process = None
  try:
    peer.start()
    with tempfile.TemporaryDirectory(prefix="tombstone-runtime-") as temporary:
      root = Path(temporary)
      fixture(root, peer.prefix)
      environment = {**os.environ, "OPENPILOT_PREFIX": peer.prefix, "PARAMS_ROOT": str(root / "params"), "PATH": str(root / "bin") + ":/usr/bin:/bin", "FIXTURE_INPUT": str(root / "retrace.input"), "FIXTURE_TRACE": str(root / "trace")}
      command = [str(daemon), "--base-dir", str(root / "base"), "--apport-dir", str(root / "apport"), "--log-root", str(root / "logs"), "--cycles", "3" if enabled else "2", "--local-sentry-dsn", receiver.dsn]
      if enabled: command += ["--local-reporting-device", "tici"]
      stderr = output / "daemon.stderr"
      with stderr.open("w") as errors, (output / "daemon.stdout").open("w") as stdout:
        process = subprocess.Popen(command, cwd=root, env=environment, stdout=stdout, stderr=errors)
      entered_sleep = wait_sleep(process, stderr)
      assert not (root / "apport/old.crash").exists()
      assert (root / "apport/.hidden.crash").exists()
      for name, data in [("new-\udcff-한-\udce2\udc82.crash", BODY.encode()), ("invalid.crash", b"bad\xff"), ("tombstone-unknown", b"unknown"), ("wrong-mode.crash", b"ignored")]:
        (root / "apport" / name).write_bytes(data)
        (root / "apport" / name).chmod(0o600 if name == "wrong-mode.crash" else 0o640)
      created = time.monotonic()
      records = {name: [] for name in ["logMessage", "errorLogMessage"]}
      if enabled:
        for name, count in [("logMessage", 6), ("errorLogMessage", 3)]:
          for _ in range(count): records[name].append(read_event(peer.subscribers[name], name, output / f"{name}.bin"))
        delay = time.monotonic() - created
        assert 4.0 <= delay < 9.5, delay
        event = receiver.events.get(timeout=5)["event"]
        assert event["user"] == {"id": "fixture-device"}
        assert event["tags"]["dirty"] is False and event["tags"]["device"] == "tici"
        # A fresh original-source execution establishes the exact report text independently.
        source_file = root / "source.crash"
        source_file.write_text(BODY)
        source_requests = [
          {"op": "configure", "base": str(root / "base"), "params": str(root / "source-params"), "pc": False, "device": "tici"},
          {"op": "report", "path": str(source_file), "root": str(root / "source-logs"), "stamp": STAMP},
        ]
        source = subprocess.run([sys.executable, str(ROOT / "rust/tools/tombstoned_reference.py"), str(binding)], input="".join(json.dumps(request)+"\n" for request in source_requests), env=environment, text=True, capture_output=True, check=True)
        (output / "source.stdout").write_text(source.stdout)
        expected = json.loads(source.stdout.splitlines()[-1])
        assert expected["result"] == {"value": None}
        expected_calls = expected["calls"]
        assert event["message"] == next(call["fields"]["message"] for call in expected_calls if call["op"] == "capture_message")
        assert event["extra"]["tombstone"] == json.loads(next(call["fields"]["value_json"] for call in expected_calls if call["op"] == "set_extra" and call["fields"]["key"] == "tombstone"))
        assert event["extra"]["tombstone_fn"] == str(root / "apport/new-\udcff-한-\udce2\udc82.crash")
        assert any(row["msg"] == "reporting new tombstone " + event["extra"]["tombstone_fn"] for row in records["logMessage"])
        copies = list((root / "logs/crash").iterdir())
        assert len(copies) == 1 and copies[0].read_bytes() == BODY.encode()
        assert copies[0].stat().st_mode & 0o7777 == 0o640
        assert not (root / "apport/new-\udcff-한-\udce2\udc82.crash").exists()
        assert (root / "apport/invalid.crash").exists() and (root / "apport/tombstone-unknown").exists()
        (output / "copied.crash").write_bytes(copies[0].read_bytes())
        (output / "sdk-event.json").write_text(json.dumps(event, indent=2)+"\n")
      else:
        delay = None
      assert process.wait(timeout=15) == 0
      assert (root / "apport/wrong-mode.crash").exists()
      if not enabled:
        assert not (root / "apport/new-\udcff-한-\udce2\udc82.crash").exists() and not (root / "apport/invalid.crash").exists() and not (root / "apport/tombstone-unknown").exists()
        assert not (root / "logs/crash").exists() and receiver.events.empty()
      for name in records: assert peer.subscribers[name].receive(non_blocking=True) is None, (name, "duplicate second-cycle record")
      code, seconds = peer.stop()
      assert code == 0
      disk = [json.loads(line) for path in peer.root.glob("swaglog.*") for line in path.read_text().splitlines()]
      assert len(disk) == (6 if enabled else 0)
      assert all(row["ctx"]["runtime_language"] == "rust" for row in disk)
      (output / "records.json").write_text(json.dumps({"wire": records, "disk": disk}, indent=2)+"\n")
      result = {"result": "PASS", "reporting_enabled": enabled, "argv": command, "first_scan_sleep_at": entered_sleep, "new_crash_report_delay_seconds": delay, "log_messages": len(records["logMessage"]), "error_messages": len(records["errorLogMessage"]), "disk_records": len(disk), "collector_shutdown_seconds": seconds, "subsequent_cycle_duplicates": 0}
      (output / "manifest.json").write_text(json.dumps(result, indent=2)+"\n")
      return result
  finally:
    if process is not None and process.poll() is None:
      process.send_signal(signal.SIGTERM)
      try: process.wait(timeout=5)
      except subprocess.TimeoutExpired: process.kill(); process.wait()
    peer.close()
    receiver.close()


def startup_failures(daemon, output):
  with tempfile.TemporaryDirectory(prefix="tombstone-startup-") as temporary:
    root = Path(temporary)
    fixture(root, "fixture")
    before = (root / "apport/old.crash").read_bytes()
    cases = [["--cycles", "0"], ["--local-reporting-device", "tici"], ["--local-sentry-dsn", "https://fixture@example.invalid/1"]]
    results = []
    for arguments in cases:
      command = [str(daemon), "--apport-dir", str(root / "apport"), "--base-dir", str(root / "base"), *arguments]
      result = subprocess.run(command, capture_output=True, text=True, timeout=5)
      assert result.returncode != 0 and (root / "apport/old.crash").read_bytes() == before
      results.append({"argv": command, "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
    (root / "base/build.json").write_text("{")
    command = [str(daemon), "--apport-dir", str(root / "apport"), "--base-dir", str(root / "base"), "--cycles", "1"]
    result = subprocess.run(command, capture_output=True, text=True, timeout=5)
    assert result.returncode != 0 and (root / "apport/old.crash").read_bytes() == before
    results.append({"argv": command, "exit_code": result.returncode, "stderr": result.stderr})
  (output / "startup-failures.json").write_text(json.dumps(results, indent=2)+"\n")
  return len(results)


def main():
  daemon, collector, binding, output = map(lambda value: Path(value).resolve(), sys.argv[1:])
  output.mkdir(parents=True, exist_ok=True)
  results = [run_case(daemon, collector, binding, output / name, enabled) for name, enabled in [("enabled", True), ("pc-disabled", False)]]
  failures = startup_failures(daemon, output)
  manifest = {"result": "PASS", "daemon_sha256": hashlib.sha256(daemon.read_bytes()).hexdigest(), "collector_sha256": hashlib.sha256(collector.read_bytes()).hexdigest(), "python_version": sys.version, "pycapnp_version": __import__("capnp").__version__, "cases": results, "startup_failure_cases": failures}
  (output / "manifest.json").write_text(json.dumps(manifest, indent=2)+"\n")
  print(json.dumps(manifest, indent=2))


if __name__ == "__main__": main()
