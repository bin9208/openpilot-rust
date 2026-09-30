from __future__ import annotations

import argparse
import json
import os
import select
import shutil
import signal
import subprocess
import time
from pathlib import Path

import numpy as np

from check_pipeline_reference import execute, write_tensor
from model_export.original import DriverArtifacts, driver
from model_output_reference import compare, new_message, original_functions
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST, build_header
from openpilot.common.transformations.camera import _ar_ox_fisheye, _os_fisheye
from openpilot.common.transformations.model import dmonitoringmodel_intrinsics
from openpilot.system.camerad.cameras.nv12_info import get_nv12_info


class Lines:
    def __init__(self, stream, destination: Path):
        self.stream = stream
        self.pending = b""
        self.destination = destination

    def until(self, expected: tuple[str, ...], timeout: float = 60) -> str:
        deadline = time.monotonic() + timeout
        while True:
            if b"\n" in self.pending:
                line, self.pending = self.pending.split(b"\n", 1)
                text = line.decode(errors="replace")
                with self.destination.open("a") as output:
                    output.write(text + "\n")
                if any(token in text for token in expected):
                    return text
                continue
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([self.stream], [], [], remaining)[0]:
                raise TimeoutError(f"waiting for {expected}; see {self.destination}")
            chunk = os.read(self.stream.fileno(), 65536)
            if not chunk:
                raise RuntimeError(f"unexpected process EOF waiting for {expected}; see {self.destination}")
            self.pending += chunk


def stop(process: subprocess.Popen) -> None:
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def build_peer(root: Path, output: Path) -> None:
    source = root / "msgq_repo/msgq"
    (output.parent / "services.h").write_text(build_header())
    files = [source / name for name in ("ipc.cc", "event.cc", "impl_msgq.cc", "impl_fake.cc", "msgq.cc",
                                       "visionipc/visionipc.cc", "visionipc/visionipc_client.cc",
                                       "visionipc/visionipc_server.cc", "visionipc/visionbuf.cc")]
    subprocess.run(["g++", "-std=c++17", "-pthread", "-I" + str(source.parent), "-I" + str(output.parent),
                    root / "rust/tools/driver_daemon_peer.cc", *files, "-o", output], check=True)


def check(args, width: int, height: int, raw: bool) -> dict:
    destination = args.output / f"{width}-raw-{int(raw)}"
    destination.mkdir(parents=True, exist_ok=False)
    original = driver(DriverArtifacts(args.models / "dmonitoring_model_tinygrad.pkl",
                                     args.models / f"dm_warp_{width}x{height}_tinygrad.pkl",
                                     args.models / "dmonitoring_model_metadata.pkl"))
    _, reference = original_functions()
    camera = _os_fisheye if width == _os_fisheye.width else _ar_ox_fisheye
    transform = np.linalg.inv(dmonitoringmodel_intrinsics @ np.linalg.inv(camera.intrinsics)).astype(np.float32)
    stride, y_height, _, size = get_nv12_info(width, height)
    prefix = f"driver-daemon-{os.getpid()}-{width}-{int(raw)}"
    (Path("/dev/shm") / f"msgq_{prefix}").mkdir()
    environment = dict(os.environ, OPENPILOT_PREFIX=prefix)
    environment.pop("SEND_RAW_PRED", None)
    if raw:
        environment["SEND_RAW_PRED"] = "0"
    processes = []
    try:
        daemon = subprocess.Popen([args.binary, "--trusted-catalog", args.catalog], env=environment,
                                  stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, bufsize=0)
        processes.append(daemon)
        daemon_lines = Lines(daemon.stderr, destination / "daemon.log")
        daemon_lines.until(("connecting to driver stream",))
        time.sleep(0.15)
        assert daemon.poll() is None, "daemon must wait for camera availability"
        peer = subprocess.Popen([args.peer, *map(str, (width, height, stride, stride * y_height, size))], env=environment,
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0)
        processes.append(peer)
        peer_lines = Lines(peer.stdout, destination / "peer.log")
        ready = peer_lines.until(("READY",))
        header_bytes = int(ready.split()[1])
        daemon_lines.until(("driver stream connected",))

        def command(value: str) -> str:
            peer.stdin.write((value + "\n").encode())
            peer.stdin.flush()
            return peer_lines.until(("OK", "TIMEOUT"))

        # Native msgq reconnects an existing reader after publisher initialization.
        # Drain this startup frame before testing the established subscription.
        warmup = destination / "warmup.bin"
        warmup.write_bytes(bytes(size))
        assert command(f"send 10 {warmup}") == "OK"
        daemon_lines.until(("models loaded",))
        queue_sizes = {name: (Path("/dev/shm") / f"msgq_{prefix}" / name).stat().st_size - header_bytes
                       for name in ("liveCalibration", "driverStateV2")}
        assert queue_sizes == {name: SERVICE_LIST[name].queue_size for name in queue_sizes}, queue_sizes
        command(f"receive 5000 {destination / 'warmup-message.bin'}")
        assert command(f"receive 200 {destination / 'unexpected.bin'}") == "TIMEOUT"
        random = np.random.default_rng(20260930)
        calibrations = [None, [0.01, -0.02, 0.03], None, [0.04]] if raw else [None]
        calibration = np.zeros((1, 3), dtype=np.float32)
        fields = 0
        previous_timestamp = 0
        for index, updated in enumerate(calibrations):
            if updated is not None:
                message = new_message("liveCalibration", valid=False)
                message.liveCalibration.rpyCalib = updated
                path = destination / f"calib-{index}.bin"
                path.write_bytes(message.to_bytes())
                assert command(f"calib {path}") == "OK"
                calibration[0, :] = updated
            frame = random.integers(0, 256, size, dtype=np.uint8)
            path = destination / f"frame-{index}.bin"
            frame.tofile(path)
            write_tensor(original.bindings.inputs["frame"], frame)
            write_tensor(original.bindings.inputs["transform"], transform)
            write_tensor(original.bindings.inputs["calib"], calibration)
            for stage in original.stages:
                execute(stage)
            output = original.bindings.outputs["model"].numpy().reshape(-1).copy()
            values = {name: output[None, start:end] for name, (start, end) in original.metadata.output_slices.items()}
            parsed = reference.parse_model_output(values)
            parsed["raw_pred"] = output.tobytes() if raw else b""
            start = time.monotonic_ns()
            assert command(f"send {index + 11} {path}") == "OK"
            packet = destination / f"message-{index}.bin"
            assert command(f"receive 30000 {packet}") == "OK"
            end = time.monotonic_ns()
            with log.Event.from_bytes(packet.read_bytes()) as message:
                actual = message.to_dict()
            timestamp = actual["logMonoTime"]
            result = actual["driverStateV2"]
            assert start <= timestamp <= end and timestamp > previous_timestamp
            previous_timestamp = timestamp
            assert 0 < result["gpuExecutionTime"] <= result["modelExecutionTime"] < 30
            expected = reference.get_driverstate_packet(parsed, index + 11, 0, result["modelExecutionTime"], result["gpuExecutionTime"])
            expected.logMonoTime = timestamp
            fields += compare(expected.to_dict(), actual)
            assert command(f"receive 200 {destination / 'unexpected.bin'}") == "TIMEOUT", "no frame must mean no publication"
        if raw:
            malformed = new_message("liveCalibration", valid=True)
            malformed.liveCalibration.rpyCalib = [0.1, 0.2]
            path = destination / "invalid-calib.bin"
            path.write_bytes(malformed.to_bytes())
            assert command(f"calib {path}") == "OK"
            assert command(f"send 90 {destination / 'frame-0.bin'}") == "OK"
            assert daemon.wait(timeout=10) == 1
            daemon_lines.until(("calibration must broadcast",))
        else:
            daemon.send_signal(signal.SIGTERM)
            assert daemon.wait(timeout=5) == 0
        assert command("stop") == "OK"
        assert peer.wait(timeout=5) == 0
        waiting = subprocess.Popen([args.binary, "--trusted-catalog", args.catalog], env=environment,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, bufsize=0)
        processes.append(waiting)
        Lines(waiting.stderr, destination / "waiting.log").until(("connecting to driver stream",))
        waiting.send_signal(signal.SIGINT)
        assert waiting.wait(timeout=5) == 0
        report = {"camera": [width, height], "frames": len(calibrations), "raw_predictions": raw, "compared_fields": fields,
                  "queue_sizes": queue_sizes, "queue_size_oracle": "original cereal.services.SERVICE_LIST",
                  "raw_comparison": "exact bytes", "parsed_float_tolerance": 1e-6, "device_validation": False,
                  "scenarios": ["original service queue capacities", "delayed camera", "frame identity", "camera validity ignored as original",
                                "zero calibration", "no-frame timeout", "SIGINT during connect"]
                               + (["invalid-flag calibration update", "retained calibration", "broadcast calibration",
                                                             "malformed calibration exit"] if raw else ["SIGTERM during receive"])}
        (destination / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report), flush=True)
        return report
    finally:
        for process in reversed(processes):
            stop(process)
        for process, name in zip(processes, ("daemon-tail", "peer-tail", "waiting-tail"), strict=False):
            stream = process.stderr if process.stderr is not None else process.stdout
            if stream is not None:
                (destination / f"{name}.log").write_bytes(stream.read())
        shutil.rmtree(Path("/dev/shm") / f"msgq_{prefix}", ignore_errors=True)
        for path in Path("/tmp").glob(f"{prefix}*"):
            if path.is_socket():
                path.unlink()


def main() -> None:
    parser = argparse.ArgumentParser(description="Exercise the actual Rust driver daemon with original native IPC and model oracle")
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--catalog", type=Path, required=True)
    parser.add_argument("--models", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    for key in ("binary", "catalog", "models", "output"):
        setattr(args, key, getattr(args, key).resolve())
    args.output.mkdir(parents=True, exist_ok=False)
    args.peer = args.output / "driver-daemon-peer"
    build_peer(Path(__file__).resolve().parents[2], args.peer)
    reports = [check(args, width, height, raw) for width, height in ((1344, 760), (1928, 1208)) for raw in (True, False)]
    (args.output / "report.json").write_text(json.dumps({"runs": reports, "device_validation": False}, indent=2) + "\n")


if __name__ == "__main__":
    main()
