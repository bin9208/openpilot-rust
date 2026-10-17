"""Quoting helper retained for the camera state comparator."""

def quoted(value: str) -> str:
  return '"' + value.replace('\\', '\\\\').replace('"', '\\"') + '"'
