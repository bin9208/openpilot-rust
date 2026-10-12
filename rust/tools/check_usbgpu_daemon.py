# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "pytest==8.3.5"]
# ///
# Run in the retained original-model environment:
# PYTHONPATH=.:tinygrad_repo:rust/tools python -P rust/tools/check_usbgpu_daemon.py --help
"""Exercise the production driving daemon loop with an admitted owned USB worker."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import time

from check_driver_daemon import Lines, stop
from check_usbgpu_selection import MODEL_SHA, RUNTIME_SHA, setup
from openpilot.cereal import car, log
from openpilot.system.camerad.cameras.nv12_info import get_nv12_info
from usbgpu_selection_source import FLAGS


OUTPUTS = ("modelV2", "drivingModelData", "cameraOdometry")


def flags(parameters: Path) -> dict[str, bool]:
  return {name: (parameters / name).read_bytes() == b"1" if (parameters / name).exists() else False for name in FLAGS}


def lifecycle(path: Path) -> list[dict[str, int | str]]:
  return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def scenario(args: argparse.Namespace, camera: tuple[int, int], mode: str) -> None:
  destination = args.evidence / f"{camera[0]}-{mode}"
  setup(destination, args.model, args.runtime, args.marker, "normal")
  prefix = f"usbgpu-daemon-{os.getpid()}-{camera[0]}-{mode}"
  shm = Path("/dev/shm") / f"msgq_{prefix}"
  shm.mkdir()
  parameters = destination / "params" / prefix
  parameters.mkdir(parents=True)
  worker_log = destination / "workers.jsonl"
  environment = dict(
    os.environ,
    OPENPILOT_PREFIX=prefix,
    PARAMS_ROOT=str(parameters.parent),
    LOGPRINT="info",
    SEND_RAW_PRED="1",
    USBGPU_WORKER_TEST_LOG=str(destination / "inputs.jsonl"),
    USBGPU_WORKER_TEST_LIFECYCLE_LOG=str(worker_log),
  )
  environment.pop("SIMULATION", None)
  if mode == "disconnect":
    environment["USBGPU_WORKER_TEST_DISCONNECT"] = "1"
  if mode == "retry":
    environment["USBGPU_WORKER_TEST_RETRY_FILE"] = str(destination / "attempts")
    environment["USBGPU_WORKER_TEST_RETRY_ERROR"] = "PCIe power on failed"
  if mode == "cancel":
    environment["USBGPU_WORKER_TEST_STARTUP_PAUSE"] = "1"
  stride, y_height, _, size = get_nv12_info(*camera)
  processes = []
  commands = []
  packets = []
  report = {"camera": camera, "mode": mode, "passed": False}
  try:
    peer_argv = [str(args.peer), *map(str, (*camera, stride, stride * y_height, size)), "dual"]
    peer = subprocess.Popen(peer_argv, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0)
    processes.append(peer)
    assert peer.stdin is not None and peer.stdout is not None
    peer_lines = Lines(peer.stdout, destination / "peer.log")
    peer_lines.until(("READY",))

    def command(value: str) -> str:
      peer.stdin.write((value + "\n").encode())
      peer.stdin.flush()
      response = peer_lines.until(("OK", "TIMEOUT", "ERR"))
      commands.append({"command": value, "response": response})
      return response

    cp = car.CarParams.new_message(brand="owned", longitudinalActuatorDelay=0.0)
    for key, value in {
      "CarParams": cp.to_bytes(),
      "VEgoStopping": b"5",
      "CameraYawTrimDeg": b"0",
      "UseWideCamera": b"1",
      "SteerActuatorDelay": b"0",
      "LatSmoothSec": b"0",
      "LongActuatorDelay": b"30",
    }.items():
      (parameters / key).write_bytes(value)
    argv = [str(args.binary), str(args.catalog), str(destination), str(args.assets), str(args.worker), "2"]
    daemon = subprocess.Popen(argv, env=environment, stderr=subprocess.PIPE, bufsize=0)
    processes.append(daemon)
    daemon_lines = Lines(daemon.stderr, destination / "daemon.log")
    if mode == "cancel":
      deadline = time.monotonic() + 30
      while not worker_log.exists() and time.monotonic() < deadline:
        time.sleep(0.01)
      assert worker_log.exists(), "worker did not start"
      daemon.send_signal(signal.SIGTERM)
      assert daemon.wait(timeout=5) == 0
      report["final"] = flags(parameters)
      assert not report["final"]["UsbGpuLoading"]
      assert not report["final"]["UsbGpuActive"] and not report["final"]["UsbGpuStartupFailed"]
    else:
      daemon_lines.until(("modeld got CarParams",))
      initial = flags(parameters)
      assert initial["UsbGpuActive"] and initial["UsbGpuLoading"] and initial["UsbGpuCompiled"]
      for topic in OUTPUTS:
        assert command(f"receive {topic} 50 {destination / 'unexpected.bin'}") == "TIMEOUT"
      image = destination / "frame.bin"
      image.write_bytes(bytes(size))
      for frame in (2, 3):
        assert command(f"send {frame} {image} {image}") == "OK"
        for topic in OUTPUTS:
          path = destination / f"{topic}-{frame}.bin"
          assert command(f"receive {topic} 30000 {path}") == "OK"
          with log.Event.from_bytes(path.read_bytes()) as packet:
            assert packet.which() == topic and getattr(packet, topic).frameId == frame
            if topic == "modelV2":
              raw = bytes(packet.modelV2.rawPredictions)
              assert raw and all(value == value and abs(value) < float("inf") for (value,) in struct.iter_unpack("<f", raw))
              if frame == 2 or mode == "retry":
                assert raw == struct.pack("<f", float(frame - 1)) + bytes(len(raw) - 4)
              packets.append({"frame": frame, "raw_bytes": len(raw)})
        state = flags(parameters)
        deadline = time.monotonic() + 2
        while state["UsbGpuLoading"] and time.monotonic() < deadline:
          time.sleep(0.001)
          state = flags(parameters)
        assert not state["UsbGpuLoading"]
        assert state["UsbGpuActive"] == (frame == 2 or mode == "retry")
        assert state["UsbGpuStartupFailed"] == (frame == 3 and mode == "disconnect")
      assert daemon.wait(timeout=5) == 0
      report.update(initial=initial, final=state, packets=packets)
      if mode == "retry":
        assert (destination / "attempts").read_text() == "2"
    assert command("stop") == "OK" and peer.wait(timeout=5) == 0
    workers = lifecycle(worker_log)
    assert workers and all(not Path(f"/proc/{worker['pid']}").exists() and not Path(str(worker["shared"])).exists() for worker in workers)
    report.update(argv=argv, peer_argv=peer_argv, workers=workers, workers_reaped_and_shared_removed=True, commands=commands, passed=True)
  finally:
    for index, process in reversed(list(enumerate(processes))):
      stop(process)
      stream = process.stderr if process.stderr is not None else process.stdout
      if stream is not None:
        (destination / f"process-{index}-tail.log").write_bytes(stream.read())
    shutil.rmtree(shm)
    for path in Path("/tmp").glob(f"{prefix}*"):
      if path.is_socket():
        path.unlink()
    (destination / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps({"camera": camera, "mode": mode, "passed": report["passed"]}), flush=True)


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  for name in ("binary", "worker", "peer", "catalog", "model", "runtime", "installed", "assets", "evidence"):
    parser.add_argument(f"--{name}", type=Path, required=True)
  parser.add_argument("--mode", choices=("disconnect", "retry", "cancel"), action="append")
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=False)
  args.marker = json.loads(args.installed.read_text())
  args.marker["catalog_url"] = "https://owned.invalid/precompiled.json"
  args.marker["pickle"]["url"] = "https://owned.invalid/model.pkl"
  args.marker["runtime"] = {"sha256": RUNTIME_SHA, "size": args.runtime.stat().st_size, "url": "https://owned.invalid/runtime.tar.gz"}
  args.marker["runtime_directory"] = f"runtime-{RUNTIME_SHA[:16]}"
  assert args.marker["pickle"]["sha256"] == MODEL_SHA
  for camera in ((1344, 760), (1928, 1208)):
    for mode in args.mode or ("disconnect", "retry", "cancel"):
      scenario(args, camera, mode)


if __name__ == "__main__":
  main()
