from __future__ import annotations

from dataclasses import dataclass
import json


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  method: str
  path: str
  body: bytes = b""
  raw: bytes | None = None


def cases() -> list[Case]:
  config = {"width": 100, "height": 80, "poly_left": [[0, 0], [20.5, 20.5], [0, 79]], "poly_right": []}
  result = [Case("index", "GET", "/"), Case("initial-status", "GET", "/api/status"),
    Case("initial-config", "GET", "/api/config"), Case("unknown", "GET", "/missing"),
    Case("invalid-stream", "GET", "/api/snapshot?stream=driver"), Case("empty-post", "POST", "/api/settings"),
    Case("settings-array", "POST", "/api/settings", b"[]"), Case("settings-null", "POST", "/api/settings", b"null"),
    Case("config-array", "POST", "/api/config", b"[]"), Case("config-empty", "POST", "/api/config", b"{}"),
    Case("config-valid", "POST", "/api/config", json.dumps(config).encode()),
    Case("config-saved", "GET", "/api/config"), Case("delete-other", "DELETE", "/missing"),
    Case("config-delete", "DELETE", "/api/config"), Case("config-delete-again", "DELETE", "/api/config"),
    Case("config-reset", "GET", "/api/config")]
  for name, value in [("valid", {"threshold": .455, "smoothingSeconds": .2505, "baseIntervalSeconds": .35, "laneThreshold": .355, "laneIntervalSeconds": .775}),
      ("strings", {"threshold": "０.５", "baseIntervalSeconds": "0.20"}), ("nan", {"threshold": float("nan")}),
      ("infinity", {"threshold": float("inf")}), ("low", {"laneThreshold": .049}), ("high", {"laneIntervalSeconds": 2.001}),
      ("unknown-surrogate", {"unused": "\ud800"}), ("huge-integer", {"threshold": 10**400})]:
    result.append(Case(f"settings-{name}", "POST", "/api/settings", json.dumps(value).encode()))
  for name, value in [("infinity-dimension", config | {"width": float("inf")}),
      ("nan-dimension", config | {"width": float("nan")}), ("huge-dimension", config | {"width": 10**400}),
      ("infinity-point", config | {"poly_left": [[0, 0], [float("inf"), 1], [0, 70]]}),
      ("surrogate-number", config | {"poly_left": [[0, 0], ["\ud800", 1], [0, 70]]})]:
    result.append(Case(f"config-{name}", "POST", "/api/config", json.dumps(value).encode()))
  for encoding in ["utf-8-sig", "utf-16", "utf-16-le", "utf-16-be", "utf-32", "utf-32-le", "utf-32-be"]:
    result.append(Case(encoding, "POST", "/api/settings", '{"threshold": 0.50}'.encode(encoding)))
  for index, body in enumerate([b"{", b"[]x", b'{"threshold":}', b'{"threshold": .5}', b'{"threshold":0.5,}', b'"foo', b"9" * 4301]):
    result.append(Case(f"malformed-{index}", "POST", "/api/settings", body))
  result.extend([Case("snapshot-road-timeout", "GET", "/api/snapshot?stream=road"),
    Case("snapshot-wide-timeout", "GET", "/api/snapshot"), Case("final-status", "GET", "/api/status")])
  for index, target in enumerate(["/x/../api/config", "/api/./config", "/api/config;params", "/api;params/config",
      "//api/config", "///api/config", "/api/%63onfig", "/api/config#fragment", "http://example.invalid/api/config;params"]):
    result.append(Case(f"raw-path-{index}", "GET", target,
                       raw=f"GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n".encode()))
  for index, length in enumerate(["invalid", "+2", "-2", "2_0", "0x2", "2.0", "9" * 4301, "x" * 205, "9" * 4301 + "x", "'quote"]):
    result.append(Case(f"raw-length-{index}", "POST", "/api/settings",
      raw=f"POST /api/settings HTTP/1.1\r\nHost: localhost\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n".encode() + b"{}" + b" " * 18))
  return result
