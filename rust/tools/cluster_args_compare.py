from __future__ import annotations

import argparse
import ast
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import io
import json
import math
from pathlib import Path
import struct
import subprocess
import sys
from types import SimpleNamespace


def source_namespace(root):
  source = root / "openpilot/selfdrive/carrot/cluster"
  namespace = {"argparse": argparse, "Path": Path, "sys": sys, "OPENPILOT_ROOT": root / "openpilot"}
  files = ["cluster_config.py", "cluster_route_replay.py", "cluster_h264_pipeline.py", "main.py"]
  for name in files:
    tree = ast.parse((source / name).read_text())
    for node in tree.body:
      if isinstance(node, ast.Assign) and all(isinstance(target, ast.Name) and target.id.isupper() for target in node.targets):
        try:
          exec(compile(ast.Module(body=[node], type_ignores=[]), str(source / name), "exec"), namespace)
        except (NameError, AttributeError):
          pass
  tree = ast.parse((source / "main.py").read_text())
  names = {"parse_args", "option_present", "resolve_usb_output_rate", "resolved_usb_display_fps"}
  selected = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
  exec(compile(ast.Module(body=selected, type_ignores=[]), str(source / "main.py"), "exec"), namespace)
  return namespace, files


def bits(value):
  return None if value is None else struct.pack(">d", value).hex()


def normalize(value):
  if isinstance(value, float) and not math.isfinite(value):
    return None
  if isinstance(value, Path):
    return str(value)
  if isinstance(value, dict):
    return {key: normalize(item) for key, item in value.items()}
  if isinstance(value, (list, tuple)):
    return [normalize(item) for item in value]
  return value


def source_case(source, argv):
  sys.argv = ["cluster", *argv]
  stderr = io.StringIO()
  try:
    with redirect_stdout(io.StringIO()), redirect_stderr(stderr):
      args = source["parse_args"]()
  except SystemExit as error:
    if error.code == 0:
      return {"status": "help"}
    return {"status": "error", "error": stderr.getvalue().split("cluster: error: ")[-1].strip()}
  rates = []
  for tici in [False, True]:
    for egpu in [False, True]:
      source["TICI"] = tici

      class Reader:
        def read(self, egpu=egpu):
          return 5.0 if egpu else 10.0

      source["ClusterUsbFpsReader"] = Reader
      try:
        target, fps_source, reader, display, auto = source["resolve_usb_output_rate"](args)
        rates.append(
          {
            "value": {
              "target_fps": target,
              "fps_source": fps_source,
              "fixed_usb_policy": reader is not None,
              "usb_display_fps": display,
              "usb_display_fps_auto": auto,
            },
            "bits": bits(target),
          }
        )
      except (ValueError, OverflowError) as error:
        rates.append({"error": type(error).__name__})
  float_fields = ["fps", "duration", "usb_h264_diagnose_interval", "route_replay_speed", "route_start_time", "profile_interval"]
  return normalize({"status": "ok", "args": vars(args), "float_bits": {key: bits(getattr(args, key)) for key in float_fields}, "rates": rates})


def cases_for(parser):
  cases = [[]]
  for action in parser._actions:
    option = action.option_strings[-1]
    if isinstance(action, argparse._HelpAction):
      cases.extend([[option], ["-h"], ["--hel"]])
      continue
    if action.nargs == 0:
      cases.extend([[option], [option, option], [option + "=true"]])
      continue
    if action.choices:
      values = [str(value) for value in action.choices] + ["invalid", ""]
    elif action.type is int:
      values = ["0", "1", "-1", "95", "100", "255", "65535", "65536", "1_0", "１２", "bad", ""]
    elif action.type is float:
      values = ["0", "1", "-1", "2.5", "1_0", "１２.５", "NaN", "-NaN", "inf", "-inf", "1e309", "bad", ""]
    else:
      values = ["custom", "", " ", "x=y", "--value"]
    cases.extend([[option + "=" + value] for value in values])
    cases.append([option])
  cases.extend(
    [
      ["--input=live", "--fps=90", "--usb-display-fps=60"],
      ["--input=live", "--output=window", "--fps=90"],
      ["--usb-codec=h264"],
      ["--usb-codec=h264", "--fps=0"],
      ["--usb-codec=h264", "--fps=2.5"],
      ["--usb-codec=h264", "--fps=inf"],
      ["--usb-codec=h264", "--fps=nan"],
      ["--usb-codec=h264", "--usb-h264-fps=400"],
      ["--usb-codec=h264", "--usb-display-fps=0"],
      ["--usb-code=h264"],
      ["--usb-h264-back=ffmpeg"],
      ["--usb-h264-ffmpeg-e=h264_v4l2m2m"],
      ["--metric", "--imperial"],
      ["--imperial", "--metric"],
      ["--usb-async", "--usb-codec=h264"],
      ["--usb-h264-no-ack", "--usb-h264-wait-ack"],
      ["--usb-h264-no-ack", "--usb-h264-soft-ack"],
      ["--usb-h264-wait-ack", "--usb-h264-soft-ack"],
      ["--usb-h264-test-pattern", "--usb-h264-test-pattern-nv12"],
      ["--usb-h264-test-pattern-nv12", "--usb-h264-backend=ffmpeg"],
      ["--navi-publish-cereal", "--input=navi"],
      ["--navi-publish-cereal", "--navi-overlay"],
      ["--input=route", "--route-replay-speed=0"],
      ["--input=route", "--route-replay-speed=nan"],
      ["--live-no-can", "--live-include-can"],
      ["--fps", "1_0"],
      ["--fps", "-1e2"],
      ["--fps", "-.5"],
      ["--fps", "1", "--fps", "2"],
      ["--fps=1", "--fps=0"],
      ["--fps", "-１２"],
      ["--fps", "-１２.５"],
      ["--usb"],
      ["--h"],
      ["--unknown"],
      ["--unknown", "--help"],
      ["--unknown", "--fps=-1"],
      ["--"],
      ["--", "--help"],
      ["--output", "--help"],
      ["-hfoo"],
      ["--metric=true"],
      ["--route=./a//b/."],
      ["--route=//a///b/../"],
      ["--route=///"],
      ["--route=."],
      ["--route=.././a/"],
      ["--duration", "-１２.５"],
    ]
  )
  return cases


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--root", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  source, files = source_namespace(args.root)
  original_parser = argparse.ArgumentParser
  captured = []

  class CaptureParser(original_parser):
    def __init__(self, *values, **kwargs):
      super().__init__(*values, **kwargs)
      captured.append(self)

  source["argparse"] = SimpleNamespace(ArgumentParser=CaptureParser, Namespace=argparse.Namespace, SUPPRESS=argparse.SUPPRESS)
  source_case(source, [])
  source["argparse"] = argparse
  cases = cases_for(captured[0])
  expected = [source_case(source, case) for case in cases]
  request = {"root": str(args.root / "openpilot"), "cases": cases}
  (args.output / "request.json").write_text(json.dumps(request, indent=2, ensure_ascii=False))
  (args.output / "source.json").write_text(json.dumps(expected, indent=2, ensure_ascii=False))
  before = hashlib.sha256(args.native.read_bytes()).hexdigest()
  result = subprocess.run([str(args.native)], input=json.dumps(request), text=True, capture_output=True, check=True)
  (args.output / "native.json").write_text(result.stdout)
  actual = json.loads(result.stdout)
  mismatches = []
  for index, (left, right) in enumerate(zip(expected, actual, strict=True)):
    if left["status"] != right["status"]:
      mismatches.append({"index": index, "argv": cases[index], "source": left, "native": right})
      continue
    if left["status"] == "error":
      if left["error"].startswith(("--", "USB drain")) and left["error"] != right["error"]:
        mismatches.append({"index": index, "argv": cases[index], "source": left, "native": right})
      continue
    if left["status"] == "ok":
      for expected_rate, actual_rate in zip(left["rates"], right["rates"], strict=True):
        if "error" in expected_rate and "error" in actual_rate:
          actual_rate["error"] = expected_rate["error"]
      if left != right:
        mismatches.append({"index": index, "argv": cases[index], "source": left, "native": right})
  after = hashlib.sha256(args.native.read_bytes()).hexdigest()
  assert before == after
  receipt = {
    "cases": len(cases),
    "mismatches": mismatches,
    "provider_sha256": before,
    "python": sys.version,
    "source_files": {name: hashlib.sha256((args.root / "openpilot/selfdrive/carrot/cluster" / name).read_bytes()).hexdigest() for name in files},
    "worktree_sources_at_comparison": {
      str(path.relative_to(args.root)): hashlib.sha256(path.read_bytes()).hexdigest()
      for path in sorted((args.root / "rust/crates/cluster/src/runtime_args").glob("*.rs"))
    },
  }
  (args.output / "result.json").write_text(json.dumps(receipt, indent=2, ensure_ascii=False))
  print(json.dumps({"cases": len(cases), "mismatches": len(mismatches), "examples": [item["argv"] for item in mismatches[:10]]}))
  if mismatches:
    raise SystemExit(1)


if __name__ == "__main__":
  main()
