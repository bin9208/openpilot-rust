"""Disposable original-Python IPC peer for actual Rust calibration daemon QA."""
from __future__ import annotations

from contextlib import contextmanager
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
from types import SimpleNamespace

import numpy as np

from calibration_reference import Parameters, compare_packet, original
from check_message_state import source as messaging_source
from openpilot.cereal import car, log, messaging
from openpilot.cereal.services import SERVICE_LIST


class Peer:
    def __init__(self, binary: Path, destination: Path, configuration: dict):
        self.destination = destination
        destination.mkdir(parents=True, exist_ok=False)
        self.shm = Path(tempfile.mkdtemp(prefix="msgq_calibration-qa-", dir="/dev/shm"))
        self.prefix = self.shm.name.removeprefix("msgq_")
        os.environ["OPENPILOT_PREFIX"] = self.prefix
        os.environ.pop("CEREAL_FAKE", None)
        os.environ.pop("ZMQ", None)
        self.publishers = {name: messaging.pub_sock(name) for name in ("cameraOdometry", "carState")}
        self.subscriber = messaging.sub_sock("liveCalibration", timeout=50)
        home = destination / "home"
        self.root = destination / "params" if configuration.get("explicit_root", True) else home / f".comma{self.prefix}" / "params"
        self.directory = self.root / self.prefix
        self.directory.mkdir(parents=True)
        self.params = Parameters(configuration.get("saved"))
        self.trim = "0"
        self.put("CameraYawTrimDeg", b"0")
        if self.params.saved is not None:
            self.put("CalibrationParams", self.params.saved)
        self.source = original(False, self.params)
        self.calibrator = self.source.Calibrator(param_put=True)
        self.calibrator.not_car = configuration.get("not_car", False)
        scope, environment = messaging_source()
        environment["simulation"] = "1" if configuration.get("simulation", True) else "0"
        self.sm = scope["SubMaster"](["cameraOdometry", "carState"], poll="cameraOdometry")
        self.sent = []
        self.context = {**self.source.__dict__, "calibrator": self.calibrator, "sm": self.sm,
                        "params_reader": self.params, "DEBUG": False,
                        "pm": SimpleNamespace(send=lambda topic, packet: self.sent.append(packet))}
        self.fields = 0
        self.timestamps: list[int] = []
        self.log = (destination / "daemon.log").open("w")
        env = dict(os.environ, OPENPILOT_PREFIX=self.prefix, HOME=str(home),
                   SIMULATION="1" if configuration.get("simulation", True) else "0")
        env.pop("DEBUG", None)
        if configuration.get("explicit_root", True):
            env["PARAMS_ROOT"] = str(self.root)
        else:
            env.pop("PARAMS_ROOT", None)
        arguments = ["--frames", str(configuration["frames"])] if "frames" in configuration else []
        self.process = subprocess.Popen([binary, *arguments], env=env, stdout=self.log, stderr=self.log)
        for publisher in self.publishers.values():
            publisher.wait_for_readers(timeout=5)
        self.subscriber.receive(non_blocking=True)  # reconnect after native publisher initialization
        sizes = {name: (self.shm / name).stat().st_size for name in (*self.publishers, "liveCalibration")}
        overheads = {size - SERVICE_LIST[name].queue_size for name, size in sizes.items()}
        assert len(overheads) == 1 and 0 < next(iter(overheads)) < 4096, sizes
        self.queue_files = sizes

    def put(self, key: str, data: bytes) -> None:
        temporary = self.directory / f"{key}.qa-tmp"
        temporary.write_bytes(data)
        temporary.replace(self.directory / key)

    def set_trim(self, value: str) -> None:
        self.put("CameraYawTrimDeg", value.encode())
        self.trim = value

    def source_step(self, messages: list, now: float) -> None:
        self.params.trim = float(np.float32(self.trim))
        self.sm.update = lambda timeout: self.sm.update_msgs(now, [message.as_reader() for message in messages])
        exec(self.source.loop_body, self.context)

    def receive(self, expected) -> dict:
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            packet = self.subscriber.receive()
            if packet is not None:
                (self.destination / f"message-{len(self.timestamps):04}.capnp").write_bytes(packet)
                with log.Event.from_bytes(packet) as message:
                    actual = message.to_dict()
                timestamp = actual["logMonoTime"]
                assert not self.timestamps or timestamp > self.timestamps[-1]
                assert timestamp <= time.monotonic_ns()
                self.timestamps.append(timestamp)
                expected.logMonoTime = timestamp
                self.fields += compare_packet(actual, expected.to_dict())
                return actual
            assert self.process.poll() is None, (self.process.returncode, self.destination / "daemon.log")
        raise TimeoutError(f"no calibration publication: {self.destination}")

    def start(self) -> dict:
        self.source_step([], time.monotonic())
        cp = car.CarParams.new_message(notCar=self.calibrator.not_car)
        self.put("CarParams", cp.to_bytes())
        return self.receive(self.sent[-1])

    def step(self, index: int, *, yaw=0.0, valid=True) -> dict | None:
        vehicle = messaging.new_message("carState", valid=valid)
        vehicle.carState.vEgo = 10.
        camera = messaging.new_message("cameraOdometry", valid=valid)
        camera.cameraOdometry.trans = [10., float(10. * np.tan(yaw)), 0.]
        camera.cameraOdometry.rot = [0., 0., 0.]
        camera.cameraOdometry.transStd = [0., 0., 0.]
        camera.cameraOdometry.wideFromDeviceEuler = [0.01, 0.02, 0.03]
        camera.cameraOdometry.roadTransformTrans = [0., 0., 1.5]
        camera.cameraOdometry.roadTransformTransStd = [0., 0., 0.]
        self.publishers["carState"].send(vehicle.to_bytes())
        self.publishers["cameraOdometry"].send(camera.to_bytes())
        self.source_step([camera, vehicle], time.monotonic())
        self.publishers["cameraOdometry"].wait_for_readers(timeout=2)
        assert self.sm.frame == index
        if index % 5 == 0:
            return self.receive(self.sent[-1])
        return None

    def signal(self, signum: signal.Signals) -> float:
        started = time.monotonic()
        self.process.send_signal(signum)
        assert self.process.wait(timeout=2) == 0
        return time.monotonic() - started

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=5)
        self.log.close()
        self.publishers.clear()
        self.subscriber = None
        shutil.rmtree(self.shm)


@contextmanager
def peer(binary: Path, destination: Path, configuration: dict):
    client = Peer.__new__(Peer)
    try:
        client.__init__(binary, destination, configuration)
        yield client
    finally:
        if hasattr(client, "process"):
            client.close()
