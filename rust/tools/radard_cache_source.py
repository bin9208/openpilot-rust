from collections import OrderedDict
import dataclasses
import hashlib
import json
import struct

from openpilot.selfdrive.carrot.radar_motion import predictor


def fingerprint(value):
  def bits(item):
    match item:
      case float() | int():
        return f"{struct.unpack('<Q', struct.pack('<d', float(item)))[0]:016x}"
      case list() | tuple():
        return [bits(child) for child in item]
      case dict():
        return {key: bits(child) for key, child in item.items()}
      case _:
        return item
  return hashlib.sha256(json.dumps(bits(value), sort_keys=True, separators=(",", ":")).encode()).hexdigest()


class CacheObserver:
  def __init__(self):
    self.geometry = OrderedDict()
    self.projection = OrderedDict()
    self.archive = {"geometry": [], "projection": []}
    self.indices = {"geometry": {}, "projection": {}}
    geometry, projection = predictor._path_geometry, predictor._project_to_model_path_cached
    assert geometry.cache_info().currsize == 0 and projection.cache_info().currsize == 0

    def record_geometry(path):
      result = geometry(path)
      if path in self.geometry:
        self.geometry.move_to_end(path)
      else:
        value = {"path": path, "value": {"points": result[0], "segments": result[1]}}
        self.geometry[path] = {"value": value, "fingerprint": fingerprint(value)}
        if len(self.geometry) > 8:
          self.geometry.popitem(last=False)
      return result

    def record_projection(path, x, y):
      result = projection(path, x, y)
      key = (path, x, y)
      if key in self.projection:
        self.projection.move_to_end(key)
      else:
        value = {"path": path, "x": x, "y": y, "value": dataclasses.asdict(result)}
        self.projection[key] = {"value": value, "fingerprint": fingerprint(value)}
        if len(self.projection) > 256:
          self.projection.popitem(last=False)
      return result

    predictor._path_geometry = record_geometry
    predictor._project_to_model_path_cached = record_projection

  def cursor(self):
    cursor = {}
    for name, entries in (("geometry", self.geometry), ("projection", self.projection)):
      values = []
      for entry in entries.values():
        identity = id(entry)
        if identity not in self.indices[name]:
          self.indices[name][identity] = len(self.archive[name])
          self.archive[name].append(entry)
        values.append(self.indices[name][identity])
      cursor[name] = values
    return cursor

  def fingerprints(self):
    return {"geometry": [entry["fingerprint"] for entry in self.geometry.values()],
            "projection": [entry["fingerprint"] for entry in self.projection.values()]}

  def table(self):
    return {name: [entry["value"] for entry in entries] for name, entries in self.archive.items()}
