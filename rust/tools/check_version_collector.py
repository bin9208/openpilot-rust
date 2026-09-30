#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["pyzmq==27.1.0", "pycapnp==2.2.2"]
# ///
# How to run: PYTHONPATH=<built-msgq>:.:rust/tools python rust/tools/check_version_collector.py PROBE COLLECTOR OUTPUT
"""Drive the missing-metadata error through the native collector and original cereal reader."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from openpilot.cereal import log
from logmessaged_native import Peer


def main() -> None:
  probe, collector, output = map(lambda value: Path(value).resolve(), sys.argv[1:])
  output.mkdir(parents=True, exist_ok=True)
  peer = Peer(collector, output / "collector", original=False)
  try:
    peer.start()
    with tempfile.TemporaryDirectory(prefix="version-no-metadata-") as path:
      request = json.dumps({"op": "metadata", "path": path}) + "\n"
      # Execute the unchanged source oracle independently to establish the event contract.
      source = subprocess.run([sys.executable, str(Path(__file__).with_name("check_version_reference.py")), "--source"], input=request, text=True, capture_output=True, check=True)
      expected = json.loads(source.stdout)
      result = subprocess.run([str(probe)], input=request, text=True, capture_output=True, check=True,
                              env={**os.environ, "OPENPILOT_PREFIX": peer.prefix})
    (output / "probe.stdout").write_text(result.stdout)
    (output / "probe.stderr").write_text(result.stderr)
    (output / "source.json").write_text(source.stdout)
    assert json.loads(result.stdout) == {key: value for key, value in expected.items() if key != "logs"}
    assert len(expected["logs"]) == 1
    messages = []
    for name in ["logMessage", "errorLogMessage"]:
      packet = peer.subscribers[name].receive()
      assert packet is not None
      (output / f"{name}.bin").write_bytes(packet)
      with log.Event.from_bytes(packet) as event:
        assert event.valid and event.which() == name and event.logMonoTime > 0
        text = getattr(event, name)
      record = json.loads(text)
      assert {key: record[key] for key in expected["logs"][0]} == expected["logs"][0]
      assert record["ctx"]["runtime_language"] == "rust"
      messages.append(record)
    assert messages[0] == messages[1]
    code, _ = peer.stop()
    assert code == 0
    records = [json.loads(line) for path in peer.root.glob("swaglog.*") for line in path.read_text().splitlines()]
    assert len(records) == 1
    assert records[0]["msg$s"] == expected["logs"][0]["msg"]
    assert records[0]["levelnum"] == 40 and records[0]["exc_info"] == expected["logs"][0]["exc_info"]
    (output / "records.json").write_text(json.dumps(records, indent=2) + "\n")
    report = {"result": "PASS", "logMessage": 1, "errorLogMessage": 1, "disk_records": 1, "source_logs": expected["logs"],
              "probe_sha256": hashlib.sha256(probe.read_bytes()).hexdigest(), "collector_sha256": hashlib.sha256(collector.read_bytes()).hexdigest()}
    (output / "manifest.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
  finally:
    peer.close()


if __name__ == "__main__":
  main()
