#!/usr/bin/env python3
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import subprocess
import signal

from encoder_outcomes import check_outcome


def digest(path: Path) -> str:
  return hashlib.sha256(path.read_bytes()).hexdigest()


def no_core() -> None:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def run(command: list[str], log: Path, env: dict[str, str], expected_failure: tuple[int, str] | None = None) -> int:
  with log.open("wb") as output:
    result = subprocess.run(command, stdout=output, stderr=subprocess.STDOUT, env=env, timeout=30, preexec_fn=no_core)
  check_outcome(result.returncode, log.read_text(), expected_failure)
  return result.returncode


FAILURES = {
  "wrong-output": {"source": (-signal.SIGABRT, "YouTube encoder output contract rejected"), "rust": (1, "YouTube encoder output contract rejected")},
  "cbr-wrong": {
    "source": (-signal.SIGABRT, "V4L2 encoder did not retain CBR_CFR rate control"),
    "rust": (1, "V4L2 encoder did not retain CBR_CFR rate control"),
  },
  "wrong-capability": {
    "source": (-signal.SIGABRT, 'strcmp((const char *)cap.driver, "msm_vidc_driver") == 0'),
    "rust": (1, "source msm_vidc encoder capability assertion"),
  },
  "wrong-offset": {"source": (-signal.SIGABRT, "v4l_buf.m.planes[0].data_offset == 0"), "rust": (-signal.SIGABRT, "source V4L zero data-offset assertion")},
  "wrong-timestamp": {
    "source": (-signal.SIGABRT, "extra.timestamp_eof/1000 == ts"),
    "rust": (-signal.SIGABRT, "source V4L timestamp synchronization assertion"),
  },
  "input-queue-failure": {"source": (-signal.SIGABRT, "VIDIOC_QBUF failed"), "rust": (-signal.SIGABRT, "Invalid argument (os error 22)")},
}


def source_oracle(root: Path, output: Path) -> dict[str, str]:
  fixture = Path(__file__).with_name("encoder_oracle")
  sources: dict[str, str] = {}

  def extract(path: str, start: str, end: str | None = None) -> str:
    filename = root / path
    sources[path] = digest(filename)
    content = filename.read_text()
    begin = content.index(start)
    return content[begin : content.index(end, begin) if end else None]

  pieces = [
    fixture.joinpath("v4l_adapter.h").read_text(),
    extract("msgq_repo/msgq/visionipc/visionbuf.h", "enum VisionStreamType"),
    extract("msgq_repo/msgq/visionipc/visionbuf_ion.cc", "struct IonFileHandle"),
    extract("openpilot/system/loggerd/loggerd.h", "struct EncoderSettings", "class EncoderInfo"),
    fixture.joinpath("v4l_base.h").read_text(),
    extract("openpilot/system/loggerd/encoder/encoder.cc", "VideoEncoder::VideoEncoder", "void VideoEncoder::publisher_publish"),
    extract("openpilot/system/loggerd/encoder/v4l_encoder.h", "class V4LEncoder"),
    extract("openpilot/system/loggerd/encoder/v4l_encoder.cc", "static void dequeue_buffer"),
    fixture.joinpath("v4l_main.cc").read_text(),
  ]
  output.joinpath("source-oracle.cc").write_text("\n".join(pieces))
  return sources


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare source and Rust V4L controls, queues and EOS using a host driver fixture")
  parser.add_argument("--rust", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  arguments = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  output = arguments.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  sources = source_oracle(root, output)
  fixture = Path(__file__).with_name("encoder_oracle")
  env = dict(os.environ)
  library = arguments.native.resolve() / "usr/lib/x86_64-linux-gnu"
  env["LD_LIBRARY_PATH"] = str(library) + (":" + env["LD_LIBRARY_PATH"] if env.get("LD_LIBRARY_PATH") else "")
  includes = ["-I" + str(root), "-I" + str(root / "openpilot"), "-I" + str(root / "third_party/linux/include")]
  builds = [
    ["g++", "-std=c++20", "-O1", "-g0", "-pthread", *includes, str(output / "source-oracle.cc"), "-o", str(output / "source-oracle")],
    ["g++", "-std=c++20", "-O1", "-g0", "-pthread", "-shared", "-fPIC", *includes, str(fixture / "fake_v4l.cc"), "-ldl", "-o", str(output / "fake-v4l.so")],
  ]
  for index, command in enumerate(builds):
    guard = {"free": shutil.disk_usage(output).free, "growth": 128 * 1024**2, "floor": 25 * 1024**3}
    output.joinpath(f"build-{index}-space.json").write_text(json.dumps(guard, indent=2) + "\n")
    if guard["free"] < guard["floor"] + guard["growth"]:
      raise RuntimeError(f"insufficient disk headroom: {guard}")
    run(command, output / f"build-{index}.log", env)
  output.joinpath("build-commands.json").write_text(json.dumps(builds, indent=2) + "\n")
  env["LD_PRELOAD"] = str(output / "fake-v4l.so")
  profiles = [
    ("main", 0, 0),
    ("main", 1, 0),
    ("main", 2, 0),
    ("main", 0, 1),
    ("--stream", 0, 0),
    ("--stream", 1, 0),
    ("--stream", 2, 0),
    ("--carrot-vision-road", 0, 0),
    ("--youtube-low", 0, 0),
    ("--youtube-medium", 0, 0),
    ("--youtube", 0, 0),
    ("--youtube-wide", 0, 0),
  ]
  cases = [(profile, "normal", False, 128) for profile in profiles]
  cases.extend([(("main", 0, 0), "normal", False, 1928), (("main", 0, 0), "poll-eintr", False, 128), (("main", 0, 0), "hold-seven", False, 128)])
  cases.extend([(("--stream", 0, 0), scenario, False, 128) for scenario in ["slice-size-failure", "slice-mode-failure", "compat-failure"]])
  cases.extend([(("--youtube", 0, 0), scenario, False, 128) for scenario in ["crop-unavailable", "crop-adjusted", "cbr-unavailable", "bitrate-adjusted"]])
  cases.extend(
    [
      (("--youtube", 0, 0), scenario, True, 128)
      for scenario in ["wrong-output", "cbr-wrong", "wrong-capability", "wrong-offset", "wrong-timestamp", "input-queue-failure"]
    ]
  )
  results: list[dict[str, object]] = []
  configuration = ("QUERYCAP", "FORMAT", "FPS", "CROP", "CONTROL", "READ_CONTROL", "BUFFERS", "STREAM")
  for number, (profile, scenario, failure, width) in enumerate(cases):
    env["ENCODER_FAKE_CASE"] = scenario
    returns: dict[str, int] = {}
    traces: dict[str, list[str]] = {}
    for kind, binary in [("source", output / "source-oracle"), ("rust", arguments.rust.resolve())]:
      name = f"case-{number:02}-{scenario}-{kind}"
      directory = output / name
      driver = output / f"{name}-driver.tsv"
      env["ENCODER_FAKE_TRACE"] = str(driver)
      env["OPENPILOT_PREFIX"] = f"encv4l-{os.getpid()}-{number}-{kind}"
      command = [str(binary), str(directory), *[str(value) for value in profile], str(width), "80", "12"]
      shared_memory = Path("/dev/shm") / ("msgq_" + env["OPENPILOT_PREFIX"])
      shared_memory.mkdir()
      try:
        returns[kind] = run(command, output / f"{name}.log", env, FAILURES[scenario][kind] if failure else None)
      finally:
        shutil.rmtree(shared_memory)
      traces[kind] = driver.read_text().splitlines()
      if not failure:
        lines = directory.joinpath("trace.tsv").read_text().splitlines()
        if len(lines) != 24:
          raise AssertionError(f"{name}: expected 24 publications, got {len(lines)}")
        counts = Counter(line.split()[0] for line in traces[kind])
        for event, expected in [
          ("ION_ALLOC", 6),
          ("ION_FREE", 6),
          ("CAPTURE_HEADER", 2),
          ("CAPTURE_FRAME", 24),
          ("CAPTURE_EOS", 2),
          ("STOP", 2),
          ("RETURN_INPUT", 24),
        ]:
          if counts[event] != expected:
            raise AssertionError(f"{name}: {event} count {counts[event]}, expected {expected}")
        returned = [line for line in traces[kind] if line.startswith("QUEUE_INPUT")]
        for segment in range(2):
          first = returned[segment * 12].split()
          if first[1] != "0":
            raise AssertionError(f"{name}: rotation did not restore source slot order")
    configs = {kind: [line for line in trace if line.startswith(configuration)] for kind, trace in traces.items()}
    if configs["source"] != configs["rust"]:
      raise AssertionError(f"case {number}: ioctl configuration differs")
    if not failure:
      source_trace = output / f"case-{number:02}-{scenario}-source/trace.tsv"
      rust_trace = output / f"case-{number:02}-{scenario}-rust/trace.tsv"
      if source_trace.read_bytes() != rust_trace.read_bytes():
        raise AssertionError(f"case {number}: encoded metadata/header/data differs")
    results.append({"profile": profile, "scenario": scenario, "width": width, "expected_failure": failure, "returns": returns})
  receipt = {
    "status": "PASS",
    "scope": "host scripted ioctl/memory fixture, not VIDC hardware validation",
    "source_sha256": sources,
    "rust_binary_sha256": digest(arguments.rust.resolve()),
    "driver_sha256": digest(output / "fake-v4l.so"),
    "source_oracle_sha256": digest(output / "source-oracle"),
    "results": results,
  }
  output.joinpath("receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
  print(json.dumps({"status": "PASS", "cases": len(results)}))


if __name__ == "__main__":
  main()
