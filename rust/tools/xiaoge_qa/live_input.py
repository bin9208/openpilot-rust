from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, replace
from pathlib import Path
import threading
import time
from typing import Final, Literal

from msgq.visionipc import VisionIpcServer, VisionStreamType
from opendbc.can import CANPacker
from openpilot.cereal import car, messaging
from openpilot.common.params import Params

WIDTH: Final = 128
HEIGHT: Final = 80
STRIDE: Final = 160
UV_OFFSET: Final = STRIDE * HEIGHT + 64
PIXELS: Final = bytes((index * 17 + index // STRIDE * 13) % 256 for index in range(UV_OFFSET + STRIDE * HEIGHT // 2))


@dataclass(frozen=True, slots=True)
class Phase:
  name: str
  direction: Literal["none", "left", "right"] = "left"
  speed: float = 20.0
  lane_width: float = 3.5
  valid: bool = True
  cereal: bool = True
  can: bool = True
  camera: bool = True
  generation: int = 0


class Inputs:
  def __init__(self, output: Path):
    self.output = output
    self.phase = Phase("startup")
    self.lock = threading.Lock()
    self.stop = threading.Event()
    self.ready = threading.Event()
    self.pool = ThreadPoolExecutor(max_workers=1)
    params = Params()
    cp = car.CarParams.new_message()
    cp.brand = "tesla"
    params.put("CarParams", cp.to_bytes())
    (output / "frame.nv12").write_bytes(PIXELS)
    self.future = self.pool.submit(self.run)
    if not self.ready.wait(5):
      self.close()
      raise TimeoutError("real Xiaoge input publishers did not start")

  def select(self, phase: Phase) -> None:
    with self.lock:
      self.phase = phase

  def close(self) -> None:
    self.stop.set()
    try:
      self.future.result(timeout=5)
    finally:
      self.pool.shutdown(wait=True)

  def run(self) -> int:
    publishers = messaging.PubMaster(["carState", "modelV2", "selfdriveState", "can"])
    packer = CANPacker("tesla_model3_party")
    server = None
    generation = -1
    frame = 0
    deadline = time.monotonic()
    with (self.output / "input-phases.jsonl").open("w") as trace:
      previous = None
      while not self.stop.is_set():
        with self.lock:
          phase = replace(self.phase)
        if phase != previous:
          trace.write(f"{time.monotonic_ns()} {phase!r}\n")
          trace.flush()
          previous = phase
        if generation != phase.generation:
          server = None
          server = VisionIpcServer("camerad")
          for stream in [VisionStreamType.VISION_STREAM_ROAD, VisionStreamType.VISION_STREAM_WIDE_ROAD]:
            server.create_buffers_with_sizes(stream, 8, WIDTH, HEIGHT, len(PIXELS), STRIDE, UV_OFFSET)
          server.start_listener()
          generation = phase.generation
          self.ready.set()
        if phase.cereal:
          vehicle = messaging.new_message("carState", valid=phase.valid)
          vehicle.carState.vEgo = phase.speed
          vehicle.carState.steeringAngleDeg = -12.5
          vehicle.carState.leftLatDist = 1.25
          vehicle.carState.leftBlindspot = True
          vehicle.carState.rightBlindspot = False
          publishers.send("carState", vehicle)
          model = messaging.new_message("modelV2", valid=phase.valid)
          model.modelV2.meta.laneChangeDirection = phase.direction
          model.modelV2.meta.laneWidthLeft = phase.lane_width
          model.modelV2.meta.laneWidthRight = phase.lane_width
          model.modelV2.meta.distanceToRoadEdgeLeft = 2.75
          model.modelV2.meta.distanceToRoadEdgeRight = 4.25
          model.modelV2.laneLineProbs = [0.1, 0.7, 0.8, 0.2]
          model.modelV2.orientationRate.z = [0.03, -0.12, 0.1]
          model.modelV2.init("leadsV3", 1)
          model.modelV2.leadsV3[0].x = [18.5]
          model.modelV2.leadsV3[0].y = [-1.25]
          model.modelV2.leadsV3[0].v = [12.0]
          model.modelV2.leadsV3[0].prob = 0.9
          publishers.send("modelV2", model)
          system = messaging.new_message("selfdriveState", valid=phase.valid)
          system.selfdriveState.enabled = True
          system.selfdriveState.active = False
          publishers.send("selfdriveState", system)
        if phase.can:
          address, payload, bus = packer.make_can_msg("DAS_road", 2, {"DAS_stopLineDist": 42.5, "DAS_trafficLightColor": 3})
          packet = messaging.new_message("can", 1, valid=True)
          packet.can[0].address = address
          packet.can[0].dat = payload
          packet.can[0].src = bus
          publishers.send("can", packet)
        if phase.camera and frame % 2 == 0:
          now = time.monotonic_ns()
          for stream in [VisionStreamType.VISION_STREAM_ROAD, VisionStreamType.VISION_STREAM_WIDE_ROAD]:
            server.send(stream, PIXELS, frame, now, now + 1_000_000)
        frame += 1
        deadline += .02
        self.stop.wait(max(0.0, deadline - time.monotonic()))
    return frame
