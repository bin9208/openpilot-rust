"""Disposable original IPC peer for real Rust torqued lifecycle and persistence QA."""

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
from torque_reference import Parameters, original, compare
from torque_cases import car_bytes
from check_message_state import source as messaging_source
from openpilot.cereal import car, log, messaging
from openpilot.cereal.services import SERVICE_LIST

TOPICS = ["carControl", "carOutput", "carState", "liveCalibration", "livePose", "liveDelay"]


class Peer:
  def __init__(self, binary: Path, numerics: Path, destination: Path, configuration: dict):
    self.destination = destination
    destination.mkdir(parents=True, exist_ok=False)
    self.shm = Path(tempfile.mkdtemp(prefix="msgq_torque-qa-", dir="/dev/shm"))
    self.prefix = self.shm.name.removeprefix("msgq_")
    os.environ["OPENPILOT_PREFIX"] = self.prefix
    os.environ.pop("CEREAL_FAKE", None)
    os.environ.pop("ZMQ", None)
    self.publishers = {name: messaging.pub_sock(name) for name in TOPICS}
    self.subscriber = messaging.sub_sock("liveTorqueParameters", timeout=50)
    home = destination / "home"
    self.root = destination / "params" if configuration.get("explicit_root", True) else home / f".comma{self.prefix}" / "params"
    self.directory = self.root / self.prefix
    self.directory.mkdir(parents=True)
    cp = car_bytes()
    self.params = Parameters(cp, configuration.get("saved"))
    self.put("CarParamsPrevRoute", cp)
    if self.params.values["LiveTorqueParameters"] is not None:
      self.put("LiveTorqueParameters", self.params.values["LiveTorqueParameters"])
    self.source = original(self.params)
    with car.CarParams.from_bytes(cp) as cp_message:
      self.estimator = self.source.TorqueEstimator(cp_message)
    scope, environment = messaging_source()
    environment["simulation"] = "1" if configuration.get("simulation", True) else "0"
    self.sm = scope["SubMaster"](TOPICS, poll="livePose")
    self.sent = []
    self.context = {
      **self.source.__dict__,
      "estimator": self.estimator,
      "sm": self.sm,
      "params": self.params,
      "DEBUG": bool(configuration.get("debug", False)),
      "pm": SimpleNamespace(send=lambda topic, packet: self.sent.append(packet)),
    }
    self.fields = 0
    self.timestamps = []
    self.log = (destination / "daemon.log").open("w")
    env = dict(
      os.environ,
      OPENPILOT_PREFIX=self.prefix,
      HOME=str(home),
      OPENBLAS_NUM_THREADS="1",
      SIMULATION=environment["simulation"],
      DEBUG=str(int(configuration.get("debug", False))),
    )
    if configuration.get("explicit_root", True):
      env["PARAMS_ROOT"] = str(self.root)
    else:
      env.pop("PARAMS_ROOT", None)
    arguments = ["--frames", str(configuration["frames"])] if "frames" in configuration else []
    self.process = subprocess.Popen([binary, "--numerics", numerics, *arguments], env=env, stdout=self.log, stderr=self.log)
    for publisher in self.publishers.values():
      publisher.wait_for_readers(timeout=5)
    self.subscriber.receive(non_blocking=True)
    self.queue_files = {name: (self.shm / name).stat().st_size for name in (*self.publishers, "liveTorqueParameters")}
    overheads = {size - SERVICE_LIST[name].queue_size for name, size in self.queue_files.items()}
    assert len(overheads) == 1 and 0 < next(iter(overheads)) < 4096

  def put(self, key: str, data: bytes) -> None:
    temporary = self.directory / f"{key}.qa-tmp"
    temporary.write_bytes(data)
    temporary.replace(self.directory / key)

  def source_step(self, messages: list) -> None:
    self.sm.update = lambda: self.sm.update_msgs(time.monotonic(), [m.as_reader() for m in messages])
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
        self.fields += compare(actual, expected.to_dict())
        return actual
      assert self.process.poll() is None, (self.process.returncode, self.destination / "daemon.log")
    raise TimeoutError(f"no torque publication: {self.destination}")

  def start(self) -> dict:
    self.source_step([])
    self.put("CarParams", car_bytes())
    return self.receive(self.sent[-1])

  def step(self, index: int, valid: bool = True) -> dict | None:
    messages = []
    for topic in TOPICS:
      event = messaging.new_message(topic, valid=valid)
      match topic:
        case "carControl":
          event.carControl.latActive = True
        case "carOutput":
          event.carOutput.actuatorsOutput.torque = -0.25
        case "carState":
          event.carState.vEgo = 16.0
        case "liveCalibration":
          event.liveCalibration.rpyCalib = [0.0, 0.0, 0.0]
        case "liveDelay":
          event.liveDelay.lateralDelay = 0.0
        case "livePose":
          event.livePose.timestamp = event.logMonoTime
          event.livePose.orientationNED.valid = event.livePose.angularVelocityDevice.valid = True
          event.livePose.angularVelocityDevice.z = 0.03125
          event.livePose.inputsOK = event.livePose.sensorsOK = event.livePose.posenetOK = True
      messages.append(event)
    # livePose is the poll wakeup: publish every other updated topic before it.
    for event in sorted(messages, key=lambda m: m.which() == "livePose"):
      self.publishers[event.which()].send(event.to_bytes())
    self.source_step(messages)
    self.publishers["livePose"].wait_for_readers(timeout=2)
    assert self.sm.frame == index
    return self.receive(self.sent[-1]) if index % 5 == 0 else None

  def signal(self, signum: signal.Signals, drain: bool = False) -> float:
    started = time.monotonic()
    self.process.send_signal(signum)
    # Source Params drains durable writes on shutdown; filesystem journal commits can exceed the IPC poll timeout.
    assert self.process.wait(timeout=10 if drain else 2) == 0
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
def peer(binary: Path, numerics: Path, destination: Path, configuration: dict):
  client = Peer.__new__(Peer)
  try:
    client.__init__(binary, numerics, destination, configuration)
    yield client
  finally:
    if hasattr(client, "process"):
      client.close()
