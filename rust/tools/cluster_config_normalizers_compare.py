from __future__ import annotations

import argparse
import ast
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--native", type=Path, required=True)
  cli.add_argument("--output", type=Path, required=True)
  args = cli.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  sys.path.insert(0, str(args.root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_config as source

  code = Path(source.__file__)
  functions = [source.normalize_cluster_screen_mode, source.normalize_cluster_panel_layout]
  names = {function.__name__ for function in functions}
  nodes = [node for node in ast.parse(code.read_text()).body if isinstance(node, ast.FunctionDef) and node.name in names]
  aliases = sorted({key.value for node in nodes for item in ast.walk(node) if isinstance(item, ast.Dict)
                    for key in item.keys if isinstance(key, ast.Constant) and isinstance(key.value, str)})
  values = [form for alias in aliases for form in [alias, alias.upper(), "  " + alias + "\t", "\x1c" + alias + "\x85"]]
  values += [str(value) for value in range(-12, 15)]
  values += ["", "unknown", "null", "None", "True", "False", "+", "-", "1.0", "NaN", "Infinity", "0x1", "1_0", "0_1", "_1", "1_",
             "١", "-١", "+００１", "\x00", "0" * 4300 + "1", "0" * 4299 + "1"]
  spaces = [chr(point) for point in range(0x110000) if chr(point).isspace()]
  values += [space + value + space for space in spaces for value in ["1", "-1", "graph", "camera-right"]]
  expected = [[function(value) for function in functions] for value in values]
  (args.output / "request.json").write_text(json.dumps(values))
  (args.output / "source.json").write_text(json.dumps(expected))
  run = subprocess.run([str(args.native)], input=json.dumps(values), capture_output=True, text=True, timeout=15, check=True)
  (args.output / "native.json").write_text(run.stdout)
  actual = json.loads(run.stdout)
  if actual != expected:
    mismatches = [{"input": value, "source": left, "native": right}
                  for value, left, right in zip(values, expected, actual, strict=True) if left != right]
    raise AssertionError(mismatches)
  report = {"inputs": len(values), "aliases": len(aliases), "whitespace_characters": len(spaces),
            "source_sha256": hashlib.sha256(code.read_bytes()).hexdigest(),
            "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(), "scope": "CLI string overrides for screen and panel; original normalizers"}
  (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
