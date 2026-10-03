from __future__ import annotations

import contextlib
import io
import random
from can_source import load
from card_qa.mazda.source import Settings
from card_qa.volkswagen.source import parameters


def control(index):
  from openpilot.cereal import car
  return car.CarControl.new_message(enabled=index % 120 < 100, latActive=index % 90 < 76, longActive=index % 80 < 65,
    leftBlinker=index % 25 < 7, rightBlinker=index % 35 < 10,
    cruiseControl={'cancel': index % 40 < 12, 'resume': index % 60 < 8, 'override': index % 75 < 7},
    actuators={'torque': (0., 0.5, -0.5, 1., -1., 1.5, -1.5)[index // 19 % 7],
               'curvature': (0., 0.001, -0.001, 0.2, -0.2)[index // 23 % 5],
               'accel': (-4., -3.5, -0.2, 0., 0.199, 0.2, 0.3, 1.6, 2., 4.)[index // 13 % 10],
               'longControlState': ('off', 'pid', 'stopping', 'starting')[index // 31 % 4]},
    hudControl={'visualAlert': ('none', 'steerRequired', 'ldw', 'fcw')[index // 7 % 4],
                'setSpeed': (0., 20., 40., 100.)[index // 13 % 4], 'leadVisible': index % 2 == 0,
                'leadDistance': (0., 5., 20., -1.)[index // 21 % 4], 'leadDistanceBars': index // 17 % 6,
                'leadLimiting': index % 400 < 250, 'leftLaneVisible': index % 3 == 0, 'rightLaneVisible': index % 4 == 0,
                'leftLaneDepart': index % 8 == 0, 'rightLaneDepart': index % 9 == 0,
                'naviEventType': (0, 1, 2, 3, 4, 5, 8)[index // 140 % 7],
                'naviEventSpeed': (30., -30., 60.)[index // 90 % 3], 'naviSpeedLimit': 80 if 500 <= index < 580 else 0})


def runtime_steps(profile, count=660, quiet=False, focused=False):
  load()
  from opendbc.can import CANPacker, CANDefine
  from opendbc.car.volkswagen.interface import CarInterface
  from opendbc.car.volkswagen.values import VolkswagenFlags as F
  from opendbc.can import parser
  parser.time = type('Clock', (), {'monotonic_ns': staticmethod(lambda: profile['now'])})
  with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
    cp = parameters(profile, Settings(profile['settings']))
    vehicle = CarInterface(cp)
    try:
      vehicle.update([])
    except NameError as failure:
      assert str(failure) == "name 'np' is not defined"
      assert not cp.flags & (F.PQ | F.MEB)
  packer = CANPacker(vehicle.CC.packer_pt.dbc.name)
  definitions = CANDefine(packer.dbc.name).dv
  messages = [(parser_.bus, state.name) for parser_ in vehicle.can_parsers.values() for state in parser_.message_states.values()]
  rng = random.Random(177129)
  steps = []
  for index in range(count):
    now = profile['now'] + index * 10_000_000
    frames = []
    for bus, name in messages:
      signals = packer.dbc.name_to_msg[name].sigs
      values = {key: rng.randrange(1 << min(signal.size, 8)) * signal.factor + signal.offset for key, signal in signals.items()}
      for key, signal in signals.items():
        if key == 'COUNTER':
          values[key] = index % (1 << signal.size) if not 160 <= index < 180 else 0
        if key in ('GE_Fahrstufe', 'MO_Waehlpos', 'Waehlhebelposition__Getriebe_1_'):
          options = list(definitions[name][key])
          values[key] = options[index // 37 % len(options)]
        if key in ('EPS_HCA_Status', 'LatCon_HCA_Status', 'LH2_Sta_HCA'):
          values[key] = list(definitions[name][key])[index // 43 % len(definitions[name][key])]
          if focused:
            label = 'FAULT' if index < 620 else 'ACTIVE'
            values[key] = next(value for value, text in definitions[name][key].items() if text.upper() == label)
        if key in ('VL_Radgeschw', 'VR_Radgeschw', 'HL_Radgeschw', 'HR_Radgeschw',
                   'Radgeschw__VL_4_1', 'Radgeschw__VR_4_1', 'Radgeschw__HL_4_1', 'Radgeschw__HR_4_1', 'Geschwindigkeit_neu__Bremse_1_'):
          values[key] = (0. if index < 210 else 0.36 if index < 220 else 1.8) if focused else (0., 0.36, 1.8, 3.6, 18., 60.)[index // 17 % 6]
        if key in ('EPS_Lenkmoment', 'LH3_LM'):
          values[key] = (0., 59., 60., 61., 79., 80., 81., 200., 300.)[index // 11 % 9]
        if key.startswith('GRA_') and signal.size == 1:
          values[key] = int(index % 8 < 4)
        if key in ('TSK_Status', 'GRA_Status'):
          values[key] = index // 53 % 8
        if key in ('Comfort_Signal_Left', 'Comfort_Signal_Right', 'SMLS_Blinker_li', 'SMLS_Blinker_re'):
          values[key] = int(index % 300 < 2) if key.endswith(('Left', 'li')) else int(130 <= index % 300 < 132)
        if quiet and key in ('Accel_Pedal_Pressure', 'Motion_State', 'Long_Control_Inhibit'):
          values[key] = 0
        if focused:
          if key == 'Motion_State': values[key] = 3 if index < 30 or 300 <= index < 350 else 0
          if key == 'Long_Control_Inhibit': values[key] = 2 if 480 <= index < 500 else 0
          if key == 'EPB_Status': values[key] = 0
          if key == 'TSK_Status': values[key] = 6 if 420 <= index < 460 or 490 <= index < 630 else 3
          if key == 'GE_Fahrstufe': values[key] = next(value for value, text in definitions[name][key].items() if text == 'D')
      address, data, bus = packer.make_can_msg(name, bus, values)
      frames.append({'address': address, 'data': list(data), 'bus': bus})
    packets = [{'mono_time': now, 'frames': frames}]
    if 200 <= index < 240 and not focused:
      packets = [{'mono_time': now, 'frames': []}]
    if index % 73 == 72 and not focused:
      packets = []
    cc = control(index)
    if focused:
      cc.enabled = not 350 <= index < 370
      cc.latActive = not 700 <= index < 760
      cc.longActive = True
      cc.cruiseControl.override = 380 <= index < 400
      cc.actuators.accel = -1. if 300 <= index < 350 else 0.3
      cc.actuators.longControlState = 'stopping' if 300 <= index < 350 else 'starting' if 230 <= index < 250 else 'pid'
      cc.hudControl.naviEventType = (0, 1, 2, 3, 4, 5, 8)[index // 150 % 7]
      cc.hudControl.naviEventSpeed = 50
      cc.hudControl.naviSpeedLimit = 0
    steps.append({'now': now, 'packets': packets, 'control': list(cc.to_bytes()), 'soft_hold': index % 4,
                  'commit': {'vCruise': 70., 'activateCruise': index % 2}})
  return steps
