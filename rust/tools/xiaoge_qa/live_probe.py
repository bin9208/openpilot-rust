from __future__ import annotations

import copy
import json
from pathlib import Path
import socket
import struct
import time
from typing import Literal

import httpx2
from openpilot.cereal import messaging

from xiaoge_qa.sockets import read_exact


def stable_status(status):
  result = copy.deepcopy(status)
  del result["lastInferenceAgeSeconds"]
  del result["camera"]["lastFrameAgeSeconds"]
  result.pop("inference")
  lane = result["lane"]
  del lane["lastFrameAgeSeconds"]
  del lane["result"]["updatedMonoTimeNanos"]
  lane.pop("inference")
  for group in ("imageSide", "vehicleSide"):
    for side in result[group].values():
      del side["updatedMonoTimeNanos"]
  if not result["camera"]["available"]:
    gate = result["gate"]
    eligible = gate["eligibleSides"]
    assert gate["side"] in eligible if eligible else gate["side"] == ""
    # Source selects its round-robin cursor even without camera input. No
    # inference advances it while stale; its retained phase is asynchronous.
    gate["side"] = eligible[0] if eligible else ""
  return result


def stable_publication(payload):
  result = copy.deepcopy(payload)
  del result["lane"]["latencyMs"]
  del result["lane"]["receivedMonoTimeNanos"]
  blindspot = result["blindspot"]
  del blindspot["receivedMonoTimeNanos"]
  del blindspot["receivedMonoTimeNanosBySide"]
  valid_sides = blindspot["validSides"]
  assert blindspot["side"] in valid_sides if valid_sides else blindspot["side"] == ""
  # Both eligible sides are inferred in round-robin order. The selected display
  # side can change between status and publication; retain its admissibility.
  blindspot["side"] = valid_sides[0] if valid_sides else ""
  return result


class Probe:
  def __init__(self, output: Path, connection: httpx2.Client, process, inputs):
    self.output, self.connection, self.process, self.inputs = output, connection, process, inputs
    self.subscriber = messaging.sub_sock("customReservedRawData0")
    self.publications = []
    self.phase = "startup"

  def drain(self) -> None:
    with (self.output / "publications.jsonl").open("a") as trace, (self.output / "publications.bin").open("ab") as raw:
      for data in messaging.drain_sock_raw(self.subscriber):
        raw.write(struct.pack("<I", len(data)))
        raw.write(data)
        event = messaging.log_from_bytes(data)
        assert event.valid and event.logMonoTime > 0
        payload = json.loads(bytes(event.customReservedRawData0))
        assert payload["type"] == "xiaogeVision" and payload["version"] == 1
        now = time.monotonic_ns()
        for name in ["lane", "blindspot"]:
          assert 0 < payload[name]["receivedMonoTimeNanos"] <= now
        blindspot = payload["blindspot"]
        timestamps = blindspot["receivedMonoTimeNanosBySide"]
        assert set(timestamps) == {"left", "right"}
        assert all(type(stamp) is int and 0 <= stamp <= now for stamp in timestamps.values())
        if max(timestamps.values()) > 0:
          assert blindspot["receivedMonoTimeNanos"] == max(timestamps.values())
        else:
          assert not blindspot["valid"]
        assert blindspot["validSides"] == [side for side in ("left", "right") if side in blindspot["validSides"]]
        assert blindspot["valid"] == bool(blindspot["validSides"])
        assert all(timestamps[side] > 0 for side in blindspot["validSides"])
        trace.write(json.dumps({"phase": self.phase, "mono_time": event.logMonoTime, "payload": payload}) + "\n")
        self.publications.append(payload)

  def status(self):
    assert self.process.poll() is None, self.process.returncode
    if self.inputs.future.done():
      self.inputs.future.result()
      raise AssertionError("input producer exited early")
    self.drain()
    response = self.connection.get("/api/status")
    response.raise_for_status()
    return response.json()

  def await_status(self, name: str, predicate, timeout: float = 15.0):
    self.phase = name
    deadline = time.monotonic() + timeout
    observed = None
    while time.monotonic() < deadline:
      try:
        observed = self.status()
      except httpx2.ConnectError:
        time.sleep(.02)
        continue
      if predicate(observed):
        (self.output / f"{name}.json").write_text(json.dumps(observed, indent=2) + "\n")
        return observed
      time.sleep(.02)
    (self.output / f"{name}-timeout.json").write_text(json.dumps(observed, indent=2) + "\n")
    raise TimeoutError(f"Xiaoge phase did not settle: {name}")

  def publication(self, status, start: int):
    sides = status["vehicleSide"]
    expected = {"type": "xiaogeVision", "version": 1,
      "lane": {"leftLine": status["lane"]["result"]["leftLine"], "rightLine": status["lane"]["result"]["rightLine"],
               "valid": status["lane"]["resultFresh"]},
      "blindspot": {"left": sides["left"]["active"], "right": sides["right"]["active"],
                    "valid": any(side["valid"] for side in sides.values()),
                    "validSides": [name for name in ("left", "right") if sides[name]["valid"]],
                    "side": next((name for name in ["left", "right"] if sides[name]["valid"]), "")}}
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
      self.drain()
      if any(stable_publication(payload) == expected for payload in self.publications[start:]):
        return expected
      time.sleep(.02)
    raise AssertionError(("missing Cereal phase result", expected, self.publications[start:]))


def tcp_values(port: int, empty: bool = False, tesla: bool = True, speed: float = 20.0):
  deadline = time.monotonic() + 5
  with socket.create_connection(("127.0.0.1", port), timeout=5) as stream:
    while time.monotonic() < deadline:
      length = struct.unpack("!I", read_exact(stream, 4))[0]
      assert 0 < length < 65536
      packet = json.loads(read_exact(stream, length))
      values = packet["data"]
      if empty and not values:
        return values
      if (not empty and set(values) == {"carState", "modelV2", "systemState", "controlsState"} and
          ("stopLineDist" in values["carState"]) == tesla and values["carState"]["vEgo"] == speed):
        return values
  raise TimeoutError("Xiaoge TCP values did not settle")


def snapshots(connection: httpx2.Client, output: Path) -> None:
  for stream in ["wide", "road"]:
    response = connection.get(f"/api/snapshot?stream={stream}")
    response.raise_for_status()
    assert response.headers["Content-Type"] == "image/jpeg"
    assert response.content[:2] == b"\xff\xd8" and response.content[-2:] == b"\xff\xd9"
    (output / f"{stream}.jpg").write_bytes(response.content)


def side_ready(status, side: Literal["left", "right"], minimum: int = 2) -> bool:
  return (status["model"]["loaded"] and status["lane"]["loaded"] and status["camera"]["available"] and
          status["lane"]["cameraAvailable"] and status["inference"]["count"] >= minimum and status["lane"]["resultFresh"] and
          status["gate"]["active"] and status["gate"]["side"] == side and status["vehicleSide"][side]["valid"] and
          all(status["vehicleSide"][name]["valid"] for name in status["gate"]["eligibleSides"]))
