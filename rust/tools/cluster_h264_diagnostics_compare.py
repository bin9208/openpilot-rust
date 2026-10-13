from __future__ import annotations

import argparse
import ast
import contextlib
import hashlib
import io
import json
from pathlib import Path
import random
import subprocess
import threading
from types import SimpleNamespace


def original(path: Path, clock: SimpleNamespace):
  tree = ast.parse(path.read_text())
  methods = {"_reset_h264_diag_window", "_diagnostics_enabled", "_record_h264_unit", "_record_h264_queue_depth",
             "_record_h264_drop", "_record_h264_send", "_maybe_log_h264_diag"}
  functions = {"_h264_diag_stats", "_h264_diag_next_start_code"}
  nodes = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in functions]
  source = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "H264UsbPipeline")
  source.body = [node for node in source.body if isinstance(node, ast.FunctionDef) and node.name in methods]
  nodes.append(source)
  namespace = {"time": SimpleNamespace(perf_counter=lambda: clock.now)}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), "exec"), namespace)
  return namespace["H264UsbPipeline"], namespace["_h264_diag_stats"]


def run_source(source, scenario, clock):
  instance = source()
  instance.diagnose_interval_s = scenario["interval"]
  instance.backend_name = scenario["backend"]
  instance._condition = threading.Lock()
  instance._debug_dropped_access_units = 0
  instance._debug_dropped_chunks = 0
  instance._reset_h264_diag_window(scenario["started"])
  results = []
  for operation in scenario["operations"]:
    clock.now = operation["at"]
    output = io.StringIO()
    with contextlib.redirect_stdout(output):
      match operation["kind"]:
        case "unit":
          instance._record_h264_unit(operation["source"], bytes(operation["packet"]), [bytes(size) for size in operation["chunks"]],
                                     keyframe=operation["keyframe"])
        case "queue":
          instance._record_h264_queue_depth(operation["depth"])
        case "drop":
          instance._record_h264_drop(operation["units"], operation["chunks"])
        case "send":
          instance._record_h264_send(operation["source"], operation["bytes"], operation["ms"])
        case "report":
          instance._maybe_log_h264_diag(force=operation["force"])
        case _:
          raise AssertionError(operation)
    results.append(output.getvalue().removesuffix("\n") or None)
  return results


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--native", type=Path, required=True)
  cli.add_argument("--output", type=Path, required=True)
  args = cli.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  path = args.root / "openpilot/selfdrive/carrot/cluster/cluster_h264_pipeline.py"
  clock = SimpleNamespace(now=0.0)
  source, stats = original(path, clock)
  randomizer = random.Random(269253)
  packets = [[], [0, 0, 1], [0, 0, 0, 1], [9, 8, 7], [0, 0, 1, 0x65, 0], [0, 0, 0, 1, 0x67, 8, 0, 0, 1, 0x65, 6, 0]]
  for _ in range(1000):
    packet = bytearray(randomizer.randbytes(randomizer.randrange(9)))
    for _ in range(randomizer.randrange(10)):
      packet += bytes([0] * randomizer.randrange(2, 8) + [1])
      packet += randomizer.randbytes(randomizer.randrange(64))
      packet += bytes(randomizer.randrange(5))
    packets.append(list(packet))
  scenarios = []
  for interval in [-1.0, 0.0, 0.0001, 0.001, 0.5, 2.0]:
    for backend in ["native", "ffmpeg", ""]:
      operations = [{"kind": "report", "at": 1.0, "force": False}]
      at = 1.0
      for index in range(40):
        at += randomizer.choice([-0.125, 0.0, 0.001, 0.125, 0.5])
        operations += [
          {"kind": "unit", "at": at, "source": randomizer.choice(["native", "ffmpeg", ""]),
           "packet": packets[randomizer.randrange(len(packets))], "chunks": [3] * (index % 7), "keyframe": index % 9 == 0},
          {"kind": "queue", "at": at, "depth": index % 11},
          {"kind": "drop", "at": at, "units": index % 3, "chunks": index % 4},
          {"kind": "send", "at": at, "source": randomizer.choice(["native", "ffmpeg", ""]),
           "bytes": index * 1023, "ms": randomizer.choice([0.0, 0.125, 1.005, 2.675, -0.5, 30.0])},
          {"kind": "report", "at": at, "force": index % 4 == 0},
        ]
      operations += [{"kind": "report", "at": at + 10, "force": True}, {"kind": "report", "at": at + 11, "force": True}]
      scenarios.append({"interval": interval, "started": 1.0, "backend": backend, "operations": operations})
  request = {"packets": packets, "scenarios": scenarios}
  expected = {"packets": [list(stats(bytes(packet))) for packet in packets],
              "scenarios": [run_source(source, scenario, clock) for scenario in scenarios]}
  (args.output / "request.json").write_text(json.dumps(request))
  (args.output / "source.json").write_text(json.dumps(expected))
  result = subprocess.run([str(args.native)], input=json.dumps(request), text=True, capture_output=True, timeout=30, check=True)
  (args.output / "native.json").write_text(result.stdout)
  actual = json.loads(result.stdout)
  failures = [{"section": section, "index": index, "source": left, "native": right}
              for section in expected for index, (left, right) in enumerate(zip(expected[section], actual[section], strict=True)) if left != right]
  report = {"packets": len(packets), "scenarios": len(scenarios), "operations": sum(len(s["operations"]) for s in scenarios),
            "failures": failures, "source_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest()}
  (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))
  if failures:
    raise AssertionError(failures)


if __name__ == "__main__":
  main()
