#!/usr/bin/env python3
"""Native calibration IPC, saved Params, nonblocking persistence and signal QA."""
from __future__ import annotations

import argparse
import fcntl
import json
from pathlib import Path
import signal
import subprocess
import time

from calibration_ipc import peer
from calibration_reference import compare_packet
from openpilot.cereal import car, log


def cadence(binary: Path, output: Path) -> dict:
    with peer(binary, output / "cadence", {"simulation": False, "explicit_root": False}) as client:
        initial = client.start()
        assert initial["valid"] is False
        started = time.monotonic()
        for index in range(1, 31):
            time.sleep(max(0., started + index * .05 - time.monotonic()))
            client.step(index, valid=not 15 <= index <= 20)
        intervals = [(b - a) / 1e9 for a, b in zip(client.timestamps, client.timestamps[1:], strict=False)]
        assert all(.18 < value < .35 for value in intervals), intervals
        # Source SubMaster increments frames on each 100ms timeout and still publishes frame%5.
        for _ in range(5):
            time.sleep(.1)
            client.source_step([], time.monotonic())
        silent = client.receive(client.sent[-1])
        assert silent["valid"] is False
        silence_interval = (client.timestamps[-1] - client.timestamps[-2]) / 1e9
        assert .4 < silence_interval < .7, silence_interval
        latency = client.signal(signal.SIGTERM)
        return {"nominal_intervals_seconds": intervals, "timeout_interval_seconds": silence_interval,
                "signal_exit_seconds": latency, "queue_files": client.queue_files, "fields": client.fields}


def persistence(binary: Path, output: Path) -> tuple[dict, bytes]:
    with peer(binary, output / "persistence", {}) as client:
        client.start()
        lock = (client.root / ".lock").open("rb")
        try:
            blocked_start = 0.
            for index in range(1, 521):
                if index == 490:
                    fcntl.flock(lock, fcntl.LOCK_EX)
                    blocked_start = time.monotonic()
                time.sleep(.005)
                client.step(index)
                if index == 510:
                    blocked_elapsed = time.monotonic() - blocked_start
                    assert not (client.directory / "CalibrationParams").exists()
                    assert blocked_elapsed < 2, "durable writer blocked publication cadence"
                    fcntl.flock(lock, fcntl.LOCK_UN)
            deadline = time.monotonic() + 2
            path = client.directory / "CalibrationParams"
            while not path.exists() and time.monotonic() < deadline:
                time.sleep(.01)
            saved_bytes = path.read_bytes()
            (client.destination / "saved.capnp").write_bytes(saved_bytes)
            with log.Event.from_bytes(saved_bytes) as actual, log.Event.from_bytes(client.params.writes[-1]) as expected:
                wanted = expected.to_dict()
                wanted["logMonoTime"] = actual.logMonoTime
                fields = compare_packet(actual.to_dict(), wanted)
            assert len(client.params.writes) == 1
            client.signal(signal.SIGINT)
            return {"frames": 520, "fields": client.fields, "persisted_fields": fields,
                    "blocked_write_publication_window_seconds": blocked_elapsed, "result": "pass"}, saved_bytes
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)
            lock.close()


def freeze_and_reload(binary: Path, output: Path, saved: bytes) -> dict:
    with peer(binary, output / "freeze-reload", {"saved": saved}) as client:
        restored = client.start()
        assert restored["liveCalibration"]["validBlocks"] == 5
        client.set_trim("0.0001")  # std::stof promotes slightly below the freeze boundary.
        for index in range(1, 101):
            time.sleep(.005)
            client.step(index, yaw=.01)
        previous = client.calibrator.rpy.copy()
        client.set_trim("0.00010000000474974513")  # Float32 promotion is above the strict boundary.
        for index in range(101, 201):
            time.sleep(.005)
            client.step(index, yaw=-.01)
        assert (client.calibrator.rpy == previous).all()
        assert client.calibrator.idx == 0 and client.calibrator.block_idx == 1
        client.set_trim("0")
        for index in range(201, 301):
            time.sleep(.005)
            client.step(index, yaw=-.01)
        assert client.calibrator.block_idx == 2
        client.signal(signal.SIGTERM)
        return {"frames": 300, "fields": client.fields, "freeze_raw_params": ["0.0001", "0.00010000000474974513"], "result": "pass"}


def signals_and_bound(binary: Path, output: Path, saved: bytes) -> list[dict]:
    results = []
    for signum in (signal.SIGINT, signal.SIGTERM):
        for start in (False, True):
            with peer(binary, output / f"{signum.name}-{'ipc' if start else 'CarParams'}", {}) as client:
                if start:
                    client.start()
                else:
                    time.sleep(.15)
                    assert client.subscriber.receive(non_blocking=True) is None
                    assert client.process.poll() is None
                results.append({"signal": signum.name, "waiting": "IPC" if start else "CarParams",
                                "exit": 0, "seconds": client.signal(signum)})
    not_car_saved = log.Event.new_message()
    payload = not_car_saved.init("liveCalibration")
    payload.rpyCalib, payload.validBlocks = [.01, .02, .03], 0
    payload.wideFromDeviceEuler, payload.height = [.01, .02, .03], [1.22]
    for not_car in (False, True):
        cache = not_car_saved.to_bytes() if not_car else saved
        with peer(binary, output / f"bounded-{not_car}", {"frames": 2, "not_car": not_car, "saved": cache}) as client:
            initial = client.start()
            assert initial["liveCalibration"]["rpyCalib"] == [0., 0., 0.]
            for index in range(1, 6):
                time.sleep(.005)
                client.step(index)
            assert client.process.wait(timeout=2) == 0
            results.append({"bounded_publications": len(client.timestamps), "not_car": not_car, "exit": 0, "fields": client.fields})
    with peer(binary, output / "corrupt-cache", {"saved": b"invalid saved calibration", "frames": 1}) as client:
        packet = client.start()
        assert packet["liveCalibration"]["validBlocks"] == 0
        assert client.process.wait(timeout=2) == 0
        assert "Error reading cached CalibrationParams" in (client.destination / "daemon.log").read_text()
        results.append({"corrupt_cache": "source defaults", "exit": 0})
    with peer(binary, output / "invalid-saved-array", {}) as client:
        invalid = log.Event.new_message()
        payload = invalid.init("liveCalibration")
        payload.rpyCalib, payload.validBlocks = [0., 0.], 5
        invalid_bytes = invalid.to_bytes()
        client.params.saved = invalid_bytes
        try:
            client.source.Calibrator(param_put=True)
        except IndexError:
            pass
        else:
            raise AssertionError("original source must reject this finite malformed saved array")
        client.put("CalibrationParams", invalid_bytes)
        client.put("CarParams", car.CarParams.new_message().to_bytes())
        assert client.process.wait(timeout=2) == 1
        assert client.subscriber.receive(non_blocking=True) is None
        assert "saved yaw missing" in (client.destination / "daemon.log").read_text()
        results.append({"invalid_saved_rpy": "source IndexError / Rust typed failure", "exit": 1})
    return results


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.binary = args.binary.resolve()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    cli_checks = []
    for options in (["--frames", "0"], ["--frames"], ["--frames", "-1"], ["--unknown"]):
        result = subprocess.run([args.binary, *options], capture_output=True, timeout=3)
        assert result.returncode == 1
        cli_checks.append({"args": options, "exit": result.returncode, "stderr": result.stderr.decode()})
    report = {"cli": cli_checks, "cadence": cadence(args.binary, args.output)}
    report["persistence"], saved = persistence(args.binary, args.output)
    report["freeze"] = freeze_and_reload(args.binary, args.output, saved)
    report["signals_and_bound"] = signals_and_bound(args.binary, args.output, saved)
    report["result"] = "pass"
    report["scope"] = "host native IPC and original source oracle; no device or performance acceptance"
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
