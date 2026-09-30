#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["pyzmq==27.2.0", "sentry-sdk==2.55.0"]
# ///
# How to run: uv run --no-project --python 3.12 rust/tools/check_tombstoned_safety.py PROBE PARAMS_BINDING OUTPUT [RUNNER ARGS...]
"""Harmless temporary-marker proof of the approved native filename-injection correction (#77)."""
import hashlib
import json
from pathlib import Path
import sys

from check_tombstoned_reference import BODY, pair


def main():
  binary, binding, output = map(lambda value: Path(value).resolve(), sys.argv[1:4])
  command = [*sys.argv[4:], str(binary)]
  records = []
  with pair(command, binding, output) as probes:
    for name, marker in [("x$(touch marker_substitution).crash", "marker_substitution"), ("x`touch marker_backtick`.crash", "marker_backtick"), ('x"$(touch marker_quote)".crash', "marker_quote")]:
      probes.write("apport/" + name, BODY, 0o640)
      probes.call("retrace", path="$FIXTURE/apport/" + name)
      source, native = [probes.root / kind for kind in ["source", "native"]]
      assert (source / marker).is_file(), "original source did not reproduce the benign marker"
      assert not (native / marker).exists(), "native adapter executed filename shell text"
      assert BODY in (native / "retrace.input").read_text()
      assert BODY not in (source / "retrace.input").read_text()
      records.append({"filename": name, "source_marker_created": True, "native_marker_created": False,
                      "source_retrace_input": (source / "retrace.input").read_text(),
                      "native_retrace_input": (native / "retrace.input").read_text()})
    probes.write("apport/ordinary space.crash", BODY, 0o640)
    probes.call("retrace", path="$FIXTURE/apport/ordinary space.crash")
    assert probes.paths("retrace.input")[0].read_bytes() == probes.paths("retrace.input")[1].read_bytes()
  result = {"result": "PASS", "approved_difference": "source executes shell substitutions; native passes filename as a positional argument", "source_issue": 77, "cases": records, "ordinary_spaces_preserved": True, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "command": command}
  (output / "safety.json").write_text(json.dumps(result, indent=2) + "\n")
  print(json.dumps(result, indent=2))


if __name__ == "__main__": main()
