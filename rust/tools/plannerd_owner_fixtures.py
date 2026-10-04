"""Synthetic owner scenarios encoded using the original full cereal schema."""

import hashlib
from pathlib import Path
import re

import numpy as np
from openpilot.cereal import car
from openpilot.selfdrive.modeld.constants import ModelConstants

SERVICES = ['carControl', 'carState', 'controlsState', 'liveParameters', 'radarState', 'liveTracks', 'modelV2', 'selfdriveState', 'carrotMan', 'livePose']


def parameters():
  path = Path(__file__).resolve().parents[2] / 'openpilot/common/params_keys.h'
  values = dict(re.findall(r'\{"([^"]+)", \{[^\n]*?, (?:INT|FLOAT), "([^"\n]*)"\}\}', path.read_text()))
  values.update(CruiseCoastingPercent='5', CruiseEcoControl='0', LeadAccelResponse='3', UseLaneLineSpeed='30', AdjustLaneOffset='12', MyDrivingModeAuto='1')
  return values


def car_params(brand='hyundai'):
  return car.CarParams.new_message(
    brand=brand,
    flags=16 if brand == 'volkswagen' else 0,
    wheelbase=2.75,
    centerToFront=1.12,
    mass=1750.0,
    tireStiffnessRear=200000.0,
    openpilotLongitudinalControl=True,
    radarDelay=0.1,
    radarUnavailable=False,
  )


def save(output, message):
  builder = message.as_builder() if hasattr(message, 'as_builder') else message.as_reader().as_builder()
  data = builder.to_bytes()
  path = output / (hashlib.sha256(data).hexdigest() + '.bin')
  if not path.exists():
    path.write_bytes(data)
  return str(path)


def model(value, i, speed):
  time = np.asarray(ModelConstants.T_IDXS)
  stopping = 120 <= i < 220
  bend = 0.002 * np.sin(i * 0.02)
  velocity = np.maximum(0.0, speed - (time * 2 if stopping else np.zeros(33)))
  x = np.minimum(speed * time, 35.0) if stopping else speed * time
  y = bend * x * x
  zeros = np.zeros(33).tolist()
  for name, xs, ys, zs in [
    ('position', x, y, np.zeros(33)),
    ('velocity', velocity, np.zeros(33), np.zeros(33)),
    ('acceleration', np.full(33, -2.0 if stopping else 0.1), np.zeros(33), np.zeros(33)),
    ('orientation', np.zeros(33), np.zeros(33), 2 * bend * x),
    ('orientationRate', np.zeros(33), np.zeros(33), 2 * bend * velocity),
  ]:
    data = getattr(value, name)
    data.x, data.y, data.z, data.t = xs.tolist(), ys.tolist(), zs.tolist(), time.tolist()
  value.laneLineProbs, value.laneLineStds, value.roadEdgeStds = [0.1, 0.95, 0.97, 0.1], [0.2, 0.1, 0.1, 0.2], [0.2, 0.3]
  for name, offsets in [('laneLines', (-5.4, -1.8, 1.8, 5.4)), ('roadEdges', (-5.0, 5.0))]:
    for line, offset in zip(value.init(name, len(offsets)), offsets, strict=True):
      line.x, line.y, line.z, line.t = x.tolist(), (y + offset).tolist(), zeros, time.tolist()
  value.meta.desireState = [0.0, 0.0, 0.0, 0.8, 0.2, 0.0, 0.0, 0.0]
  value.meta.desirePrediction = [0.0] * 32
  value.meta.laneWidthLeft, value.meta.laneWidthRight = 3.6, 3.4
  value.meta.disengagePredictions.gasPressProbs = [0.2, 0.7]
  value.meta.laneChangeState = 'laneChangeStarting' if 250 <= i < 285 else 'off'
  value.meta.laneChangeDirection = 'left' if 250 <= i < 285 else 'none'
  value.meta.desire = 'laneChangeLeft' if 250 <= i < 285 else 'none'
  value.action.desiredAcceleration = -0.2
  value.action.desiredVelocity = speed
  value.action.shouldStop = stopping and speed < 0.3
  lead = value.init('leadsV3', 1)[0]
  lead.prob, lead.x, lead.v, lead.xStd, lead.yStd, lead.vStd = 0.98, [36.0], [0.0], [1.0], [0.2], [0.3]
  value.frameId = i


def messages(factory, i, now):
  speed = 15.0 if i < 120 else max(0.0, 15.0 - (i - 120) * 0.3) if i < 180 else 0.0 if i < 220 else 20.0
  timestamp = int(now * 1e9)
  result = {service: factory(service, logMonoTime=timestamp, valid=True) for service in SERVICES if service != 'liveTracks'}
  state = result['carState'].carState
  state.vEgo, state.vEgoCluster, state.aEgo = speed, speed, -2.0 if 120 <= i < 170 else 0.1
  state.vCluRatio, state.vCruise, state.vCruiseCluster = 1.0, 72.0, 72.0
  state.standstill = speed == 0.0
  state.useLaneLineSpeed = 30.0
  state.leftBlinker = 250 <= i < 285
  state.gasPressed = 330 <= i < 335
  state.brakePressed = 340 <= i < 345
  state.softHoldActive = 2 if 190 <= i < 205 else 0
  controls = result['controlsState'].controlsState
  controls.longControlState = 'off' if i < 4 or 335 <= i < 338 else 'stopping' if speed < 0.3 else 'pid'
  controls.curvature, controls.desiredCurvature = float(0.002 * np.sin(i * 0.02)), float(0.002 * np.sin(i * 0.02 + 0.2))
  controls.forceDecel = 345 <= i < 350
  selfdrive = result['selfdriveState'].selfdriveState
  selfdrive.enabled = i >= 4
  selfdrive.experimentalMode = 290 <= i < 325
  selfdrive.personality = ['aggressive', 'standard', 'relaxed', 'moreRelaxed'][(i // 70) % 4]
  model(result['modelV2'].modelV2, i, speed)
  radar = result['radarState'].radarState
  for role, index in [('leadOne', 0), ('leadTwo', 1)]:
    lead = getattr(radar, role)
    lead.status, lead.radar, lead.radarTrackId = 45 <= i < 120 or 235 <= i < 290, True, 35 + index
    lead.dRel, lead.yRel, lead.vRel = 40.0 + index * 50 + 0.05 * i, 0.1 if index == 0 else 3.6, 0.5
    lead.vLead = lead.vLeadK = speed + 0.5
    lead.aLead = lead.aLeadK = 0.4 if i % 80 < 40 else -0.7
    lead.aLeadTau, lead.modelProb, lead.jLead = 1.5, 0.99, -0.2
  cutin = radar.leadCutInRisk
  cutin.status, cutin.radar = 280 <= i < 290, True
  cutin.dRel, cutin.vRel, cutin.score = 20.0, -4.0, 0.7
  nav = result['carrotMan'].carrotMan
  nav.desiredSpeed, nav.activeCarrot, nav.xDistToTurn = 100.0, 2, 500.0
  nav.vTurnSpeed = int(75.0 * np.sin(i * 0.03))
  nav.trafficState = 1 if i < 210 else 2
  if 310 <= i < 320:
    nav.desiredSpeed, nav.xDistToTurn, nav.atcType = 40.0, 50.0, 'turn left'
  pose = result['livePose'].livePose
  pose.inputsOK = pose.sensorsOK = True
  pose.angularVelocityDevice.valid = True
  pose.angularVelocityDevice.z = -0.04 if state.leftBlinker else 0.0
  if i in (100, 101, 270):
    result['radarState'].valid = False
  if i == 271:
    result['livePose'].logMonoTime -= 160_000_000
  return result
