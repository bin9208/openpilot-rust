#!/usr/bin/env python3
"""Keep the original abort reproducer and require bounded native DEBUG publication."""

from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
from check_torque_daemon import saved_points
from torque_ipc import peer
from torque_reference import compare
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST


def check(binary: Path, numerics: Path, output: Path) -> None:
  output.mkdir(parents=True, exist_ok=False)
  data = saved_points()
  payload = output / "source-points.capnp"
  payload.write_bytes(data)
  rows = []
  limit = SERVICE_LIST["liveTorqueParameters"].queue_size // 3 - 16
  for per_bucket in [1, 440, 448, 510, 1500]:
    saved = saved_points(per_bucket)
    with peer(binary, numerics, output / f"rust-{per_bucket}", {"saved": saved, "debug": True, "frames": 6}) as client:
      omissions = []

      def bounded_message(topic, packet):
        assert topic == "liveTorqueParameters"
        # The only approved source difference is omission of an oversized diagnostic list.
        size = len(packet.to_bytes())
        if size > limit:
          omissions.append(len(packet.liveTorqueParameters.points))
          wanted = packet.to_dict()
          del wanted["liveTorqueParameters"]["points"]
          packet = log.Event.new_message(**wanted)
        client.sent.append(packet)

      client.context["pm"].send = bounded_message
      packet = client.start()
      assert packet["liveTorqueParameters"]["totalBucketPoints"] == per_bucket * 8
      for index in range(1, 26):
        client.step(index)
      assert client.process.wait(timeout=10) == 0
      persisted = (client.directory / "LiveTorqueParameters").read_bytes()
      with log.Event.from_bytes(persisted) as actual, log.Event.from_bytes(client.params.writes[-1]) as expected:
        wanted = expected.to_dict()
        wanted["logMonoTime"] = actual.logMonoTime
        persisted_fields = compare(actual.to_dict(), wanted)
        assert len(actual.liveTorqueParameters.points) == per_bucket * 8
      native_packets = list(client.destination.glob("message-*.capnp"))
      assert len(native_packets) == 6
      assert all(len(path.read_bytes()) <= limit for path in native_packets)
      rust_error = (client.destination / "daemon.log").read_text()
      if per_bucket >= 448:
        assert len(omissions) == 6
        warnings = [line for line in rust_error.splitlines() if "omitting" in line and "DEBUG points" in line]
        assert len(warnings) == 1, (per_bucket, warnings)
      else:
        warnings = [line for line in rust_error.splitlines() if "omitting" in line and "DEBUG points" in line]
        assert not omissions and not warnings
      rows.append({"points": per_bucket * 8, "omitted_publications": len(omissions), "fields": client.fields,
                   "persisted_fields": persisted_fields, "warnings": len(warnings), "exit": client.process.returncode})
  child = Path(__file__).with_name('torque_capacity_child.py').resolve()
  try:
    with tempfile.TemporaryDirectory(prefix="msgq_torque-capacity-", dir="/dev/shm") as directory:
      environment = dict(os.environ, OPENPILOT_PREFIX=Path(directory).name.removeprefix("msgq_"))
      original = subprocess.run([sys.executable, str(child), str(payload)], env=environment, cwd="/tmp", capture_output=True, timeout=5)
  except subprocess.TimeoutExpired as error:
    (output / "source-stderr.log").write_bytes(error.stderr or b'')
    (output / "source-stages.jsonl").write_bytes(error.stdout or b'')
    raise
  (output / "source-stderr.log").write_bytes(original.stderr)
  (output / "source-stages.jsonl").write_bytes(original.stdout)
  assert original.returncode == -signal.SIGABRT, (original.returncode, original.stderr)
  stages = [json.loads(line) for line in original.stdout.splitlines()]
  assert [item['stage'] for item in stages] == ['before_import', 'after_import', 'before_send'], stages
  report = {
    "result": "pass",
    "inherited_defect_reproduced": True,
    "native_scenarios": rows,
    "points": 4080,
    "packet_bytes": len(data),
    "queue_bytes": 250 * 1024,
    "source_returncode": original.returncode,
    "rust_returncode": 0,
    "rust_error": rust_error,
    "source_guard": "msgq_repo/msgq/msgq.cc:246 assert(3 * total_msg_size <= q->size)",
    "source_stages": stages,
    "source_core_dump": "disabled with PR_SET_DUMPABLE=0; SIGABRT and original size guard unchanged",
    "scope": "host IPC regression; DEBUG points omitted only over transport limit, full learning and Params oracle retained",
  }
  (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report, indent=2))


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--numerics", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  check(args.binary.resolve(), args.numerics.resolve(), args.output.resolve())
