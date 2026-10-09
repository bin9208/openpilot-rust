from __future__ import annotations

import dataclasses
import json
import os
import selectors
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import TypeAlias

from openpilot.cereal import log, messaging
from openpilot.system.webrtc.device.video import LiveStreamVideoStreamTrack
from openpilot.system.webrtc.schema import generate_field
from openpilot.system.webrtc.webrtcd import CerealOutgoingMessageProxy

Json: TypeAlias = str | int | float | bool | None | list["Json"] | dict[str, "Json"]


@dataclasses.dataclass(frozen=True, slots=True)
class Case:
  name: str
  service: str
  data: Json


def run(binary: Path, mode: str, payload: bytes = b"", *args: str) -> subprocess.CompletedProcess[bytes]:
  return subprocess.run([str(binary), mode, *args], input=payload, capture_output=True, check=False, timeout=10)


def outgoing_cases(binary: Path, output: Path) -> list[str]:
  cases = [
    Case("struct", "carState", {"vEgo": 12.375, "gearShifter": "drive", "wheelSpeeds": {"fl": 3.25}}),
    Case("nonfinite", "carState", {"vEgo": float("nan"), "aEgo": float("inf")}),
    Case("group", "liveCalibration", {"deprecated": {"calStatus": 2}, "rpyCalib": []}),
    Case("text", "logMessage", "한글\n\u0000"),
    Case("list-empty", "can", []),
    Case("list-data", "can", [{"address": 1, "dat": "ABC", "src": 2}]),
    Case("root-data", "ubloxRaw", "ABC"),
    Case("unknown-enum", "carState", {"gearShifter": 65535}),
  ]
  passed = []
  proxy = CerealOutgoingMessageProxy(None)
  for case in cases:
    size = None if isinstance(case.data, dict) else len(case.data)
    message = messaging.new_message(case.service, size=size, logMonoTime=123456789, valid=True)
    setattr(message, case.service, case.data)
    raw = message.to_bytes()
    result = run(binary, "outgoing", raw)
    source_error = None
    try:
      expected = json.dumps({"type": case.service, "logMonoTime": 123456789, "valid": True, "data": proxy.to_json(getattr(message.as_reader(), case.service))})
    except (TypeError, ValueError, RuntimeError) as error:
      source_error = type(error).__name__
      expected = None
    (output / f"{case.name}-outgoing.bin").write_bytes(raw)
    (output / f"{case.name}-outgoing.json").write_text(
      json.dumps(
        {"source": expected, "source_error": source_error, "returncode": result.returncode, "stdout": result.stdout.decode(), "stderr": result.stderr.decode()},
        indent=2,
      )
      + "\n"
    )
    if expected is None:
      assert result.returncode != 0, case.name
    else:
      assert result.returncode == 0, result.stderr
      assert result.stdout.decode().rstrip("\n") == expected, case.name
    passed.append(case.name)
  return passed


def incoming_cases(binary: Path, output: Path) -> list[str]:
  cases = [
    Case("struct", "carState", {"vEgo": 9.75, "gearShifter": "reverse", "wheelSpeeds": {"fr": 1.5}}),
    Case("nonfinite", "carState", {"vEgo": float("nan"), "aEgo": -float("inf")}),
    Case("group", "liveCalibration", {"deprecated": {"calStatus": 3}, "rpyCalib": [1, 2, 3]}),
    Case("text", "logMessage", "한글"),
    Case("list", "can", [{"address": 1, "dat": "AB", "src": 2}, {"address": 3, "dat": "CD", "src": 4}]),
    Case("root-data", "ubloxRaw", "ABC"),
  ]
  passed = []
  for case in cases:
    request = json.dumps({"type": case.service, "data": case.data, "valid": True, "logMonoTime": 999})
    size = None if isinstance(case.data, dict) else len(case.data)
    original = messaging.new_message(case.service, size=size, logMonoTime=240000)
    setattr(original, case.service, case.data)
    result = run(binary, "incoming", request.encode())
    assert result.returncode == 0, result.stderr
    with log.Event.from_bytes(result.stdout) as native:
      native_data = native.to_dict()
    original_bytes = original.to_bytes()
    with log.Event.from_bytes(original_bytes) as source:
      source_data = source.to_dict()
    assert repr(native_data) == repr(source_data), case.name
    (output / f"{case.name}-incoming-native.bin").write_bytes(result.stdout)
    (output / f"{case.name}-incoming-source.bin").write_bytes(original_bytes)
    (output / f"{case.name}-incoming.json").write_text(
      json.dumps({"input": request, "returncode": result.returncode, "source_fields": repr(source_data), "native_fields": repr(native_data)}, indent=2) + "\n"
    )
    passed.append(case.name)
  return passed


def camera_cases(binary: Path, output: Path, prefix: str) -> list[str]:
  passed = []
  for camera, carrot in [("road", False), ("road", True), ("wideRoad", True), ("driver", True)]:
    source_track = None
    publisher = None
    try:
      endpoint = LiveStreamVideoStreamTrack.camera_to_sock_mapping[camera]
      publisher = messaging.PubMaster([endpoint])
      source_track = LiveStreamVideoStreamTrack(camera, use_source_frame_timestamps=carrot)
      with subprocess.Popen([str(binary), "camera", camera, "carrot" if carrot else "standard"], stdout=subprocess.PIPE, stderr=subprocess.PIPE) as process:
        try:
          assert process.stdout is not None
          with selectors.DefaultSelector() as selector:
            selector.register(process.stdout, selectors.EVENT_READ)
            assert selector.select(5), "native camera readiness timeout"
            assert process.stdout.readline() == b"READY\n"
          message = messaging.new_message(endpoint)
          encoded = getattr(message, endpoint)
          encoded.idx.frameId = 4294967295
          encoded.header = b"\x00\x00\x00\x01\x67"
          encoded.data = b"owned-frame"
          publisher.send(endpoint, message)
          stdout, stderr = process.communicate(timeout=5)
          assert process.returncode == 0, stderr
          native = json.loads(stdout)
          recv = source_track.recv()
          try:
            recv.send(None)
          except StopIteration as completed:
            packet = completed.value
          else:
            recv.close()
            raise AssertionError("owned source message was not available")
          expected = {"frame_id": 4294967295, "pts": packet.pts, "data": list(bytes(packet))}
          assert native == expected, (camera, carrot, native, expected)
          name = f"{camera}-{'carrot' if carrot else 'standard'}"
          (output / f"{name}-camera.json").write_text(
            json.dumps({"source": expected, "native": native, "namespace": prefix, "returncode": process.returncode, "stderr": stderr.decode()}, indent=2)
            + "\n"
          )
          passed.append(name)
        finally:
          if process.poll() is None:
            process.kill()
            process.wait(timeout=5)
    finally:
      if source_track is not None:
        source_track.close_sock()
        source_track._sock = None
      if publisher is not None:
        publisher.sock.clear()
      source_track = None
      publisher = None
  return passed


def cameras(binary: Path, output: Path) -> list[str]:
  previous = os.environ.get("OPENPILOT_PREFIX")
  with tempfile.TemporaryDirectory(prefix="msgq_rtc240-", dir="/dev/shm") as namespace:
    prefix = Path(namespace).name.removeprefix("msgq_")
    os.environ["OPENPILOT_PREFIX"] = prefix
    try:
      return camera_cases(binary, output, prefix)
    finally:
      if previous is None:
        os.environ.pop("OPENPILOT_PREFIX", None)
      else:
        os.environ["OPENPILOT_PREFIX"] = previous


def main() -> None:
  binary, output = Path(sys.argv[1]), Path(sys.argv[2])
  output.mkdir(parents=True, exist_ok=True)
  names = [name for name in log.Event.schema.fields if not name.endswith("DEPRECATED")]
  original = {name: generate_field(log.Event.schema.fields[name]) for name in names}
  result = run(binary, "schema", b"", *names)
  (output / "schema-native.json").write_bytes(result.stdout)
  (output / "schema-source.json").write_text(json.dumps(original, indent=2) + "\n")
  assert result.returncode == 0, result.stderr
  assert json.loads(result.stdout) == original
  summary = {
    "schema_fields": len(names),
    "outgoing": outgoing_cases(binary, output),
    "incoming": incoming_cases(binary, output),
    "actual_msgq_cameras": cameras(binary, output),
  }
  (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
  print(json.dumps(summary, indent=2))


if __name__ == "__main__":
  main()
