#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp"]
# ///
# ─── How to run ───
# Imported by: PYTHONPATH=. python rust/tools/check_selfdrived_controller.py --help
# ──────────────────
"""Cereal input sequences for source policy branches and exact frame boundaries."""
from __future__ import annotations

from openpilot.cereal import car, messaging

MODE = {'replay': False, 'simulation': True, 'testing_closet': False, 'device_type': 'tici', 'nvme_present': False, 'branch': 'fixture'}
TOPICS = ('deviceState', 'pandaStates', 'peripheralState', 'modelV2', 'liveCalibration', 'carOutput', 'driverMonitoringState',
          'longitudinalPlan', 'livePose', 'managerState', 'liveParameters', 'radarState', 'liveTorqueParameters', 'carrotMan',
          'controlsState', 'carControl', 'driverAssistance', 'alertDebug', 'userBookmark', 'audioFeedback', 'roadCameraState')


def event(topic, **values):
  message = messaging.new_message(topic, 1 if topic == 'pandaStates' else None)
  message.valid = True
  message.logMonoTime = 12345
  if topic == 'pandaStates':
    message.pandaStates[0].from_dict(values)
  else:
    getattr(message, topic).from_dict(values)
  return list(message.to_bytes())


def car_state(**values):
  data = {'canValid': True, 'gearShifter': 'drive', 'cruiseState': {'available': True}, 'vEgo': 20.0, 'vCruise': 100.0}
  data.update(values)
  return list(car.CarState.new_message(**data).to_bytes())


def healthy_messages(overrides=None, invalid=(), omit=()):
  fields = {'pandaStates': {'safetyModel': 'toyota', 'controlsAllowed': True}, 'liveCalibration': {'calStatus': 'calibrated', 'rpyCalib': [0., 0., 0.]},
            'liveParameters': {'valid': True}, 'livePose': {'inputsOK': True, 'posenetOK': True},
            'deviceState': {'freeSpacePercent': 50.}, 'modelV2': {'velocity': {'x': [20.]}}}
  fields.update(overrides or {})
  result = []
  for topic in TOPICS:
    if topic in ('alertDebug', 'userBookmark', 'audioFeedback') or topic in omit:
      continue
    raw = event(topic, **fields.get(topic, {}))
    if topic in invalid:
      from openpilot.cereal import log
      with log.Event.from_bytes(bytes(raw)) as reader:
        message = reader.as_builder()
        message.valid = False
        raw = list(message.to_bytes())
    result.append(raw)
  return result


def cases():
  rows = []
  def parameter(key, value):
    rows.append({'operation': 'parameter', 'key': key, 'bytes': list(value) if value is not None else None, 'directory': False})
  def init(**changes):
    ready = changes.pop('ready', False)
    mode = MODE.copy()
    mode.update(changes.pop('mode', {}))
    cp_values = {'brand': 'other', 'networkLocation': 'fwdCamera', 'pcmCruise': True, 'openpilotLongitudinalControl': True,
                 'alphaLongitudinalAvailable': True, 'safetyConfigs': [{'safetyModel': 'toyota'}]}
    cp_values.update(changes)
    cp = car.CarParams.new_message(**cp_values)
    rows.append({'operation': 'init', 'cp': list(cp.to_bytes()), 'mode': mode, 'language': 'en', 'health_simulation': mode['simulation']})
    if ready:
      step(count=602)
  now = 10.
  def step(messages=None, count=1, dt=.01, lagging=False, **current):
    nonlocal now
    row = {'operation': 'step' if count == 1 else 'advance', 'current': car_state(**current),
           'messages': healthy_messages() if messages is None else messages, 'now': now, 'lagging': lagging}
    if count != 1:
      row.update(count=count, dt=dt)
    rows.append(row)
    now += count * dt
  for key, value in [('DisableDM', b'1'), ('UseWideCamera', b'0'), ('LongitudinalPersonality', b'1')]:
    parameter(key, value)
  init(mode={'simulation': False})
  for frame in range(3):
    rows.append({'operation': 'step', 'current': car_state(canValid=False), 'messages': [], 'now': 10.0+frame*.01, 'lagging': False})
  init()
  step()
  step(cruiseState={'available': True, 'enabled': True})
  step(cruiseState={'available': True, 'enabled': True}, steeringPressed=True)
  step(count=205, cruiseState={'available': True, 'enabled': True})
  parameter('DisengageOnAccelerator', b'1')
  step(gasPressed=True)
  step(brakePressed=True, standstill=True)
  step(brakePressed=True, standstill=False)
  step(regenBraking=True, standstill=False)
  for cp in ({'brand': 'mock'}, {'passive': True}, {'secOcRequired': True},
             {'mode': {'device_type': 'mici'}}, {'mode': {'nvme_present': True}},
             {'alphaLongitudinalAvailable': False, 'openpilotLongitudinalControl': False},
             {'notCar': True}, {'pcmCruise': False}):
    init(**cp)
    step(buttonEvents=[{'type': 'resumeCruise', 'pressed': True}], vCruise=251.)
  for wide in (b'0', b'1'):
    parameter('UseWideCamera', wide)
    for available in ([], [0], [0, 2]):
      rows.append({'operation': 'streams', 'values': available})
      init(mode={'replay': True})
      step()
  parameter('UseWideCamera', b'0')
  rows.append({'operation': 'streams', 'values': [0]})
  init(mode={'simulation': False})
  step(messages=[], canValid=False, count=600)
  step(messages=[], canValid=False)
  step(messages=[], canValid=False)
  parameter('DisableDM', b'0')
  init()
  for policy in ('vision', 'wheeltouch'):
    for level in ('none', 'one', 'two', 'three'):
      step(healthy_messages({'driverMonitoringState': {'activePolicy': policy, 'alertLevel': level,
            'lockout': True, 'alwaysOnLockout': True, 'visionPolicyState': {'uncertainOffroadAlertPercent': 100}}}))
  parameter('DisableDM', b'1')
  parameter('IsLdwEnabled', b'1')
  init()
  for status in ('uncalibrated', 'recalibrating', 'recalibrating', 'invalid', 'calibrated'):
    step(healthy_messages({'liveCalibration': {'calStatus': status, 'rpyCalib': [0., 0., 0.]},
                          'driverAssistance': {'leftLaneDeparture': True}}))
  for kind in ('fork left prepare', 'fork left', 'turn left', 'turn right', '', 'turn right prepare', 'fork right'):
    step(healthy_messages({'carrotMan': {'atcType': kind}}))
  for phase in ('preLaneChange', 'laneChangeStarting', 'laneChangeFinishing', 'off'):
    for direction in ('none', 'left', 'right'):
      for blind in (False, True):
        step(healthy_messages({'modelV2': {'meta': {'laneChangeState': phase, 'laneChangeDirection': direction}}}),
             leftBlindspot=blind, rightBlindspot=blind)
  init(mode={'simulation': False}, ready=True)
  step()
  for free in (7., 6.999):
    for memory in (90, 91):
      for thermal in ('green', 'yellow', 'red', 'danger'):
        step(healthy_messages({'deviceState': {'freeSpacePercent': free, 'memoryUsagePercent': memory, 'thermalStatus': thermal}}))
  stalled = healthy_messages({'peripheralState': {'pandaType': 'uno', 'fanSpeedRpm': 499},
                              'deviceState': {'freeSpacePercent': 50., 'fanSpeedPercentDesired': 51}})
  step(stalled, count=1501)
  step(healthy_messages({'peripheralState': {'pandaType': 'uno', 'fanSpeedRpm': 500},
                        'deviceState': {'freeSpacePercent': 50., 'fanSpeedPercentDesired': 51}}))
  init()
  mismatch = healthy_messages({'pandaStates': {'safetyModel': 'hondaNidec', 'controlsAllowed': True,
                            'faults': ['relayMalfunction']}})
  step(mismatch, count=1000)
  step(mismatch)
  step(mismatch)
  init(mode={'replay': True})
  step(cruiseState={'available': True, 'enabled': True})
  denied = healthy_messages({'pandaStates': {'safetyModel': 'toyota', 'controlsAllowed': False}})
  step(denied, count=199, cruiseState={'available': True, 'enabled': True})
  step(denied, cruiseState={'available': True, 'enabled': True})
  step(healthy_messages({'pandaStates': {'safetyModel': 'silent', 'controlsAllowed': False}}))
  init(mode={'simulation': False, 'nvme_present': True}, ready=True)
  step()
  for name in ('jetlinkd', 'loggerd', 'modeld'):
    step(healthy_messages({'managerState': {'processes': [{'name': name, 'running': False, 'shouldBeRunning': True}]}}))
  names = ['loggerd', 'modeld', "quote'", 'both\'"', 'line\n\t', '한글', '\u200b']
  for order in (names, list(reversed(names)), names + names):
    step(healthy_messages({'managerState': {'processes': [{'name': name, 'running': False, 'shouldBeRunning': True} for name in order]}}))
  for error in ({}, {'canError': True}, {'radarUnavailableTemporary': True}):
    step(healthy_messages({'radarState': {'radarErrors': error}}, invalid=('radarState',)))
  step(healthy_messages(invalid=('pandaStates',)), canTimeout=True)
  step(canValid=False)
  for source in ('native', 'jetlink'):
    step(healthy_messages({'modelV2': {'jetlink': {'source': source, 'lossLatched': True}}}))
    step(healthy_messages({'modelV2': {'jetlink': {'source': source}}}, invalid=('modelV2',)))
  step(lagging=True)
  step(healthy_messages(invalid=('driverAssistance',)))
  step(healthy_messages(invalid=('driverAssistance',)))
  step([], dt=2.)
  step([], dt=2.)
  step(count=120, dt=.02)
  parameter('UsbGpuLoading', b'1')
  step(healthy_messages(invalid=('driverAssistance',)))
  parameter('UsbGpuLoading', b'0')
  step(healthy_messages(invalid=('driverAssistance',)))
  step(healthy_messages(invalid=('driverAssistance',)), dt=5.)
  step(healthy_messages(invalid=('driverAssistance',)))
  parameter('UsbGpuActive', b'1')
  step()
  parameter('UsbGpuActive', b'0')
  step()
  init(mode={'simulation': False}, ready=True)
  step()
  step(healthy_messages({'livePose': {'inputsOK': False, 'posenetOK': False}, 'liveParameters': {'valid': False}}))
  step(healthy_messages({'controlsState': {'lateralControlState': {'pidState': {'active': True, 'saturated': True}}, 'curvature': 0.},
                        'modelV2': {'velocity': {'x': [20.]}, 'action': {'desiredCurvature': .01}, 'meta': {'hardBrakePredicted': True}}}), count=202)
  step(healthy_messages({'longitudinalPlan': {'fcw': True}}), cruiseState={'available': True, 'enabled': True})
  step(healthy_messages({'modelV2': {'meta': {'hardBrakePredicted': True}}}), brakePressed=True)
  for raw in (None, b'null', b'false', b'0', b'{}', b'[]', b'"x"', b'NaN', b'bad', b'\xff',
              'null'.encode('utf-16'), 'null'.encode('utf-32'), b'"\\ud800"'):
    parameter('Offroad_ExcessiveActuation', raw)
    init()
    step()
  parameter('Offroad_ExcessiveActuation', None)
  for raw in (None, b'', b'  +01\n', b'1_0', b'-000', b'bad', b'\xff', b'2', b'0'):
    parameter('LongitudinalPersonality', raw)
    init()
    rows.append({'operation': 'params_cycle'})
    step()
  parameter('LongitudinalPersonality', b'1')
  init(mode={'simulation': False})
  step(healthy_messages({'managerState': {'rebootRequired': True}}), count=1501, dt=.5)
  step(healthy_messages({'managerState': {'rebootRequired': True}}))
  parameter('NNFFModelName', b'fixture-model')
  init()
  step(count=551)
  parameter('NNFFModelName', None)
  step(healthy_messages({'longitudinalPlan': {'events': [{'name': 'torqueNNLoad'}]},
                        'modelV2': {'meta': {'hardBrakePredicted': True}}}))
  step(healthy_messages({'longitudinalPlan': {'events': [{'name': 'torqueNNLoad'}]}}))
  init()
  step(healthy_messages({'longitudinalPlan': {'events': [{'name': 'torqueNNLoad'}]}}))
  init(notCar=True)
  step(count=202)
  init(mode={'replay': True})
  step(healthy_messages({'modelV2': {'frameDropPerc': 20.001}}), count=77, vEgo=2000.)
  step(vEgo=2000.)
  step(count=5, vEgo=2000.)
  init()
  step([*healthy_messages(), event('alertDebug', alertText1='one', alertText2='two'), event('userBookmark'), event('audioFeedback', blockNum=3)])
  step([*healthy_messages(), event('alertDebug', alertText1='one', alertText2='two')])
  init()
  step(healthy_messages({'controlsState': {'lateralControlState': {'debugState': {'active': True}}}}))
  init(mode={'replay': True})
  step(cruiseState={'available': True, 'enabled': True})
  lead = {'radarTrackId': 7, 'dRel': 20., 'yRel': 1., 'vRel': -1.}
  step(healthy_messages({'radarState': {'leadsCutIn': [lead], 'leadTwo': dict(lead, status=True)}}), latEnabled=True)
  step(healthy_messages({'radarState': {'leadsCutIn': [lead], 'leadTwo': dict(lead, status=True)}}), latEnabled=True)
  init(safetyConfigs=[])
  step()
  init()
  step(healthy_messages({'carControl': {'longActive': True}, 'livePose': {'inputsOK': True, 'posenetOK': True,
                        'accelerationDevice': {'x': 10.}}}), count=102, aEgo=10.)
  parameter('Offroad_ExcessiveActuation', None)
  init(mode={'simulation': False}, ready=True, pcmCruise=False)
  step(cruiseState={'available': True, 'enabled': True}, count=601)
  step(cruiseState={'available': True, 'enabled': True})
  init()
  step(healthy_messages(omit=('driverAssistance',)))
  step(healthy_messages(omit=('driverAssistance',)))
  step(healthy_messages(invalid=('driverAssistance',)))
  return rows
