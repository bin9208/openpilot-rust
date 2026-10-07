"""Lossless source-fixture values for the independent radar controller oracle."""

from collections import deque
import dataclasses
import math


def number(value):
  value = float(value)
  if math.isnan(value):
    return "NaN"
  if math.isinf(value):
    return "Infinity" if value > 0 else "-Infinity"
  return value


def plain(value):
  match value:
    case None | bool() | int() | str():
      return value
    case float():
      return number(value)
    case list() | tuple() | deque():
      return [plain(item) for item in value]
    case set() | frozenset():
      return [plain(item) for item in sorted(value)]
    case dict():
      if all(isinstance(key, str) for key in value):
        return {key: plain(item) for key, item in value.items()}
      return {"entries": [[plain(key), plain(item)] for key, item in value.items()]}
    case _:
      if dataclasses.is_dataclass(value):
        return {field.name: plain(getattr(value, field.name)) for field in dataclasses.fields(value)}
      return {key: plain(item) for key, item in vars(value).items() if not callable(item)}


def model(value):
  position = getattr(value, "position", None)
  velocity = getattr(value, "velocity", None)
  return {
    "position": {name: [number(item) for item in getattr(position, name, ())] for name in ("x", "y")},
    "velocity": [number(item) for item in getattr(velocity, "x", ())],
    "lane_probabilities": [number(item) for item in getattr(value, "laneLineProbs", ())],
    "leads": [
      {"probability": number(getattr(lead, "prob", 0.0)),
       **{name: [number(item) for item in getattr(lead, name)] for name in ("x", "y", "v", "a", "xStd", "yStd", "vStd")
          if hasattr(lead, name)}}
      for lead in getattr(value, "leadsV3", ())
    ],
  }


def point(value):
  def field(snake, camel, fallback=0.0):
    item = getattr(value, snake, getattr(value, camel, fallback))
    return fallback if item is None else item

  return {
    "track_id": int(field("track_id", "trackId", -1)),
    "source": str(field("source", "radarSource", "frontRadar")).rsplit(".", 1)[-1],
    "measured": bool(getattr(value, "measured", False)),
    "radar_track_state": int(field("radar_track_state", "trackState", 0)),
    **{snake: number(field(snake, camel)) for snake, camel in (
      ("d_rel", "dRel"), ("y_rel", "yRel"), ("v_rel", "vRel"), ("a_rel", "aRel"),
      ("yv_rel", "yvRel"), ("a_lead", "aLead"), ("j_lead", "jLead"),
    )},
  }
