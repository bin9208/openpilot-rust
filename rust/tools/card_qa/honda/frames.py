from __future__ import annotations

import contextlib
import io
import random
from can_source import load
from card_qa.mazda.source import Settings


def runtime_steps(profile, advanced, count=480, pump=False):
  load()
  from opendbc.can import CANPacker, CANDefine
  from opendbc.car import Bus, interfaces, structs
  from opendbc.car.honda.hondacan import CanBus
  from opendbc.car.honda.interface import CarInterface
  from opendbc.car.honda.values import DBC, HondaFlags as F
  from openpilot.cereal import car
  interfaces.Params = lambda: Settings(profile['settings'])
  firmware = [structs.CarParams.CarFw.new_message(ecu=fw['ecu'], fwVersion=bytes(fw['fw_version'])) for fw in profile['firmware']]
  fingerprint = {bus: {} for bus in range(8)}
  fingerprint.update({bus: dict(rows) for bus, rows in profile['fingerprints']})
  with contextlib.redirect_stdout(io.StringIO()):
    cp = CarInterface.get_params(profile['candidate'], fingerprint, firmware, profile['alpha_long'], True, False)
  if profile.get('offset', False):
    cp.safetyConfigs = [{'safetyModel': 'noOutput'}, {}]
  bus = CanBus(cp)
  name = DBC[profile['candidate']][Bus.pt]
  packer = CANPacker(name)
  definitions = CANDefine(name).dv
  gearbox = 'GEARBOX_ALT_2' if cp.transmissionType == 'manual' else 'GEARBOX_15T' if profile['candidate'] == 'HONDA_ACCORD' and cp.transmissionType == 'cvt' else 'GEARBOX_ALT' if profile['candidate'] == 'HONDA_CIVIC_2022' and cp.transmissionType == 'cvt' else 'GEARBOX'
  messages = [(bus.pt, name) for name in ('SCM_BUTTONS', 'CAR_SPEED', 'ENGINE_DATA', 'SCM_FEEDBACK', 'SEATBELT_STATUS', 'STEER_STATUS', 'VSA_STATUS',
                                        'WHEEL_SPEEDS', 'STEERING_SENSORS', 'STEER_MOTOR_TORQUE', 'POWERTRAIN_DATA', gearbox)]
  bosch, radarless = bool(cp.flags & F.BOSCH), bool(cp.flags & F.BOSCH_RADARLESS)
  if profile['candidate'] not in ('HONDA_ACCORD', 'HONDA_CIVIC_BOSCH', 'HONDA_CIVIC_BOSCH_DIESEL', 'HONDA_CRV_HYBRID', 'HONDA_INSIGHT',
                                  'ACURA_RDX_3G', 'HONDA_E', 'HONDA_CIVIC_2022', 'HONDA_HRV_3G', 'HONDA_ODYSSEY_CHN', 'HONDA_FREED', 'HONDA_HRV'):
    messages.append((bus.pt, 'DOORS_STATUS'))
  if radarless:
    messages += [(bus.pt, 'CRUISE_FAULT_STATUS'), (bus.camera, 'ACC_HUD'), (bus.camera, 'LKAS_HUD')]
  else:
    if cp.openpilotLongitudinalControl: messages.append((bus.pt, 'STANDSTILL'))
    if not bosch: messages += [(bus.camera, 'ACC_HUD'), (bus.camera, 'LKAS_HUD'), (bus.camera, 'BRAKE_COMMAND'), (bus.pt, 'CRUISE')]
    if bosch and not cp.openpilotLongitudinalControl: messages += [(bus.pt, 'ACC_HUD'), (bus.pt, 'ACC_CONTROL')]
  if bosch or profile['candidate'] in ('HONDA_CIVIC', 'HONDA_ODYSSEY', 'HONDA_ODYSSEY_CHN'): messages.append((bus.pt, 'EPB_STATUS'))
  if cp.flags & F.BOSCH_ALT_BRAKE: messages.append((bus.pt, 'BRAKE_MODULE'))
  body = CANPacker(DBC[profile['candidate']][Bus.body]) if cp.enableBsm else None
  rng = random.Random(177126 + int(advanced))
  steps = []
  for index in range(count):
    now = profile['now'] + index * 10_000_000
    frames = []
    for source, message in messages:
      signals = packer.dbc.name_to_msg[message].sigs
      values = {key: rng.randrange(1 << min(signal.size, 12)) * signal.factor + signal.offset for key, signal in signals.items()}
      if not 150 <= index < 180:
        for key, signal in signals.items():
          if key == 'COUNTER': values[key] = index % (1 << signal.size)
      if message == 'WHEEL_SPEEDS':
        values.update({key: 3.6 if pump else (0., 0.001, 0.4, 3.6, 7.2, 18., 43.2, 90.)[index // 17 % 8] for key in ('WHEEL_SPEED_FL', 'WHEEL_SPEED_FR', 'WHEEL_SPEED_RL', 'WHEEL_SPEED_RR')})
      elif message == 'ENGINE_DATA': values['XMISSION_SPEED'] = (0., 0.001, 0.4, 3.6, 7.2, 18., 43.2, 90.)[index // 17 % 8] if not pump else 3.6
      elif message == 'CAR_SPEED': values.update(ROUGH_CAR_SPEED_2=0 if index < 40 else index % 150, IMPERIAL_UNIT=int(index % 70 < 20))
      elif message == 'STEER_STATUS': values.update(STEER_STATUS=index // 15 % 16, STEER_TORQUE_SENSOR=(-1201, -1200, -401, -400, 0, 400, 401, 1200, 1201)[index // 13 % 9])
      elif message == 'SCM_BUTTONS': values.update(CRUISE_BUTTONS=(0, 4, 3, 0, 2, 1, 5)[index // 5 % 7], CRUISE_SETTING=(0, 3, 1, 0, 2)[index // 7 % 5])
      elif message == 'SCM_FEEDBACK': values.update(LEFT_BLINKER=int(index % 270 < 2), RIGHT_BLINKER=int(120 <= index % 270 < 122))
      elif message == 'POWERTRAIN_DATA': values.update(BRAKE_SWITCH=int(index % 5 in (0, 1, 4)), BRAKE_PRESSED=int(index % 37 == 0), PEDAL_GAS=index % 5, ACC_STATUS=int(index % 90 < 80))
      elif message == 'ACC_HUD': values.update(CRUISE_SPEED=(0, 60, 159, 160, 161, 252, 254, 255)[index // 11 % 8])
      elif message == gearbox:
        if 'GEAR_SHIFTER' in signals: values['GEAR_SHIFTER'] = list(definitions[gearbox]['GEAR_SHIFTER'])[index // 9 % len(definitions[gearbox]['GEAR_SHIFTER'])]
        elif 'GEAR_MT' in signals: values['GEAR_MT'] = (0, 1, 6, 14)[index // 9 % 4]
      address, data, source = packer.make_can_msg(message, source, values)
      frames.append({'address': address, 'data': list(data), 'bus': source})
      if message == 'POWERTRAIN_DATA' and index % 31 == 30:
        values['BRAKE_SWITCH'] = 1
        values['COUNTER'] = (index + 1) % (1 << signals['COUNTER'].size)
        address, data, source = packer.make_can_msg(message, source, values)
        frames.append({'address': address, 'data': list(data), 'bus': source})
    if body is not None:
      for message in ('BSM_STATUS_LEFT', 'BSM_STATUS_RIGHT'):
        address, data, source = body.make_can_msg(message, bus.radar, {'BSM_ALERT': index % 3})
        frames.append({'address': address, 'data': list(data), 'bus': source})
    packets = [{'mono_time': now, 'frames': frames}]
    if 180 <= index < 220 and not pump: packets = [{'mono_time': now, 'frames': []}]
    if index % 71 == 70 and not pump: packets = []
    control = car.CarControl.new_message(enabled=index % 100 < 85, latActive=index % 90 < 75, longActive=True if pump else index % 80 < 65,
      cruiseControl={'cancel': index % 50 < 12, 'resume': index % 70 < 9},
      actuators={'torque': (0., 0.5, -0.5, 1., -1., 1.5, -1.5)[index // 21 % 7],
                 'accel': -1.0 if pump else (-4., -3.5, -0.2, -0.001, 0., 0.199, 0.301, 1.6, 2., 4.)[index // 13 % 10],
                 'longControlState': 'stopping' if index < 430 else 'starting'},
      hudControl={'visualAlert': ('none', 'steerRequired', 'ldw', 'fcw', 'brakePressed', 'wrongGear', 'seatbeltUnbuckled', 'speedTooHigh')[index // 10 % 8],
                  'speedVisible': index % 5 != 0, 'setSpeed': (0., 20., 40., 100.)[index // 13 % 4],
                  'lanesVisible': index % 3 != 0, 'leadVisible': index % 2 == 0, 'leadDistanceBars': index // 9 % 5})
    settings = {40: {'CustomSteerMax': '1200', 'CustomSteerDeltaUp': '20', 'CustomSteerDeltaDown': '30'},
                100: {'CustomSteerMax': '0', 'CustomSteerDeltaUp': '-1', 'CustomSteerDeltaDown': '0'},
                230: {'CustomSteerMax': '  +800tail', 'CustomSteerDeltaUp': '8', 'CustomSteerDeltaDown': '18'}}.get(index, {}) if advanced else {}
    steps.append({'now': now, 'packets': packets, 'control': list(control.to_bytes()), 'settings': settings,
                  'soft_hold': index % 4, 'commit': {'vCruise': 70., 'activateCruise': index % 2}})
  return steps
