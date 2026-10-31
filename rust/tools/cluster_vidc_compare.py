#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

SCENARIOS = [
  "normal",
  "wide-stride",
  "min-low",
  "min-high",
  "min-control-eintr",
  "fewer-buffers",
  "extra-buffers",
  "other-capability",
  "ioctl-eintr",
  "poll-eintr",
  "enoent",
  "timestamp-zero",
  "empty-frame",
  "port-change",
  "leased-port-change",
  "timeout",
  "poll-error",
  "wrong-input-index",
  "wrong-capture-index",
  "zero-buffers",
  "wrong-h264",
  "wrong-nv12",
  "zero-input-size",
  "queue-failure",
]
MACROS = {
  "V4L2_EVENT_MSM_VIDC_START",
  "V4L2_EVENT_MSM_VIDC_FLUSH_DONE",
  "V4L2_EVENT_MSM_VIDC_PORT_SETTINGS_CHANGED_INSUFFICIENT",
  "V4L2_QCOM_CMD_FLUSH_CAPTURE",
  "V4L2_QCOM_CMD_FLUSH",
}


def digest(path):
  return hashlib.sha256(path.read_bytes()).hexdigest()


def errno_output(rows):
  values = []
  for row in rows:
    error = re.fullmatch(r"ERROR (?:VIDIOC_[^:]+: )?(.*?) \((?:os error )?(\d+)\)", row)
    values.append({"errno": int(error[2]), "message": error[1]} if error else row)
  return values


def build(command, output):
  available = shutil.disk_usage(output.parent).free
  growth = 128 * 1024**2
  if available < growth:
    raise RuntimeError(f"insufficient build space: {available}")
  result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30, check=False)
  output.write_bytes(result.stdout)
  if result.returncode:
    raise RuntimeError(f"source ABI build failed; {output}")
  return {"argv": command, "available_bytes": available, "estimated_growth_bytes": growth}


def prepare(root, output):
  fixture = root / "rust/tools/decoder_oracle"
  paths = [
    "openpilot/tools/replay/qcom_decoder.h",
    "msgq/visionipc/visionbuf.h",
    "msgq/visionipc/visionbuf_ion.cc",
    "openpilot/system/camerad/cameras/nv12_info.h",
    "openpilot/system/loggerd/decoder/cluster_h264_decoder.h",
    "openpilot/system/loggerd/decoder/cluster_h264_decoder.cc",
  ]
  vendor = "\n".join(line for line in (root / paths[0]).read_text().splitlines() if line.startswith("#define ") and line.split()[1] in MACROS)
  (output / "vendor_events.h").write_text(vendor + "\n")
  pieces = [
    "#include <iostream>\n#include <array>\n#include <deque>\n#include <mutex>\n#include <chrono>",
    (root / "rust/tools/encoder_oracle/v4l_adapter.h").read_text(),
    vendor,
  ]
  for path, begin in [
    (paths[1], "enum VisionStreamType"),
    (paths[2], "struct IonFileHandle"),
    (paths[3], "#include <cassert>"),
    (paths[4], "constexpr unsigned int"),
    (paths[5], "namespace {"),
  ]:
    text = (root / path).read_text()
    pieces.append(text[text.index(begin) :])
  pieces.append((fixture / "main.cc").read_text())
  (output / "source.cc").write_text("\n".join(pieces))
  includes = ["-I" + str(root), "-I" + str(root / "openpilot"), "-I" + str(root / "third_party/linux/include"), "-I" + str(output)]
  builds = [
    build(["c++", "-std=c++20", "-O1", "-g0", "-pthread", *includes, str(output / "source.cc"), "-o", str(output / "source")], output / "source-build.log"),
    build(
      ["c++", "-std=c++20", "-O1", "-g0", "-pthread", "-shared", "-fPIC", *includes, str(fixture / "fake_vidc.cc"), "-ldl", "-o", str(output / "fake-vidc.so")],
      output / "provider-build.log",
    ),
  ]
  return {
    "source_hashes": {path: digest(root / path) for path in paths},
    "builds": builds,
    "provider_sha256": digest(output / "fake-vidc.so"),
    "source_binary_sha256": digest(output / "source"),
  }


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("root", type=Path)
  parser.add_argument("output", type=Path)
  parser.add_argument("--binary", type=Path)
  args = parser.parse_args()
  root, output = args.root.resolve(), args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  preparation = prepare(root, output)
  (output / "preparation.json").write_text(json.dumps(preparation, indent=2))
  if args.binary is None:
    return
  cases = []
  for scenario in SCENARIOS:
    values = {}
    for kind, binary in [("source", output / "source"), ("native", args.binary.resolve())]:
      trace = output / f"{scenario}-{kind}.trace"
      trace.unlink(missing_ok=True)
      env = {**os.environ, "LD_PRELOAD": str(output / "fake-vidc.so"), "VIDC_FAKE_CASE": scenario, "VIDC_FAKE_TRACE": str(trace)}
      result = subprocess.run([str(binary), scenario], env=env, capture_output=True, timeout=5, check=False)
      (output / f"{scenario}-{kind}.stdout").write_bytes(result.stdout)
      (output / f"{scenario}-{kind}.stderr").write_bytes(result.stderr)
      if result.returncode:
        raise RuntimeError(f"{scenario} {kind} process failed {result.returncode}")
      values[kind] = {
        "output": result.stdout.decode().splitlines(),
        "trace": [line for line in trace.read_text().splitlines() if line not in ("OPEN ion", "CLOSE ion")],
      }
    cases.append(
      {
        "name": scenario,
        "output_match": values["source"]["output"] == values["native"]["output"],
        "behavior_match": errno_output(values["source"]["output"]) == errno_output(values["native"]["output"]),
        "trace_match": values["source"]["trace"] == values["native"]["trace"],
        "source": values["source"],
        "native": values["native"],
      }
    )
  result = {
    **preparation,
    "native_binary_sha256": digest(args.binary),
    "cases": len(cases),
    "matched": sum(case["behavior_match"] and case["trace_match"] for case in cases),
    "literal_outputs_matched": sum(case["output_match"] for case in cases),
    "results": cases,
  }
  (output / "result.json").write_text(json.dumps(result, indent=2))
  print(json.dumps({"cases": result["cases"], "matched": result["matched"]}))
  raise SystemExit(result["matched"] != result["cases"])


if __name__ == "__main__":
  main()
