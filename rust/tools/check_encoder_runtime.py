#!/usr/bin/env python3
from __future__ import annotations

import argparse
import gc
import hashlib
import json
import os
from pathlib import Path
import resource
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

# width, height, allocation length, stride, UV offset: the existing padded NV12
# fixture. These are transport layouts, not encoder output dimensions.
DEFAULT_LAYOUT = (128, 80, 160 * 120, 160, 160 * 80)
LAYOUT_CHANGES = {
  "dimensions": (160, 96, 192 * 144, 192, 192 * 96),
  "stride": (128, 80, 192 * 120, 192, 192 * 80),
  "uv-offset": (128, 80, 160 * 120 + 128, 160, 160 * 80 + 128),
  "allocation-length": (128, 80, 160 * 120 + 128, 160, 160 * 80),
}
LAYOUT_FAILURE = "encoder stream layout changed after reconnect; supervisor restart required"


def digest(data: bytes) -> str:
  return hashlib.sha256(data).hexdigest()


def assert_restart_sequence(rows: dict, before: dict) -> int:
  """Only a bounded generation-handshake prefix may be missing, never holes."""
  first = None
  for service, packets in rows.items():
    post = packets[before[service]:]
    if service == "thumbnail":
      assert not post, "no thumbnail is due in frames 500..519"
      continue
    ids = [packet["idx"]["frameId"] for packet in post]
    assert ids and ids[0] in (501, 502), (service, "handshake loss exceeds two frames", ids)
    assert ids == list(range(ids[0], 520)), (service, "noncontiguous repaired output", ids)
    if first is None:
      first = ids[0]
    assert first == ids[0], "codecs consumed different post-restart frames"
    assert all((packet["width"], packet["height"]) ==
               ((526, 330) if service == "qRoadEncodeData" else DEFAULT_LAYOUT[:2]) for packet in post)
    # Independent continuity checks, in addition to the full original control.
    previous = packets[before[service] - 1]["idx"]
    for packet in post:
      index = packet["idx"]
      assert index["encodeId"] == previous["encodeId"] + 1, (service, previous, index)
      assert index["segmentNum"] in (previous["segmentNum"], previous["segmentNum"] + 1)
      assert index["segmentId"] == (previous["segmentId"] + 1 if index["segmentNum"] == previous["segmentNum"] else 0)
      previous = index
  assert first is not None
  return first


def assert_exact_control(candidate: Peer, control: Peer) -> None:
  # Keep actual bytes, not only their hashes, in this assertion. Only wall-clock
  # and envelope monotonic timestamps are excluded by the existing drain parser.
  assert candidate.rows == control.rows, f"metadata/codec hashes differ: {candidate.directory}, {control.directory}"
  assert candidate.payloads == control.payloads, f"encoded bytes differ: {candidate.directory}, {control.directory}"


def prepare_child(cpus: set[int]) -> None:
  os.sched_setaffinity(0, cpus)
  # Expected layout rejection aborts a real owned process. Keep its exit status
  # and diagnostic without generating a reproducible, potentially large core.
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


class Peer:
  def __init__(self, binary: Path, directory: Path, case: str, native: Path, cpus: set[int],
               *, layout: tuple[int, ...] | None = None):
    self.directory = directory
    directory.mkdir()
    self.case = case
    self.is_native = directory.name.endswith("-rust")
    self.before_restart = None
    self.layout = DEFAULT_LAYOUT if layout is None else layout
    self.expected_failure = None
    self.closed = False
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
    self.payloads: dict[str, list[tuple[bytes, bytes]]] = {service: [] for service in self.services}
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
    self.command, self.environment, self.cpus = command, environment, cpus
    self.launch()

  def launch(self, phase: str = "") -> None:
    self.log_path = self.directory / f"{phase}daemon.log"
    self.output = self.log_path.open("wb")
    self.process = subprocess.Popen(self.command, stdout=self.output, stderr=subprocess.STDOUT,
                                    env=self.environment, preexec_fn=lambda: prepare_child(self.cpus))
    try:
      self.await_initialized(phase)
    except BaseException:
      self.close()
      raise

  def await_initialized(self, phase: str = "") -> None:
    deadline = time.monotonic() + 10
    expected = len(self.streams) * 64
    while True:
      self.check_alive()
      mappings = Path(f"/proc/{self.process.pid}/maps").read_text()
      imported = mappings.count("/dev/shm/msgq_visionbuf_")
      publishers_ready = all(str(self.shm / service) in mappings for service in self.services)
      if imported >= expected and publishers_ready:
        self.directory.joinpath(f"{phase}maps.txt").write_text(mappings)
        executable = Path(f"/proc/{self.process.pid}/exe")
        stat = executable.stat()
        self.directory.joinpath(f"{phase}executed-elf.json").write_text(json.dumps({
          "pid": self.process.pid, "sha256": digest(executable.read_bytes()),
          "device": stat.st_dev, "inode": stat.st_ino, "bytes": stat.st_size,
        }, indent=2) + "\n")
        if "libpython" in mappings:
          raise AssertionError("native encoder mapped a Python runtime")
        self.directory.joinpath(f"{phase}startup.json").write_text(json.dumps({"imported_buffers": imported, "expected": expected,
                                                                             "layout": self.layout}) + "\n")
        break
      if time.monotonic() >= deadline:
        raise TimeoutError(f"encoder startup: {self.directory}")
      time.sleep(0.01)
    time.sleep(0.25)
    self.drain()

  def start_server(self) -> None:
    self.server = VisionIpcServer("camerad")
    for stream in self.streams:
      self.server.create_buffers_with_sizes(stream, 64, *self.layout)
    self.server.start_listener()

  def replace_server(self, layout: tuple[int, ...] | None = None) -> None:
    del self.server
    gc.collect()
    if layout is not None:
      self.layout = layout
    self.start_server()

  def imported_names(self) -> set[str]:
    return {line.split()[5] for line in Path(f"/proc/{self.process.pid}/maps").read_text().splitlines()
            if "/dev/shm/msgq_visionbuf_" in line}

  def generation_handshake(self) -> None:
    # Original msgq_init_publisher evicts the prior reader registration. Sending
    # 500 wakes that reader; wait for its new registration AND caught-up pointer
    # before 501. No additional inputs may be sacrificed waiting for recovery.
    may_exit = self.expected_failure is not None
    self.send(500, expect_alive=not may_exit)
    deadline = time.monotonic() + 10
    while True:
      if may_exit and self.process.poll() is not None:
        return
      self.check_alive()
      # Read-only snapshot of msgq_header_t (msgq_repo/msgq/msgq.h): 3 scalar
      # words and three arrays of 40 words. Do not change registration/pointers.
      with (self.shm / f"visionipc_camerad_{int(self.streams[0])}").open("rb") as queue:
        header = struct.unpack("=123Q", queue.read(123 * 8))
      count, write = header[:2]
      assert count <= 40
      if count and all(header[83 + index] and header[43 + index] and header[3 + index] == write
                       for index in range(count)):
        break
      assert time.monotonic() < deadline, "VisionIPC generation reader did not re-register/catch up"
      time.sleep(0.01)
    # 501 may be the server-ID mismatch packet, or the first accepted packet if
    # the reader re-registered before 500. Subsequent inputs start only after
    # replacement imports are observed; they have no handshake-loss exemption.
    self.send(501, expect_alive=not may_exit)

  def await_replacement_imports(self, previous: set[str]) -> None:
    deadline = time.monotonic() + 10
    while True:
      self.check_alive()
      names = self.imported_names()
      if not names.intersection(previous) and len(names - previous) == 64 * len(self.streams):
        self.directory.joinpath("replacement-imports.json").write_text(json.dumps({
          "old": sorted(previous), "new": sorted(names), "old_mappings_remaining": 0,
        }, indent=2) + "\n")
        return
      assert time.monotonic() < deadline, "encoder did not replace its imported camera buffers"
      time.sleep(0.01)

  def await_layout_failure(self) -> int:
    self.expected_failure = (-signal.SIGABRT, LAYOUT_FAILURE)
    try:
      status = self.process.wait(timeout=10)
    except subprocess.TimeoutExpired:
      self.process.kill()
      self.process.wait(timeout=5)
      raise AssertionError("layout change neither rejected nor requested a supervisor restart")
    check_outcome(status, self.log_path.read_text(), self.expected_failure)
    self.directory.joinpath("layout-rejection.json").write_text(json.dumps({
      "pid": self.process.pid, "returncode": status, "diagnostic": LAYOUT_FAILURE,
      "new_layout": self.layout, "supervisor_restart_required": True,
    }, indent=2) + "\n")
    self.drain()
    return status

  def launch_replacement_process(self) -> None:
    assert self.process.poll() == -signal.SIGABRT
    old_pid = self.process.pid
    self.directory.joinpath("old-packets.json").write_text(json.dumps(self.rows, indent=2) + "\n")
    self.directory.joinpath("old-inputs.json").write_text(json.dumps(self.inputs, indent=2) + "\n")
    self.raw.close()
    self.output.close()
    self.raw = (self.directory / "replacement-messages.bin").open("wb")
    self.rows = {service: [] for service in self.services}
    self.payloads = {service: [] for service in self.services}
    self.inputs = []
    self.expected_failure = None
    # Same server B, IPC namespace and Params, but a distinct actual child with
    # fresh codec ownership. No in-process geometry reconfiguration is allowed.
    self.launch("replacement-")
    assert self.process.pid != old_pid

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
            payload = bytes(value.pop("thumbnail"))
            header = b""
            value["thumbnail_sha256"] = digest(payload)
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
          self.payloads[service].append((header, payload))

  def send(self, frame: int, pause: float = 0.05, *, expect_alive: bool = True) -> None:
    _, _, length, stride, _ = self.layout
    for stream in self.streams:
      pixels = bytes((index * 17 + index // stride * 13 + frame * 7 + int(stream) * 11) % 256 for index in range(length))
      self.server.send(stream, pixels, frame, 1_000_000_000 + frame * 50_000_000, 1_020_000_000 + frame * 50_000_000)
    self.inputs.append({"frame": frame, "active": (self.params / "CarrotVisionActive").read_text(),
                        "streams": [int(stream) for stream in self.streams], "layout": self.layout,
                        "timestamp_sof": 1_000_000_000 + frame * 50_000_000,
                        "timestamp_eof": 1_020_000_000 + frame * 50_000_000})
    time.sleep(pause)
    if expect_alive:
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

  def exercise(self, *, replace_server: bool = True) -> None:
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
      # Keep generation B inside an already warm codec segment (450..529).
      # The old 229->500 gap forced multiple source-policy rotations, mixing
      # codec startup delay with transport handshake loss. This fixed history is
      # identical for original, native and uninterrupted control, never adapted
      # to the native output. All other scenarios keep their original history.
      history = range(370, 500) if self.case == "main-restart" else range(100, 230)
      for frame in history:
        self.send(frame)
      self.await_frame(history.stop - 1)
      thumbnails = 0 if self.case == "main-restart" else 1
      if self.case.startswith("main") and len(self.rows["thumbnail"]) != thumbnails:
        raise AssertionError("missing frame-100 thumbnail")
      for service, rows in self.rows.items():
        if service != "thumbnail" and {row["idx"]["segmentNum"] for row in rows} != {0, 1}:
          raise AssertionError(f"missing rotated packets: {service}")
    if self.case == "main-restart" and replace_server:
      before = {key: len(value) for key, value in self.rows.items()}
      self.before_restart = before
      previous_imports = self.imported_names()
      self.replace_server()
      self.generation_handshake()
      if self.is_native:
        self.await_replacement_imports(previous_imports)
      for frame in range(502, 520):
        self.send(frame)
      self.drain()
      if self.is_native:
        self.await_frame(519)
        self.post_restart_start = assert_restart_sequence(self.rows, before)
      elif before != {key: len(value) for key, value in self.rows.items()}:
        raise AssertionError("original receive-only restart behavior changed")

  def close(self) -> None:
    if getattr(self, "closed", False):
      return
    self.closed = True
    expected_failure = getattr(self, "expected_failure", None)
    try:
      if expected_failure is None:
        stop_cleanly(self.process, self.directory / "shutdown.json")
      else:
        try:
          status = self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
          self.process.kill()
          self.process.wait(timeout=5)
          raise AssertionError("expected encoder layout failure did not terminate")
        check_outcome(status, self.log_path.read_text(), expected_failure)
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
    check_outcome(self.process.returncode, self.log_path.read_text(), expected_failure)


def verify_repaired_restart(candidate: Peer, source: Path, directory: Path, native: Path, cpus: set[int]) -> dict:
  # Validate the native output BEFORE deciding control inputs. The only permitted
  # selection is its first accepted frame in the predeclared {501,502} window;
  # every subsequent frame must exist in both codecs. Never replay an arbitrary
  # observed list or intersect away missing native packets.
  first = assert_restart_sequence(candidate.rows, candidate.before_restart)
  control = Peer(source, directory, "main-restart", native, cpus)
  try:
    control.exercise(replace_server=False)
    for frame in range(first, 520):
      control.send(frame)
    control.await_frame(519)
  finally:
    control.close()
  assert control.inputs == [item for item in candidate.inputs if item["frame"] < 500 or item["frame"] >= first]
  assert_exact_control(candidate, control)
  result = {"repaired_restart": True, "uninterrupted_original_control": str(directory),
            "pre_and_post_bytes_equal_to_control": True, "post_metadata_and_indices_exact": True,
            "handshake_inputs": [500, 501], "maximum_handshake_loss": 2,
            "first_accepted": first, "accepted_post_frames": list(range(first, 520)),
            "packets": {service: len(rows) for service, rows in control.rows.items()},
            "input_pixels_layout_and_timestamps_equal": True}
  candidate.directory.joinpath("repaired-restart.json").write_text(json.dumps(result, indent=2) + "\n")
  return result


def verify_layout_restarts(binary: Path, source: Path, directory: Path, native: Path, cpus: set[int]) -> list[dict]:
  results = []
  for name, layout in LAYOUT_CHANGES.items():
    peer = Peer(binary, directory / f"layout-{name}-rust", "main-restart", native, cpus)
    try:
      for frame in range(100, 170):
        peer.send(frame)
      peer.await_frame(169)
      before = {service: len(rows) for service, rows in peer.rows.items()}
      peer.replace_server(layout)
      peer.expected_failure = (-signal.SIGABRT, LAYOUT_FAILURE)
      peer.generation_handshake()
      status = peer.await_layout_failure()
      assert before == {service: len(rows) for service, rows in peer.rows.items()}, "changed-layout input reached old codec"
      old_pid = peer.process.pid
      peer.launch_replacement_process()
      for frame in range(100, 170):
        peer.send(frame)
      peer.await_frame(169)
      for service, rows in peer.rows.items():
        if service == "thumbnail":
          assert [row["frameId"] for row in rows] == [100]
          continue
        ids = [row["idx"]["frameId"] for row in rows]
        # A newly opened H264 codec has inherited startup buffering. All 70
        # inputs go to both processes, without adapting the control to outputs;
        # exact source comparison below checks that startup behavior as well.
        assert ids and all(100 <= frame < 170 for frame in ids), service
        if service == "roadEncodeData":
          assert ids == list(range(100, 170))
        assert [row["idx"]["encodeId"] for row in rows] == list(range(len(rows)))
        assert [row["idx"]["segmentId"] for row in rows] == list(range(len(rows)))
        assert all(row["idx"]["segmentNum"] == 0 for row in rows)
        assert all((row["width"], row["height"]) == ((526, 330) if service == "qRoadEncodeData" else layout[:2])
                   for row in rows), service
    finally:
      peer.close()
    # Independent original process starts normally on the new complete layout.
    # Compare every byte/field, not just successful receipt or matching dimensions.
    control = Peer(source, directory / f"layout-{name}-source", "main-restart", native, cpus, layout=layout)
    try:
      for frame in range(100, 170):
        control.send(frame)
      control.await_frame(169)
    finally:
      control.close()
    assert peer.inputs == control.inputs
    assert_exact_control(peer, control)
    results.append({"case": name, "layout": layout, "rejection_returncode": status,
                    "rejected_before_old_codec_publication": True, "old_pid": old_pid,
                    "replacement_pid": peer.process.pid, "same_server_B_reused": True,
                    "replacement_bytes_indices_geometry_exact": True})
  directory.joinpath("layout-restarts.json").write_text(json.dumps(results, indent=2) + "\n")
  return results


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
    compared = [peer.rows for peer in peers]
    compared_payloads = [peer.payloads for peer in peers]
    if case == "main-restart":
      # #191 deliberately repairs only the native restart boundary. Every
      # publication before replacement still matches the unchanged source.
      compared = [{service: rows[:peer.before_restart[service]] for service, rows in peer.rows.items()} for peer in peers]
      compared_payloads = [{service: payloads[:peer.before_restart[service]] for service, payloads in peer.payloads.items()} for peer in peers]
    if compared[0] != compared[1]:
      raise AssertionError(f"{case}: source/Rust publications differ; inspect packets.json")
    assert compared_payloads[0] == compared_payloads[1], f"{case}: source/Rust encoded bytes differ"
    counts = {key: len(value) for key, value in peers[0].rows.items()}
    result = {"case": case, "packets_per_process": counts, "exact_bytes_and_metadata": True,
              "repaired_restart": False}
    if case == "main-restart":
      result["original_restart_failure_preserved"] = True
      result["restart_control"] = verify_repaired_restart(peers[1], arguments.source.resolve(),
        output / f"case-{index}-uninterrupted-source", arguments.native.resolve(), cpus)
      result["layout_restarts"] = verify_layout_restarts(arguments.rust.resolve(), arguments.source.resolve(),
        output, arguments.native.resolve(), cpus)
      result["repaired_restart"] = True
      result["post_restart_bytes_exact"] = True
      result["comparison_scope"] = "pre-boundary original; post-boundary uninterrupted original control"
      del result["packets_per_process"]
      result["packets_by_process"] = {
        "original_failed_restart": counts,
        "native_repaired": {service: len(rows) for service, rows in peers[1].rows.items()},
        "uninterrupted_original_control": result["restart_control"]["packets"],
      }
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
