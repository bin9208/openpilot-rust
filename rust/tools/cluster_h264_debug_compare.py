from __future__ import annotations

import argparse
import ast
import hashlib
import json
from pathlib import Path
import random
import subprocess

from cluster_sps_compare import original, synthetic


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--native", type=Path, required=True)
  cli.add_argument("--output", type=Path, required=True)
  args = cli.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  path = args.root / "openpilot/selfdrive/carrot/cluster/cluster_h264_pipeline.py"
  scope = original(args.root)
  nodes = []
  for node in ast.parse(path.read_text()).body:
    if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id.startswith(("H264_NAL_NAMES", "V4L2_")) for target in node.targets):
      nodes.append(node)
    if isinstance(node, ast.FunctionDef) and node.name == "_bytes_head":
      nodes.append(node)
    if isinstance(node, ast.ClassDef) and node.name == "H264UsbPipeline":
      method = next(item for item in node.body if isinstance(item, ast.FunctionDef) and item.name == "_debug_should_log")
      method.decorator_list = []
      nodes.append(method)
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), "exec"), scope)
  packets = []
  for profile in [66, 100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135]:
    for chroma, separate, scaling, poc, frame_only, cropped, vui in [
      (1, 0, 0, 0, 1, 0, "none"), (1, 0, 1, 1, 0, 1, "untimed"),
      (3, 1, 0, 2, 1, 1, "timing"), (2, 0, 0, 0, 1, 0, "malformed"),
    ]:
      packet = synthetic(scope, profile, chroma, separate, scaling, poc, frame_only, cropped, vui)
      packets += [packet, b"prefix\x00\x00\x01\x68\x80" + packet * 12 + b"\x00\x00"]
      packets += [packet[:end] for end in range(len(packet))]
  packets += [p.read_bytes() for p in sorted((args.root / ".omo/evidence/253-cluster-resume/decoder-source-aligned").glob("*.h264"))]
  randomizer = random.Random(253269)
  packets += [b"\x00\x00\x01\x67" + randomizer.randbytes(size) for size in range(256)]
  flags = [(value, codec, key) for value in [0, 1, 8, 16, 32, 0x20000, 0x2000000, 0x2020038, 0xffffffff]
           for codec in [False, True] for key in [False, True]]
  gates = [(index, limit, interval) for index in range(151) for limit, interval in [(40, 30), (60, 25), (0, 0), (0, 7)]]
  request = {"packets": [list(packet) for packet in packets], "flags": flags, "gates": gates}
  expected = {"packets": [{"heads": [scope["_bytes_head"](packet, limit) for limit in [0, 8, 16, 32]],
                            "summaries": [scope["_h264_packet_summary"](packet, limit) for limit in [0, 1, 6, 10]],
                            "sps": [scope["_h264_sps_info"](packet[start:end]) for _, start, end in scope["_h264_nals"](packet)]}
                           for packet in packets],
              "flags": [scope["_h264_flags_text"](*values) for values in flags],
              "gates": [scope["_debug_should_log"](*values) for values in gates]}
  (args.output / "request.json").write_text(json.dumps(request))
  (args.output / "source.json").write_text(json.dumps(expected))
  result = subprocess.run([str(args.native)], input=json.dumps(request), text=True, capture_output=True, timeout=30, check=True)
  (args.output / "native.json").write_text(result.stdout)
  actual = json.loads(result.stdout)
  failures = [{"section": section, "index": index, "source": left, "native": right}
              for section in expected for index, (left, right) in enumerate(zip(expected[section], actual[section], strict=True)) if left != right]
  report = {"packets": len(packets), "flag_cases": len(flags), "log_gates": len(gates), "failures": failures,
            "source_sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest()}
  (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))
  if failures:
    raise AssertionError(failures)


if __name__ == "__main__":
  main()
