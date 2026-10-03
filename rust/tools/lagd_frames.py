"""Synthetic cereal streams for unchanged main-loop and private IPC checks."""

import math
from openpilot.cereal import car, log

TOPICS = ['livePose', 'liveCalibration', 'carState', 'controlsState', 'carControl']


def car_params(name='SYNTHETIC-LAGD', delay=0.2):
  value = car.CarParams.new_message(carFingerprint=name, steerActuatorDelay=delay)
  return value.to_bytes()


def cached(lag=0.4, blocks=5, status='estimated'):
  message = log.Event.new_message()
  message.init('liveDelay')
  message.liveDelay.lateralDelayEstimate = lag
  message.liveDelay.validBlocks = blocks
  message.liveDelay.status = status
  return message.to_bytes()


def messages(index, mode='normal'):
  time = index * 0.05
  desired = 0.3 * math.sin(time * 5.0) + 0.02 * math.sin(time * 11.0)
  actual = 0.3 * math.sin((time - 0.35) * 5.0) + 0.02 * math.sin((time - 0.35) * 11.0)
  speed = 20.0
  output = []
  for topic in TOPICS:
    event = log.Event.new_message(logMonoTime=round((100.0 + time) * 1e9), valid=True)
    value = event.init(topic)
    if topic == 'carControl':
      value.latActive = not (mode == 'recovery' and 300 <= index < 310)
    elif topic == 'carState':
      value.vEgo = speed if not (mode == 'recovery' and 500 <= index < 505) else 15.0
      value.steeringPressed = mode == 'recovery' and 700 <= index < 710
    elif topic == 'controlsState':
      value.desiredCurvature = desired / speed / speed
      state = value.lateralControlState.init(['pidState', 'angleState', 'debugState', 'torqueState'][index % 4])
      state.saturated = mode == 'recovery' and 900 <= index < 910
    elif topic == 'liveCalibration':
      value.rpyCalib = [0.0, 0.0, 0.0]
      value.calStatus = 'uncalibrated' if mode == 'recovery' and 1100 <= index < 1110 else 'calibrated'
    elif topic == 'livePose':
      value.angularVelocityDevice.z = actual / speed
      value.angularVelocityDevice.zStd = 0.01
      value.angularVelocityDevice.valid = not (mode == 'recovery' and 1150 <= index < 1160)
      value.posenetOK = True
      value.inputsOK = True
      value.orientationNED.valid = False  # deliberately unused by lagd's pose validity policy
      value.sensorsOK = False  # deliberately unused by lagd's pose validity policy
      if mode == 'recovery' and 1200 <= index < 1210:
        event.valid = False
    output.append(event.to_bytes())
  return output
