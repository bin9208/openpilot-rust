from __future__ import annotations

import argparse
import base64
from contextlib import contextmanager
from dataclasses import asdict
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys

from cluster_simulator_compare import compare


@contextmanager
def scoped(target, **values):
  previous = {key: getattr(target, key) for key in values}
  try:
    for key, value in values.items():
      setattr(target, key, value)
    yield
  finally:
    for key, value in previous.items():
      setattr(target, key, value)


def normalize(value):
  if isinstance(value, bytes):
    return list(value)
  if isinstance(value, (tuple, list)):
    return [normalize(item) for item in value]
  if isinstance(value, dict):
    return {key: normalize(item) for key, item in value.items()}
  return value


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--native", type=Path)
  cli.add_argument("--output", type=Path, required=True)
  args = cli.parse_args()
  args.root, args.output = args.root.resolve(), args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  sys.path.insert(0, str(args.root))
  sys.path.insert(0, str(args.root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_navi_source as source
  from openpilot.selfdrive.carrot import carrot_navi as receiver_module

  with (
    scoped(source.EmbeddedNaviReceiverServer, start=lambda self: None, close=lambda self: None),
    scoped(source.time, monotonic=lambda: 10.0),
  ):
    original = source.NaviSimulatorSource(host="127.0.0.1", beacon_enabled=False)
  steps, expected = [], []
  session = "0011223344556677"
  tick = 0
  wall = 100_000

  def perform(op, **fields):
    nonlocal tick
    tick += 1
    now = 10.0 + tick * 0.05
    step = {"op": op, "session": session, "now": now, "wall": wall, "mono": round(now * 1e9), **fields}
    steps.append(step)
    with (
      scoped(source.time, monotonic=lambda: now, monotonic_ns=lambda: step["mono"], time_ns=lambda: wall * 1_000_000, strftime=lambda *args: "12:34:56"),
      scoped(receiver_module.secrets, token_hex=lambda size: session),
    ):
      if op == "negotiate":
        original.receiver.negotiate(fields["value"], "fixture-app")
      elif op == "connect":
        original.receiver.control_connected()
      elif op == "disconnect":
        original.receiver.control_disconnected()
      elif op == "json":
        original.receiver.record_json(session, fields["name"], fields["value"], "fixture-peer")
      elif op == "binary":
        metadata, payload = receiver_module.parse_binary_packet(bytes(fields["bytes"]))
        original.receiver.record_binary(session, fields["kind"], fields["name"], metadata, payload, "fixture-peer")
      elif op == "update":
        expected.append({"state": normalize(asdict(original.update())), "status": original.status_text()})

  requirements = {
    "type": "requirements_query",
    "protocol_version": 2,
    "catalog_revision": 1,
    "streams": [{"kind": kind, "name": name, "schema_version": 1} for kind, name in source.CATALOG],
  }

  def item(name, sequence, value, present=True):
    envelope = {
      "type": "item_update",
      "protocol_version": 2,
      "session_id": session,
      "kind": "json",
      "name": name,
      "sequence": sequence,
      "source_timestamp_ms": wall,
      "sent_at_ms": wall,
      "present": present,
      "value": value,
    }
    envelope.update(manifest_revision=1, schema_version=1, stream_handle=list(source.CATALOG).index(("json", name)) + 1)
    if not present:
      envelope["reason"] = "disabled"
    perform("json", name=name, value=envelope)

  def image(kind, name, sequence, present=True):
    handle = list(source.CATALOG).index((kind, name)) + 1
    payload = base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jR3sAAAAASUVORK5CYII=") if present else b""
    if present and kind == "render":
      payload = base64.b64decode(
        "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/2wBDAQkJCQwLDBgNDRgyIRwhMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjL/wAARCAABAAEDASIAAhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEAAwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSExBhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwDz2iiivoTzD//Z"
      )
    packet = (
      struct.pack(
        "!4sBBBBIIQQIHH",
        b"CNV2",
        2,
        1 if present else 4,
        2 if present and kind == "render" else 1,
        0,
        handle,
        1,
        sequence,
        wall,
        len(payload),
        int(present),
        int(present),
      )
      + payload
    )
    perform("binary", kind=kind, name=name, bytes=list(packet))

  try:
    perform("update")
    perform("negotiate", value=requirements)
    perform("connect")
    perform("update")
    for sequence in range(1, 13):
      wall += 50
      item("vehicle", sequence, {"speed_kph": sequence * 4.5, "road_name": "원본 도로"})
      item("speed", sequence, {"current_kph": sequence * 3, "road_limit_kph": [0, 30, 80][sequence % 3]})
      item("app_status", sequence, {"foreground": sequence % 2 == 0, "window_focused": True, "main_map_visible": 1, "ui_capture_available": False})
      item("camera_state", sequence, {"camera_mode": "follow", "view_level": sequence, "view_sub_level": 0.5, "tilt": None, "bearing": -0.0})
      item("composition_state", sequence, {"generation": sequence, "map_active": True, "tbt_active": sequence % 2, "camera_active": False})
      image("image", "tbt_current_full", sequence)
      image("render", "map_main", sequence)
      perform("update")
      perform("update")
    wall += 3000
    perform("update")
    wall += 1
    perform("update")
    wall -= 4000
    perform("update")
    item("vehicle", 13, None, False)
    image("image", "tbt_current_full", 13, False)
    perform("update")
    perform("disconnect")
    perform("update")
    perform("update")
    session = "8899aabbccddeeff"
    perform("negotiate", value=requirements)
    perform("connect")
    perform("update")
    item("speed", 1, {"current_kph": 88, "road_limit_kph": 60})
    perform("update")
  finally:
    original._h264_worker.close()
  request = args.output / "request.json"
  request.write_text(json.dumps(steps))
  (args.output / "source.json").write_text(json.dumps(expected))
  if args.native is None:
    print(json.dumps({"steps": len(steps), "samples": len(expected)}))
    return
  run = subprocess.run([str(args.native.resolve()), str(request)], text=True, capture_output=True, timeout=30)
  (args.output / "native.json").write_text(run.stdout)
  (args.output / "native.stderr").write_text(run.stderr)
  run.check_returncode()
  compare(expected, json.loads(run.stdout))
  report = {
    "steps": len(steps),
    "samples": len(expected),
    "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(),
    "source_sha256": hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
    "receiver_startup": "separate owned listener test; source policy comparison suppresses only socket startup",
  }
  (args.output / "result.json").write_text(json.dumps(report, indent=2))
  print(json.dumps(report))


if __name__ == "__main__":
  main()
