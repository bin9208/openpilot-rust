#!/usr/bin/env python3
from __future__ import annotations

import argparse
import gc
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import time

from msgq.visionipc import VisionIpcServer, VisionStreamType
import openpilot.cereal.messaging as messaging
from openpilot.cereal import log
from encoder_outcomes import check_outcome, stop_cleanly


STREAMS = [VisionStreamType.VISION_STREAM_ROAD, VisionStreamType.VISION_STREAM_DRIVER, VisionStreamType.VISION_STREAM_WIDE_ROAD]
CASES = [
  "main",
  "--stream",
  "--carrot-vision-road",
  "--youtube-low",
  "--youtube-medium",
  "--youtube",
  "--youtube-wide",
  "main-multi",
  "main-restart",
  "main-lag",
]


def digest(data: bytes) -> str:
  return hashlib.sha256(data).hexdigest()


class Peer:
  def __init__(self, binary: Path, directory: Path, case: str, native: Path, cpus: set[int]):
    self.directory = directory
    directory.mkdir()
    self.case = case
    self.prefix = f"encoder-runtime-{os.getpid()}-{directory.name}"
    self.shm = Path("/dev/shm") / ("msgq_" + self.prefix)
    self.shm.mkdir()
    self.params = directory / "params" / self.prefix
    self.params.mkdir(parents=True)
    for key, value in {"RecordRoadCam": "2", "RecordFront": "1", "RecordAudio": "0", "CarrotVisionActive": "0"}.items():
      (self.params / key).write_text(value)
    os.environ["OPENPILOT_PREFIX"] = self.prefix
    self.streams = STREAMS if case == "main-multi" else [STREAMS[2] if case == "--youtube-wide" else STREAMS[0]]
    if case.startswith("main"):
      self.services = ["roadEncodeData", "qRoadEncodeData", "thumbnail"]
      if case == "main-multi":
        self.services += ["driverEncodeData", "wideRoadEncodeData"]
    else:
      self.services = ["livestreamRoadEncodeData" if case in ["--stream", "--carrot-vision-road"] else "youtubeRoadEncodeData"]
    self.sockets = {service: messaging.sub_sock(service, conflate=False, timeout=0) for service in self.services}
    self.rows: dict[str, list[dict[str, object]]] = {service: [] for service in self.services}
    self.raw = (directory / "messages.bin").open("wb")
    self.inputs: list[dict[str, object]] = []
    self.start_server()
    environment = dict(os.environ, PARAMS_ROOT=str(directory / "params"), LOGGERD_TEST="1", LOGGERD_SEGMENT_LENGTH="4", LOGPRINT="debug")
    environment["DEBUG_ENCODER"] = "1" if case == "main" else "0"
    environment["LD_LIBRARY_PATH"] = str(native / "usr/lib/x86_64-linux-gnu") + (
      ":" + environment["LD_LIBRARY_PATH"] if environment.get("LD_LIBRARY_PATH") else ""
    )
    command = [str(binary)] + ([] if case.startswith("main") else [case])
    directory.joinpath("invocation.json").write_text(
      json.dumps(
        {
          "argv": command,
          "binary_sha256": digest(binary.read_bytes()),
          "fixture_cpus": sorted(cpus),
          "prefix": self.prefix,
          "streams": [int(stream) for stream in self.streams],
        },
        indent=2,
      )
      + "\n"
    )
    self.output = (directory / "daemon.log").open("wb")
    self.process = subprocess.Popen(command, stdout=self.output, stderr=subprocess.STDOUT, env=environment, preexec_fn=lambda: os.sched_setaffinity(0, cpus))
    try:
      self.await_initialized()
    except BaseException:
      self.close()
      raise

  def await_initialized(self) -> None:
    deadline = time.monotonic() + 10
    expected = len(self.streams) * 64
    while True:
      self.check_alive()
      mappings = Path(f"/proc/{self.process.pid}/maps").read_text()
      imported = mappings.count("/dev/shm/msgq_visionbuf_")
      publishers_ready = all(str(self.shm / service) in mappings for service in self.services)
      if imported >= expected and publishers_ready:
        self.directory.joinpath("maps.txt").write_text(mappings)
        executable = Path(f"/proc/{self.process.pid}/exe")
        stat = executable.stat()
        self.directory.joinpath("executed-elf.json").write_text(json.dumps({
          "pid": self.process.pid, "sha256": digest(executable.read_bytes()),
          "device": stat.st_dev, "inode": stat.st_ino, "bytes": stat.st_size,
        }, indent=2) + "\n")
        if "libpython" in mappings:
          raise AssertionError("native encoder mapped a Python runtime")
        self.directory.joinpath("startup.json").write_text(json.dumps({"imported_buffers": imported, "expected": expected}) + "\n")
        break
      if time.monotonic() >= deadline:
        raise TimeoutError(f"encoder startup: {self.directory}")
      time.sleep(0.01)
    time.sleep(0.25)
    self.drain()

  def start_server(self) -> None:
    self.server = VisionIpcServer("camerad")
    for stream in self.streams:
      self.server.create_buffers_with_sizes(stream, 64, 128, 80, 160 * 120, 160, 160 * 80)
    self.server.start_listener()

  def check_alive(self) -> None:
    if self.process.poll() is not None:
      raise RuntimeError(f"encoder exited {self.process.returncode}: {self.directory}")

  def drain(self) -> None:
    for service, socket in self.sockets.items():
      for raw in messaging.drain_sock_raw(socket):
        self.raw.write(struct.pack("<I", len(raw)))
        self.raw.write(raw)
        with log.Event.from_bytes(raw) as event:
          if event.which() != service or not event.valid or event.logMonoTime <= 0:
            raise AssertionError(f"invalid publication envelope: {service}")
          packet = getattr(event, service)
          value = packet.to_dict()
          if service == "thumbnail":
            value["thumbnail_sha256"] = digest(bytes(value.pop("thumbnail")))
          else:
            if value.pop("unixTimestampNanos") <= 0:
              raise AssertionError("missing wall timestamp")
            payload = bytes(value.pop("data"))
            header = bytes(value.pop("header", b""))
            if value["idx"]["len"] != len(payload):
              raise AssertionError("incorrect encoded length")
            value["data_sha256"] = digest(payload)
            value["header_sha256"] = digest(header)
            if int(value["idx"]["flags"]) & 8 == 0 and header:
              raise AssertionError("header on a non-keyframe")
          self.rows[service].append(value)

  def send(self, frame: int, pause: float = 0.05) -> None:
    for stream in self.streams:
      pixels = bytes((index * 17 + index // 160 * 13 + frame * 7 + int(stream) * 11) % 256 for index in range(160 * 120))
      self.server.send(stream, pixels, frame, 1_000_000_000 + frame * 50_000_000, 1_020_000_000 + frame * 50_000_000)
    self.inputs.append({"frame": frame, "active": (self.params / "CarrotVisionActive").read_text(), "streams": [int(stream) for stream in self.streams]})
    time.sleep(pause)
    self.check_alive()
    self.drain()

  def await_frame(self, frame: int) -> None:
    deadline = time.monotonic() + 15
    videos = [service for service in self.services if service != "thumbnail"]
    while not all(self.rows[service] and self.rows[service][-1]["idx"]["frameId"] == frame for service in videos):
      self.check_alive()
      self.drain()
      if time.monotonic() >= deadline:
        raise TimeoutError(f"last frame {frame}: {self.directory}; {[len(self.rows[service]) for service in videos]}")
      time.sleep(0.01)

  def exercise(self) -> None:
    if self.case == "main-multi":
      self.send(98, 0.2)
      self.send(99, 0.2)
    if self.case == "main-lag":
      self.process.send_signal(signal.SIGSTOP)
      deadline = time.monotonic() + 2
      while "\nState:\tT" not in Path(f"/proc/{self.process.pid}/status").read_text():
        if time.monotonic() >= deadline:
          raise TimeoutError("owned encoder did not stop")
        time.sleep(0.001)
      try:
        for frame in range(100, 170):
          self.send(frame, 0)
      finally:
        self.process.send_signal(signal.SIGCONT)
      self.await_frame(169)
      for frame in range(170, 240):
        self.send(frame)
      self.await_frame(239)
      if self.rows["roadEncodeData"][0]["idx"]["frameId"] != 106:
        raise AssertionError("overwritten native buffers were not dropped")
      if self.rows["thumbnail"]:
        raise AssertionError("overwritten thumbnail frame was encoded")
    elif self.case == "--carrot-vision-road":
      self.send(100)
      for frame in range(101, 121):
        self.send(frame)
      if any(self.rows.values()):
        raise AssertionError("inactive software prewarm unexpectedly produced a packet")
      (self.params / "CarrotVisionActive").write_text("1")
      for frame in range(121, 206):
        self.send(frame)
      self.await_frame(205)
      previous = {key: len(value) for key, value in self.rows.items()}
      (self.params / "CarrotVisionActive").write_text("0")
      for frame in range(206, 216):
        self.send(frame)
      if previous != {key: len(value) for key, value in self.rows.items()}:
        raise AssertionError("inactive encoder published new frames")
      (self.params / "CarrotVisionActive").write_text("1")
      for frame in range(216, 266):
        self.send(frame)
      self.await_frame(265)
      if any(row["idx"]["segmentNum"] != 0 for row in self.rows[self.services[0]]):
        raise AssertionError("on-demand encoder rotated")
    else:
      for frame in range(100, 230):
        self.send(frame)
      self.await_frame(229)
      if self.case.startswith("main") and len(self.rows["thumbnail"]) != 1:
        raise AssertionError("missing frame-100 thumbnail")
      for service, rows in self.rows.items():
        if service != "thumbnail" and {row["idx"]["segmentNum"] for row in rows} != {0, 1}:
          raise AssertionError(f"missing rotated packets: {service}")
    if self.case == "main-restart":
      before = {key: len(value) for key, value in self.rows.items()}
      del self.server
      gc.collect()
      self.start_server()
      for frame in range(500, 520):
        self.send(frame)
      self.drain()
      if before != {key: len(value) for key, value in self.rows.items()}:
        raise AssertionError("encoder changed inherited receive-only restart behavior")

  def close(self) -> None:
    try:
      stop_cleanly(self.process, self.directory / "shutdown.json")
    finally:
      try:
        self.drain()
      finally:
        self.raw.close()
        self.output.close()
        self.directory.joinpath("packets.json").write_text(json.dumps(self.rows, indent=2) + "\n")
        self.directory.joinpath("inputs.json").write_text(json.dumps(self.inputs, indent=2) + "\n")
        del self.server
        self.sockets.clear()
        gc.collect()
        shutil.rmtree(self.shm)
        Path(f"/tmp/{self.prefix}_visionipc_camerad").unlink(missing_ok=True)
        Path(f"/tmp/logmessage{self.prefix}").unlink(missing_ok=True)
    check_outcome(self.process.returncode, (self.directory / "daemon.log").read_text())


def main() -> None:
  parser = argparse.ArgumentParser(description="Run source and Rust encoderd over owned original VisionIPC and native message queues")
  parser.add_argument("--source", type=Path, required=True)
  parser.add_argument("--rust", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--case", action="append", choices=CASES)
  arguments = parser.parse_args()
  output = arguments.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  if shutil.disk_usage(output).free < 26 * 1024**3:
    raise RuntimeError("runtime capture requires 25 GiB plus 1 GiB headroom")
  cpus = set(sorted(os.sched_getaffinity(0))[:2])
  results: list[dict[str, object]] = []
  for index, case in enumerate(arguments.case or CASES):
    peers = []
    for kind, binary in [("source", arguments.source.resolve()), ("rust", arguments.rust.resolve())]:
      peer = Peer(binary, output / f"case-{index}-{kind}", case, arguments.native.resolve(), cpus)
      try:
        peer.exercise()
      finally:
        peer.close()
      peers.append(peer)
    if peers[0].rows != peers[1].rows:
      raise AssertionError(f"{case}: source/Rust publications differ; inspect packets.json")
    counts = {key: len(value) for key, value in peers[0].rows.items()}
    result = {"case": case, "packets_per_process": counts, "exact_bytes_and_metadata": True}
    results.append(result)
    output.joinpath("progress.json").write_text(json.dumps(results, indent=2) + "\n")
    print(json.dumps(result), flush=True)
  output.joinpath("receipt.json").write_text(
    json.dumps(
      {"status": "PASS", "results": results, "scope": "host PC codecs and real VisionIPC/message queues; not AGNOS hardware or CPU performance"}, indent=2
    )
    + "\n"
  )


if __name__ == "__main__":
  main()
