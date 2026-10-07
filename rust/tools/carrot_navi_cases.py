#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# uv run rust/tools/carrot_navi_cases.py OUTPUT.json
# ──────────────────
from __future__ import annotations

import copy
import json
from pathlib import Path
import struct
import sys
from typing import TypedDict

type Json = None | bool | int | float | str | list[Json] | dict[str, Json]

JSON_NAMES = ("vehicle", "guidance_current", "guidance_next", "lane_current", "lane_ahead", "speed",
  "traffic_signal", "crossroad", "route", "navigation_status", "app_status", "camera_state", "composition_state")
IMAGE_NAMES = ("tbt_current_compact", "tbt_current_full", "tbt_next", "traffic_signal", "lane_top", "lane_bottom",
  "safety_primary", "safety_secondary", "safety_section", "crossroad_minimized", "crossroad_expanded",
  "center_tbt_icon", "center_tbt_text", "center_tbt_fee")
CATALOG = [("json", name) for name in JSON_NAMES] + [("image", name) for name in IMAGE_NAMES] + [("render", "map_main")]
SESSION = "0011223344556677"
MAP = {"map_theme": "auto", "map_type": "normal", "map_hz": 10, "map_bitrate_kbps": 3000, "screen_center_y_ratio": 0.8}


class Case(TypedDict):
  id: str
  config: dict[str, Json]
  steps: list[dict[str, Json]]


def query() -> dict[str, Json]:
  return {"type": "requirements_query", "protocol_version": 2, "catalog_revision": 1,
    "streams": [{"kind": kind, "name": name, "schema_version": 1} for kind, name in CATALOG]}


def envelope(name: str, sequence: int = 1) -> dict[str, Json]:
  handle = JSON_NAMES.index(name) + 1
  return {"type": "item_update", "protocol_version": 2, "session_id": SESSION, "kind": "json", "name": name,
    "stream_handle": handle, "manifest_revision": 1, "schema_version": 1, "sequence": sequence,
    "source_timestamp_ms": 1000, "sent_at_ms": 1001, "present": True, "value": [] if name == "lane_ahead" else {}}


def case(identifier: str, steps: list[dict[str, Json]], config: dict[str, Json] | None = None) -> Case:
  return {"id": identifier, "config": config or {}, "steps": steps}


def negotiate() -> dict[str, Json]:
  return {"op": "negotiate", "value": query()}


def binary_metadata(sequence: int, message_type: int = 3, flags: int = 0) -> dict[str, Json]:
  return {"stream_handle": 28, "manifest_revision": 1, "sequence": sequence, "source_timestamp_ms": 1000,
    "message_type": message_type, "format_or_reason": 3, "flags": flags, "width": 960, "height": 540}


def packet(message_type: int = 1, format_code: int = 1, payload: bytes = b"\x89PNG\r\n\x1a\ncontent",
           width: int = 32, height: int = 24, handle: int = 14, revision: int = 1) -> bytes:
  return struct.pack(">4sBBBBIIQQIHH", b"CNV2", 2, message_type, format_code, 0, handle, revision,
    1, 1000, len(payload), width, height) + payload


def build() -> list[Case]:
  cases = [case("initial", [{"op": "payload"}, {"op": "manifest"}])]
  for key, choices in {
    "map_theme": [" DARK ", "light", "neon", None], "map_type": [" SATELLITE ", "terrain"],
    "map_hz": [1, 60, 0, 61, True, "20", float("inf")], "map_bitrate_kbps": [1, 12000, 0, 12001],
    "screen_center_y_ratio": [0.5, 0.9, 0.49, 0.91, "0.68", float("nan")],
  }.items():
    for index, value in enumerate(choices):
      config = {key: value}
      cases.append(case(f"init-{key}-{index}", [negotiate()], config))
      cases.append(case(f"manifest-{key}-{index}", [{"op": "manifest", "config": config}]))
  for key, choices in {
    "type": [None, "wrong"], "protocol_version": [True, "2", 2.0], "catalog_revision": [True, "1", 1.0, 2],
    "streams": [None, [], query()["streams"][:-1], query()["streams"] + [query()["streams"][0]]],
  }.items():
    for index, value in enumerate(choices):
      offered = query()
      offered[key] = value
      cases.append(case(f"query-{key}-{index}", [{"op": "negotiate", "value": offered}]))
  for index, field_value in enumerate([True, 1.0, "1", 2, None]):
    offered = query()
    offered["streams"][0]["schema_version"] = field_value
    cases.append(case(f"query-schema-{index}", [{"op": "negotiate", "value": offered}]))
  duplicate = query()
  duplicate["streams"][1] = duplicate["streams"][0]
  cases.append(case("query-duplicate", [{"op": "negotiate", "value": duplicate}]))
  for name in JSON_NAMES:
    value = envelope(name)
    value["value"] = [{"lane": 2}] if name == "lane_ahead" else {"future": {"text": "한글", "items": [1, None]}, "foreground": False}
    clear = envelope(name, 2) | {"present": False, "value": None, "reason": "source_absent"}
    cases.append(case(f"item-{name}", [negotiate(), {"op": "connect"}, {"op": "json", "name": name, "value": value},
      {"op": "payload"}, {"op": "json", "name": name, "value": clear}, {"op": "payload"}, {"op": "disconnect"}]))
  for key, choices in {
    "protocol_version": [True, "2", 2.0], "session_id": ["old"], "kind": ["image"], "name": ["speed"],
    "stream_handle": [2, "1", True], "manifest_revision": [2, "1", True], "schema_version": [2, "1", True],
    "sequence": [True, -1, " 1_000 ", 1.9, 2**80, float("nan"), float("inf")],
    "source_timestamp_ms": [None, True, -1, "1"], "sent_at_ms": [None, True, -1, "1"],
    "present": [1, "true", None], "value": [[], None, 123],
  }.items():
    for index, value in enumerate(choices):
      item = envelope("vehicle") | {key: value}
      cases.append(case(f"json-{key}-{index}", [negotiate(), {"op": "json", "value": item}]))
  for key in ("sequence", "source_timestamp_ms", "sent_at_ms", "present", "value"):
    item = envelope("vehicle")
    del item[key]
    cases.append(case(f"json-missing-{key}", [negotiate(), {"op": "json", "value": item}]))
  cases.append(case("session-state", [negotiate(), {"op": "connect"}, {"op": "json", "value": envelope("vehicle")},
    {"op": "json", "value": envelope("vehicle")}, {"op": "fail", "value": "failure", "peer": "-"},
    {"op": "publish", "value": "p" * 300}, {"op": "publish"}, {"op": "disconnect"}, {"op": "disconnect"},
    negotiate(), {"op": "json", "value": envelope("vehicle")}, {"op": "config", "config": MAP | {"map_theme": "dark"}},
    {"op": "config", "config": MAP | {"map_theme": "dark"}}, negotiate()]))
  media_steps = [negotiate()]
  for sequence, message_type, flags in [(1, 2, 0), (2, 3, 1), (3, 3, 0), (4, 2, 0), (5, 3, 1), (6, 4, 0)]:
    metadata = binary_metadata(sequence, message_type, flags)
    if message_type == 4:
      metadata.update({"format_or_reason": 1, "width": 0, "height": 0})
    media_steps.append({"op": "binary", "kind": "render", "name": "map_main", "metadata": metadata,
      "packet": "" if message_type == 4 else "000000016766"})
    media_steps.append({"op": "bootstrap"})
  media_steps.extend([{"op": "drain"}, {"op": "drain"}, negotiate()])
  cases.append(case("media-lifecycle", media_steps))
  cases.append(case("controls-bounded", [negotiate(), *[{"op": "control", "value": {"protocol_version": 2,
    "type": "protocol_error" if index == 270 else "unknown", "message": str(index)}} for index in range(271)]]))
  for name in ("vehicle", "missing", "lane_top"):
    cases.append(case("stream-" + name, [negotiate(), {"op": "stream", "kind": "image" if name == "lane_top" else "json", "name": name}]))
  valid_packets = [packet(), packet(format_code=2, payload=b"\xff\xd8content\xff\xd9"),
    packet(2, 3, b"\x00\x00\x01config"), packet(3, 3, b"\x00\x00\x00\x01frame"),
    *[packet(4, reason, b"", 0, 0) for reason in range(1, 6)]]
  invalid_packets = [b"", packet()[:39], b"xxxx" + packet()[4:], packet()[:4] + b"\x01" + packet()[5:],
    packet(handle=0), packet(revision=0), packet(width=0), packet(height=0), packet(payload=b"bad"),
    packet(format_code=4), packet(format_code=2, payload=b"\xff\xd8"), packet(3, 3, b"bad"),
    packet(2, 1), packet(4, 0, b"", 0, 0), packet(4, 1, b"x", 0, 0), packet(4, 1, b"", 1, 0), packet(9)]
  for index, value in enumerate(valid_packets + invalid_packets):
    cases.append(case(f"binary-parse-{index}", [{"op": "parse_binary", "packet": value.hex()}]))
  for index, raw in enumerate(['{}', '[]', '1', 'null', '{"value":NaN,"huge":1208925819614629174706176}',
    '{"text":"\\ud800"}', '{"key":1,"key":2}', '{', '{"x":', '\ufeff{}']):
    cases.append(case(f"json-parse-{index}", [{"op": "parse_json", "value": raw}]))
  cases.append(case("map-policy", [{"op": "hz", "value": value} for value in [None, True, 0, 1, 2, 3, 4, "2", 2.9]] +
    [{"op": "bitrate", "value": value} for value in [[960, 540, 5], [960, 540, 20], [480, 270, 30], [1920, 1080, 30], [-1, 0, 0], [960, 540, 17]]]))
  for missing in ("stream_handle", "manifest_revision"):
    item = envelope("vehicle")
    del item[missing]
    cases.append(case("json-missing-" + missing, [negotiate(), {"op": "json", "value": item}]))
  for name in ("vehicle", "guidance_current"):
    item = envelope(name) | {"value": {"text": "\ud800"}}
    clear = envelope(name, 2) | {"present": False, "value": None, "reason": "\ud800"}
    cases.append(case("unicode-state-" + name, [negotiate(), {"op": "json", "name": name, "value": item},
      {"op": "json", "name": name, "value": item | {"sequence": 2}}, {"op": "json", "name": name, "value": clear | {"sequence": 3}}]))
  semantic = {
    "vehicle": {"lat": 37.123, "lon": 127.789, "heading_deg": 390, "speed_kph": 321, "road_name": "한글" * 100, "virtual_gps": True},
    "guidance_current": {"distance_m": 320, "time_sec": 35, "turn_type": 12, "road_name": "road", "main_text": "left", "near_direction": "near",
      "mid_direction": "mid", "far_direction": "far", "point": {"lat": 37.9, "lon": -999}},
    "guidance_next": {"distance_m": 950, "time_sec": 95, "turn_type": 6, "road_name": "next", "main_text": "right", "point": {"lat": None, "lon": 5}},
    "lane_current": {"count": 20, "distance_m": 3_000_000, "visible": None, "lane_play": [], "current_lane": 1.9, "turn_code": 8,
      "turn_info": [1, -99999, "2", True, "١٢", None] * 4, "etc_info": [3, 5], "available": [1, 0, 1], "guide_line_color": 45000, "road_category": -50000, "voice_code": 7},
    "lane_ahead": [{"count": index, "visible": True, "distance_m": index * 20} for index in range(18)],
    "speed": {"current_kph": -0.0, "road_limit_kph": 50,
      "sdi": {"type": 22, "distance_m": 93, "speed_limit_kph": 30, "section_type": 7, "block_type": 2, "block_speed_kph": 40, "block_distance_m": 80},
      "sdi_secondary": {"type": 1, "distance_m": 420, "speed_limit_kph": 50, "section_type": 3, "block_type": 4, "block_speed_kph": 60, "block_distance_m": 500},
      "section": {"active": True, "speed_limit_kph": 30, "average_kph": 61, "overall_average_kph": 55, "remaining_distance_m": 999,
        "remaining_time_sec": 200, "progress": 2, "suspended": None, "off_route": True}},
    "traffic_signal": {"visible": True, "distance_m": 42, "source": "source", "lights": {"red": {"on": True, "remain_sec": 1000},
      "left": {"on": False, "remain_sec": 0}, "green": {"on": "yes", "remain_sec": 2.5}, "right": {}, "uturn": {"on": None, "remain_sec": "7"}}, "ui_counter": {"remain_sec": 0}},
    "crossroad": {"visible": True, "distance_m": 73, "image_code": 2**33, "image_url": "한글" * 400},
    "route": {"remain_distance_m": 1234, "remain_time_sec": 123, "moved_distance_m": 456, "moved_time_sec": 45, "total_distance_m": 1690,
      "polyline": [{"lat": 37.1 + index / 100000, "lon": 127.1 + index / 100000} for index in range(300)]},
    "navigation_status": {"mode": "navigation" * 50, "guidance_active": True, "off_route": [], "route_present": {"yes": True}},
  }
  snapshot = {"generation": 2**80 + 1, "session_id": "s" * 50, "connected": True,
    "items": {name: {"present": True, "sequence": index + 1, "source_timestamp_ms": 1000 + index,
      "received_mono_ns": 2000 + index, "value": value} for index, (name, value) in enumerate(semantic.items())}}
  cases.append(case("payload-all-fields", [{"op": "payload", "value": snapshot}]))
  for index, road_limit in enumerate([None, 60, 320, 520, 1020, 300, 200, "50", True, float("nan"), float("inf")]):
    data = copy.deepcopy(snapshot)
    data["items"]["speed"]["value"]["road_limit_kph"] = road_limit
    cases.append(case(f"payload-road-limit-{index}", [{"op": "payload", "value": data}]))
  return copy.deepcopy(cases)


def main() -> None:
  Path(sys.argv[1]).write_text(json.dumps(build(), ensure_ascii=True, indent=2) + "\n")


if __name__ == "__main__":
  main()
