"""Compare complete radar snapshots, retaining scalar types and float bit patterns."""
import struct


def differences(expected, actual, path="", records=None):
  if records is None:
    records = []
  if isinstance(expected, (dict, list)) and not isinstance(actual, type(expected)):
    records.append({"path": path, "expected_type": type(expected).__name__, "actual_type": type(actual).__name__})
  elif isinstance(expected, dict):
    if expected.keys() != actual.keys():
      records.append({"path": path, "expected_keys": sorted(expected), "actual_keys": sorted(actual)})
    else:
      for key, value in expected.items():
        differences(value, actual[key], path + "." + key, records)
  elif isinstance(expected, list):
    if len(expected) != len(actual):
      records.append({"path": path, "expected_length": len(expected), "actual_length": len(actual)})
    else:
      for index, (value, other) in enumerate(zip(expected, actual, strict=True)):
        differences(value, other, f"{path}[{index}]", records)
  elif isinstance(expected, float):
    if not isinstance(actual, (float, int)) or struct.pack("<d", expected) != struct.pack("<d", float(actual)):
      records.append({"path": path, "expected": expected, "actual": actual, "expected_hex": expected.hex(),
                      "actual_hex": float(actual).hex() if isinstance(actual, (float, int)) else None})
  elif expected != actual:
    records.append({"path": path, "expected": expected, "actual": actual})
  return records
