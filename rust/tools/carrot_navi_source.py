#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["aiohttp==3.13.3"]
# ///
# ─── How to run ───
# uv run rust/tools/carrot_navi_source.py INPUT.json OUTPUT.json
# A cached source environment can run this directly with the checkout on PYTHONPATH.
# ──────────────────
from __future__ import annotations

from contextlib import ExitStack, redirect_stdout
from dataclasses import dataclass
from enum import StrEnum
import hashlib
from io import BytesIO, TextIOWrapper
import json
from pathlib import Path
import sys
from typing import TypedDict, assert_never
from unittest.mock import patch

from openpilot.selfdrive.carrot import carrot_navi as source
from openpilot.selfdrive.carrot.carrot_navi_cereal import build_carrot_navi_payload

type Json = None | bool | int | float | str | list[Json] | dict[str, Json]


class Operation(StrEnum):
  MANIFEST = "manifest"
  NEGOTIATE = "negotiate"
  CONNECT = "connect"
  DISCONNECT = "disconnect"
  CONTROL = "control"
  JSON = "json"
  BINARY = "binary"
  PARSE_BINARY = "parse_binary"
  PARSE_JSON = "parse_json"
  STREAM = "stream"
  CONFIG = "config"
  FAIL = "fail"
  DRAIN = "drain"
  BOOTSTRAP = "bootstrap"
  PUBLISH = "publish"
  PAYLOAD = "payload"
  HZ = "hz"
  BITRATE = "bitrate"


class Step(TypedDict, total=False):
  op: str
  value: Json
  kind: str
  name: str
  session: str
  peer: str
  packet: str
  metadata: dict[str, Json]
  config: dict[str, Json]


class Case(TypedDict):
  id: str
  config: dict[str, Json]
  steps: list[Step]


@dataclass(slots=True)  # noqa: MUTABLE_OK
class Clock:
  """Advance only when the original requests a clock, retaining call order."""
  wall_calls: int = 0
  mono_calls: int = 0

  def wall(self) -> int:
    self.wall_calls += 1
    return 1_700_000_000_000 + self.wall_calls

  def mono(self) -> int:
    self.mono_calls += 1
    return 1_000_000_000 + self.mono_calls * 1_000


def records(values: list[source.ItemRecord]) -> list[Json]:
  return [{"summary": value.summary(), "payload_hex": value.payload.hex() if value.payload is not None else None,
    "received_mono_ns": value.received_mono_ns} for value in values]


def dashboard(receiver: source.CarrotNaviReceiver) -> dict[str, Json]:
  snapshot = receiver.dashboard_snapshot()
  for name in ("records", "binary_configs"):
    snapshot[name] = {key: records([record])[0] for key, record in snapshot[name].items()}
  return snapshot


def perform(receiver: source.CarrotNaviReceiver, step: Step) -> Json:
  value = step.get("value")
  session = step.get("session", "0011223344556677")
  name, kind = step.get("name", "vehicle"), step.get("kind", "json")
  peer = step.get("peer", "fixture-peer")
  match Operation(step["op"]):
    case Operation.MANIFEST:
      return source.build_manifest(session, **step.get("config", {}))
    case Operation.NEGOTIATE:
      return receiver.negotiate(value, "fixture-app")
    case Operation.CONNECT:
      receiver.control_connected()
    case Operation.DISCONNECT:
      receiver.control_disconnected()
    case Operation.CONTROL:
      receiver.record_control(value, peer)
    case Operation.JSON:
      receiver.record_json(session, name, value, peer)
    case Operation.BINARY:
      receiver.record_binary(session, kind, name, step["metadata"], bytes.fromhex(step["packet"]), peer)
    case Operation.PARSE_BINARY:
      metadata, payload = source.parse_binary_packet(bytes.fromhex(step["packet"]))
      return {"metadata": metadata, "payload_hex": payload.hex()}
    case Operation.PARSE_JSON:
      return source.parse_json_object(value)
    case Operation.STREAM:
      return receiver.stream_config(session, kind, name)
    case Operation.CONFIG:
      return receiver.set_map_config(**step["config"])
    case Operation.FAIL:
      receiver.fail(value, peer)
    case Operation.DRAIN:
      return records(receiver.drain_media_updates())
    case Operation.BOOTSTRAP:
      return records(receiver.media_bootstrap())
    case Operation.PUBLISH:
      receiver.record_cereal_publish(value)
    case Operation.PAYLOAD:
      return build_carrot_navi_payload(receiver.cereal_snapshot() if value is None else value, publish_mono_ns=999)
    case Operation.HZ:
      return source.resolve_map_hz(value)
    case Operation.BITRATE:
      return source.resolve_map_bitrate_kbps(*value)
    case unreachable:
      assert_never(unreachable)
  return None


def capture(case: Case) -> dict[str, Json]:
  clock = Clock()
  output = BytesIO()
  writer = TextIOWrapper(output, encoding="utf-8", errors="strict", write_through=True)
  rows: list[Json] = []
  with ExitStack() as stack:
    stack.enter_context(patch.object(source, "now_ms", clock.wall))
    stack.enter_context(patch.object(source.time, "monotonic_ns", clock.mono))
    stack.enter_context(patch.object(source.secrets, "token_hex", lambda _n: "0011223344556677"))
    stack.enter_context(redirect_stdout(writer))
    try:
      receiver = source.CarrotNaviReceiver(**case["config"])
    except (TypeError, ValueError, OverflowError) as error:
      return {"id": case["id"], "init_error": {"type": type(error).__name__, "message": str(error)}}
    for step in case["steps"]:
      try:
        result, error = perform(receiver, step), None
      except (KeyError, TypeError, ValueError, OverflowError, UnicodeError) as rejected:
        result = None
        error = {"type": type(rejected).__name__, "message": str(rejected)}
      rows.append({"result": result, "error": error, "health": receiver.health(), "latest": receiver.latest(),
        "cereal": receiver.cereal_snapshot(), "bootstrap": records(receiver.media_bootstrap()),
        "clock_calls": [clock.wall_calls, clock.mono_calls], "dashboard": dashboard(receiver)})
  writer.flush()
  logs = output.getvalue().decode("utf-8")
  writer.close()
  return {"id": case["id"], "rows": rows, "stdout": logs}


def main() -> None:
  input_path, output_path = (Path(value) for value in sys.argv[1:])
  cases: list[Case] = json.loads(input_path.read_text())
  results = [capture(case) for case in cases]
  output_path.write_text(json.dumps(results, ensure_ascii=True, indent=2) + "\n")
  sources = [Path(source.__file__), Path(sys.modules[build_carrot_navi_payload.__module__].__file__), Path(__file__), Path(__file__).with_name("carrot_navi_cases.py")]
  output_path.with_suffix(".invocation.json").write_text(json.dumps({"argv": sys.argv,
    "python": sys.version, "sources": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources},
    "test_seams": "explicit wall/mono call counters and fixed 8-byte session entropy only", "cases": len(cases)}, indent=2) + "\n")


if __name__ == "__main__":
  main()
