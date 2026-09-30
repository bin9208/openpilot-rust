# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "zstandard==0.25.0"]
# ///
# Run with the trusted original-model environment, matching its LLVM compiler:
# PYTHONPATH=.:tinygrad_repo:rust/tools uv run rust/tools/check_driving_daemon.py --help
from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import subprocess
import time
from pathlib import Path

import numpy as np

from check_driver_daemon import Lines, stop
from model_output_reference import compare, new_message
from driving_daemon_reference import Oracle, TOPICS
from openpilot.cereal import car, log
from openpilot.cereal.services import SERVICE_LIST, build_header
from openpilot.selfdrive.modeld import fill_model_msg
from openpilot.system.camerad.cameras.nv12_info import get_nv12_info

OUTPUTS = ("modelV2", "drivingModelData", "cameraOdometry")


def build_peer(root: Path, destination: Path) -> None:
    source = root / "msgq_repo/msgq"
    (destination.parent / "services.h").write_text(build_header())
    files = [source / name for name in ("ipc.cc", "event.cc", "impl_msgq.cc", "impl_fake.cc", "msgq.cc", "visionipc/visionipc.cc",
                                       "visionipc/visionipc_client.cc", "visionipc/visionipc_server.cc", "visionipc/visionbuf.cc")]
    subprocess.run(["g++", "-std=c++17", "-pthread", "-I" + str(source.parent), "-I" + str(destination.parent),
                    root / "rust/tools/driving_daemon_peer.cc", *files, "-o", destination], check=True)


def check(args, resolution: tuple[int, int], mode: str):
    destination = args.output / f"{resolution[0]}-{mode}"
    destination.mkdir()
    oracle = Oracle(args.models, resolution, mode)
    raw = mode == "dual"
    fill_model_msg.SEND_RAW_PRED = raw
    stride, y_height, _, size = get_nv12_info(*resolution)
    prefix = f"driving-qa-{os.getpid()}-{resolution[0]}-{mode}"
    shm = Path("/dev/shm") / f"msgq_{prefix}"
    shm.mkdir()
    parameters = destination / "params" / prefix
    parameters.mkdir(parents=True)
    environment = dict(os.environ, OPENPILOT_PREFIX=prefix, PARAMS_ROOT=str(parameters.parent), LOGPRINT='info')
    environment.pop("SEND_RAW_PRED", None)
    environment.pop("SIMULATION", None)
    if raw:
        environment["SEND_RAW_PRED"] = "0"
    processes = []
    capture = None
    try:
        if args.collector is not None:
            from model_logging_capture import Collector
            capture = Collector(args.collector, destination / 'logging', environment)
        waiting = subprocess.Popen([args.binary, "--trusted-catalog", args.catalog], env=environment, stderr=subprocess.PIPE, bufsize=0)
        processes.append(waiting)
        Lines(waiting.stderr, destination / "camera-wait.log").until(("modeld init",))
        time.sleep(.15)
        assert waiting.poll() is None, 'missing cameras must keep the daemon waiting'
        waiting.send_signal(signal.SIGINT)
        assert waiting.wait(timeout=5) == 0
        peer = subprocess.Popen([args.peer, *map(str, (*resolution, stride, stride * y_height, size)), mode], env=environment,
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0)
        processes.append(peer)
        peer_lines = Lines(peer.stdout, destination / "peer.log")
        peer_lines.until(("READY",))

        def command(value: str) -> str:
            peer.stdin.write((value + "\n").encode())
            peer.stdin.flush()
            return peer_lines.until(("OK", "TIMEOUT"))

        zero = destination / "warmup.bin"
        zero.write_bytes(bytes(size))
        waiting = subprocess.Popen([args.binary, "--trusted-catalog", args.catalog], env=environment, stderr=subprocess.PIPE, bufsize=0)
        processes.append(waiting)
        waiting_lines = Lines(waiting.stderr, destination / "params-wait.log")
        waiting_lines.until(("connected extra cam" if mode == "dual" else "connected main cam",))
        waiting_lines.until(("models loaded",))
        assert command(f"send 2 {zero} {zero}") == "OK"
        time.sleep(.15)
        assert waiting.poll() is None, 'missing CarParams must keep the daemon waiting'
        waiting.send_signal(signal.SIGTERM)
        assert waiting.wait(timeout=5) == 0
        cp = car.CarParams.new_message(longitudinalActuatorDelay=0.)
        values = {"CarParams": cp.to_bytes(), "VEgoStopping": b"5", "CameraYawTrimDeg": b"35", "UseWideCamera": b"1",
                  "SteerActuatorDelay": b"0", "LatSmoothSec": b"0", "LongActuatorDelay": b"30"}
        for key, value in values.items():
            (parameters / key).write_bytes(value)
        if mode == "road":
            waiting = subprocess.Popen([args.binary, "--trusted-catalog", args.catalog], env=environment, stderr=subprocess.PIPE, bufsize=0)
            processes.append(waiting)
            Lines(waiting.stderr, destination / "frame-wait.log").until(("modeld got CarParams",))
            waiting.send_signal(signal.SIGTERM)
            assert waiting.wait(timeout=5) == 0
        count = 12 if raw else 4
        daemon = subprocess.Popen([args.binary, "--trusted-catalog", args.catalog, "--frames", str(count + 1)], env=environment,
                                  stderr=subprocess.PIPE, bufsize=0)
        processes.append(daemon)
        daemon_lines = Lines(daemon.stderr, destination / "daemon.log")
        daemon_lines.until(("connected extra cam" if mode == "dual" else "connected main cam",))
        daemon_lines.until(("models loaded",))
        daemon_lines.until(("modeld got CarParams",))
        assert command(f"send 2 {zero} {zero}") == "OK"
        oracle.prepare(2, (np.zeros(size, dtype=np.uint8), np.zeros(size, dtype=np.uint8)))
        for topic in OUTPUTS:
            assert command(f"receive {topic} 200 {destination / 'unexpected.bin'}") == "TIMEOUT"
        random = np.random.default_rng(20260930)
        fields = messages = 0
        trace = []
        for index in range(count):
            frame_id = index + 3 + int(index >= 9)
            events = {name: new_message(name, valid=False) for name in TOPICS if name != "liveCalibration" or index in (1, 4, 8)}
            events["deviceState"].deviceState.deviceType = "tici"
            events["roadCameraState"].roadCameraState.sensor = "os04c10" if resolution[0] == 1344 else "ar0231"
            events["roadCameraState"].roadCameraState.frameId = frame_id + 3
            events["carState"].carState.from_dict({"canValid": True, "vEgo": 5. if index < 8 else 20., "leftBlinker": 1 <= index <= 3,
                                                 "rightBlinker": 6 <= index <= 8, "leftLaneLine": 0, "rightLaneLine": 0})
            events["driverMonitoringState"].driverMonitoringState.isRHD = index >= 6
            events["carControl"].carControl.latActive = True
            events["liveDelay"].liveDelay.lateralDelay = .11 + index * .013
            if "liveCalibration" in events:
                events["liveCalibration"].liveCalibration.from_dict({"rpyCalib": [.001 * index, -.015, .007], "calStatus": "calibrated"})
            oracle.sm.updated = dict.fromkeys(TOPICS, False)
            for topic, event in events.items():
                path = destination / f"{topic}-{index}.bin"
                path.write_bytes(event.to_bytes())
                assert command(f"publish {topic} {path}") == "OK"
                oracle.sm[topic] = getattr(event, topic)
                oracle.sm.updated[topic] = oracle.sm.seen[topic] = True
            images = tuple(random.integers(0, 256, size, dtype=np.uint8) for _ in range(2))
            if mode != "dual":
                images = (images[0], images[0])
            paths = [destination / f"{name}-{index}.bin" for name in ("main", "extra")]
            for image, path in zip(images, paths, strict=True):
                image.tofile(path)
            oracle.prepare(frame_id, images)
            start = time.monotonic_ns()
            assert command(f"send {frame_id} {paths[0]} {paths[1]}") == "OK"
            actual = []
            for topic in OUTPUTS:
                path = destination / f"{topic}-output-{index}.bin"
                response = command(f"receive {topic} {200 if oracle.scope['prepare_only'] else 30000} {path}")
                assert response == ("TIMEOUT" if oracle.scope["prepare_only"] else "OK")
                if response == "OK":
                    with log.Event.from_bytes(path.read_bytes()) as packet:
                        actual.append(packet.to_dict())
            trace.append({"frame_id": frame_id, "prepare_only": oracle.scope["prepare_only"], "desire": int(oracle.scope["desire"]),
                          "is_rhd": oracle.scope["is_rhd"], "packed_sha256": oracle.scope["packed_sha256"]})
            if actual:
                elapsed = actual[0]["modelV2"]["modelExecutionTime"]
                assert 0 < elapsed < 30
                expected = oracle.publications(elapsed)
                for reference, observed in zip(expected, actual, strict=True):
                    assert start <= observed["logMonoTime"] <= time.monotonic_ns()
                    reference.logMonoTime = observed["logMonoTime"]
                    (destination / f"{reference.which()}-expected-{index}.bin").write_bytes(reference.to_bytes())
                    fields += compare(reference.to_dict(), observed)
                    messages += 1
            for topic in OUTPUTS:
                assert command(f"receive {topic} 100 {destination / 'unexpected.bin'}") == "TIMEOUT"
        assert daemon.wait(timeout=5) == 0
        assert command("stop") == "OK" and peer.wait(timeout=5) == 0
        logging_report = capture.finish('modeld', [process.pid for process in processes]) if capture is not None else None
        if capture is not None:
            from model_logging_reference import DrivingScenario, driving
            logging_report['source_calls'] = driving(capture.records['logMessage'],
                DrivingScenario(daemon.pid, resolution, size, mode, (2, *(step['frame_id'] for step in trace))))
        report = {"resolution": resolution, "streams": mode, "frames": count, "messages": messages, "compared_fields": fields,
                  "raw_predictions": raw, "raw_comparison": "exact bytes", "float_tolerance": 1e-6, "polynomial_tolerance": 2e-5,
                  "discrete_fields": "exact", "llvm_path": os.environ.get("LLVM_PATH"), "device_validation": False, "trace": trace,
                  "queue_capacities": {name: int(SERVICE_LIST[name].queue_size) for name in (*TOPICS, *OUTPUTS)},
                  "startup": {"model_loaded_before_car_params": True, "model_loaded_before_first_frame": True,
                              "first_frame_id": 2, "first_frame_prepare_only": True},
                  "signal_checks": ["SIGINT camera discovery", "SIGTERM CarParams wait"] + (["SIGTERM first-frame wait"] if mode == "road" else [])}
        report['logging'] = logging_report
        (destination / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report), flush=True)
        return report
    finally:
        if capture is not None:
            capture.close()
        for index, process in reversed(list(enumerate(processes))):
            stop(process)
            stream = process.stderr if process.stderr is not None else process.stdout
            if stream is not None:
                (destination / f"process-{index}-tail.log").write_bytes(stream.read())
        shutil.rmtree(shm)
        for path in Path("/tmp").glob(f"{prefix}*"):
            if path.is_socket():
                path.unlink()


def main() -> None:
    parser = argparse.ArgumentParser(description="Exercise the Rust driving daemon through original native camera/message transports")
    for name in ("binary", "catalog", "models", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument('--collector', type=Path)
    args = parser.parse_args()
    if args.collector is not None:
        args.collector = args.collector.resolve()
    for name in ("binary", "catalog", "models", "output"):
        setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=False)
    args.peer = args.output / "driving-daemon-peer"
    build_peer(Path(__file__).resolve().parents[2], args.peer)
    scenarios = tuple((resolution, mode) for resolution in ((1344, 760), (1928, 1208)) for mode in ("dual", "road", "wide"))
    reports = [check(args, resolution, mode) for resolution, mode in scenarios]
    (args.output / "report.json").write_text(json.dumps({"runs": reports, "device_validation": False}, indent=2) + "\n")


if __name__ == "__main__":
    main()
