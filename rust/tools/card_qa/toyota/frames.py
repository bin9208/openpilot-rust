from __future__ import annotations

import random
from can_source import load
from card_qa.mazda.source import Settings


def runtime_steps(profile, advanced):
  load()
  from opendbc.can import CANPacker
  from opendbc.car import Bus, interfaces, structs
  from opendbc.car.toyota.interface import CarInterface
  from opendbc.car.toyota.values import DBC, ToyotaFlags as F, TSS2_CAR, RADAR_ACC_CAR, UNSUPPORTED_DSU_CAR
  from opendbc.car.secoc import build_sync_mac
  from openpilot.cereal import car

  interfaces.Params = lambda: Settings(profile['settings'])
  firmware = [structs.CarParams.CarFw.new_message(ecu=fw['ecu'], fwVersion=bytes(fw['fw_version'])) for fw in profile['firmware']]
  fingerprint = {bus: {} for bus in range(8)}
  fingerprint.update({bus: dict(rows) for bus, rows in profile['fingerprints']})
  cp = CarInterface.get_params(profile['candidate'], fingerprint, firmware, profile['alpha_long'], True, False)
  secoc = bool(cp.flags & F.SECOC)
  acc_bus = 2 if profile['candidate'] in TSS2_CAR - RADAR_ACC_CAR else 0
  unsupported = profile['candidate'] in UNSUPPORTED_DSU_CAR
  packer = CANPacker(DBC[profile['candidate']][Bus.pt])
  messages = [(0, name) for name in ('BODY_CONTROL_STATE', 'BRAKE_MODULE', 'ESP_CONTROL', 'WHEEL_SPEEDS', 'STEER_ANGLE_SENSOR',
                                    'STEER_TORQUE_SENSOR', 'EPS_STATUS', 'BLINKERS_STATE', 'PCM_CRUISE', 'BODY_CONTROL_STATE_2', 'LIGHT_STALK')]
  if secoc: messages += [(0, name) for name in ('SECOC_SYNCHRONIZATION', 'GAS_PEDAL', 'GEAR_PACKET_HYBRID')]
  else:
    messages += [(0, 'VSC1S07'), (0, 'GEAR_PACKET')]
    if not cp.enableDsu and not cp.flags & F.DISABLE_RADAR: messages.append((acc_bus, 'PRE_COLLISION'))
    if profile['candidate'] != 'TOYOTA_MIRAI': messages.append((0, 'ENGINE_RPM'))
  messages += [(0, 'DSU_CRUISE' if unsupported else 'PCM_CRUISE_2'), (0, 'PCM_CRUISE_ALT' if unsupported else 'PCM_CRUISE_SM')]
  if profile['candidate'] in TSS2_CAR and not cp.flags & F.DISABLE_RADAR: messages += [(acc_bus, 'ACC_CONTROL'), (acc_bus, 'PCS_HUD')]
  if profile['candidate'] != 'TOYOTA_PRIUS_V': messages.append((2, 'LKAS_HUD'))
  if cp.enableBsm: messages.append((0, 'BSM'))
  rng = random.Random(177125 + int(advanced))
  key = bytes(profile.get('secoc_key', b'00' * 16))
  steps = []
  for index in range(320):
    now = profile['now'] + index * 10_000_000
    frames = []
    for bus, name in messages:
      signals = packer.dbc.name_to_msg[name].sigs
      values = {key: rng.randrange(1 << min(signal.size, 12)) * signal.factor + signal.offset for key, signal in signals.items()}
      if not 150 <= index < 180:
        for key_name, signal in signals.items():
          if 'COUNTER' in key_name: values[key_name] = index % (1 << signal.size)
      if name == 'WHEEL_SPEEDS':
        values.update({key: (0., 0.001, 0.4, 3.6, 7.2, 18., 43.2, 90.)[index // 17 % 8] for key in ('WHEEL_SPEED_FL', 'WHEEL_SPEED_FR', 'WHEEL_SPEED_RL', 'WHEEL_SPEED_RR')})
      elif name == 'STEER_ANGLE_SENSOR':
        values.update(STEER_ANGLE=(0., 25., 89., 90., -91., -30.)[index // 29 % 6], STEER_FRACTION=index % 10 * 0.1,
                      STEER_RATE=(0., 99., 100., 101., -101.)[index // 23 % 5])
      elif name == 'STEER_TORQUE_SENSOR':
        values.update(STEER_ANGLE=(0., 27., 92., 100., -88., -20.)[index // 29 % 6], STEER_ANGLE_INITIALIZING=int(index < 8),
                      STEER_TORQUE_DRIVER=(0., 99., 100., 101., 149., 150., 499., 500., -500.)[index // 13 % 9],
                      STEER_TORQUE_EPS=(-1800., -1000., 0., 1000., 1800.)[index // 31 % 5])
      elif name == 'EPS_STATUS':
        values['LKA_STATE'] = (0, 1, 3, 9, 11, 17, 21, 25)[index // 17 % 8]
        if 'LTA_STATE' in signals: values['LTA_STATE'] = (0, 1, 3, 9, 11, 17, 21, 25)[index // 13 % 8]
      elif name == 'SECOC_SYNCHRONIZATION':
        trip = index // 90
        reset = index // 50 if advanced else 0
        values.update(TRIP_CNT=trip, RESET_CNT=reset, AUTHENTICATOR=build_sync_mac(key, trip, reset) ^ (1 if reset % 2 else 0))
      elif name == 'PCM_CRUISE':
        values.update(CRUISE_STATE=(0, 1, 6, 7, 8)[index // 13 % 5], CRUISE_ACTIVE=int(index % 100 < 85))
        if 'GAS_RELEASED' in signals: values['GAS_RELEASED'] = index % 2
      elif name == 'PCM_CRUISE_2':
        values.update(SET_SPEED=(0, 20, 70, 100)[index // 19 % 4], ACC_FAULTED=int(index % 50 < 3), LOW_SPEED_LOCKOUT=index // 11 % 3,
                      MAIN_ON=int(index % 150 < 140), PCM_FOLLOW_DISTANCE=index // 7 % 4)
      elif name == 'ACC_CONTROL': values.update(ACC_TYPE=index // 23 % 3, DISTANCE=int(index % 20 < 5))
      address, data, source = packer.make_can_msg(name, bus, values)
      frames.append({'address': address, 'data': list(data), 'bus': source})
    packets = [{'mono_time': now, 'frames': frames}]
    if 180 <= index < 220: packets = [{'mono_time': now, 'frames': []}]
    if index % 71 == 70: packets = []
    control = car.CarControl.new_message(
      enabled=index % 100 < 85, latActive=index % 90 < 75, longActive=index % 80 < 65,
      cruiseControl={'cancel': index % 50 < 12, 'resume': index % 70 < 9},
      actuators={'torque': (0., 0.5, -0.5, 1., -1., 1.5, -1.5)[index // 21 % 7], 'steeringAngleDeg': (0., 30., -30., 120., -120.)[index // 27 % 5],
                 'accel': (-4., -3.5, -0.001, 0., 0.199, 0.301, 1.5, 2., 4.)[index // 13 % 9],
                 'longControlState': ('off', 'pid', 'stopping', 'starting')[index // 17 % 4]},
      hudControl={'visualAlert': ('none', 'steerRequired', 'ldw', 'fcw')[index // 10 % 4], 'leftLaneVisible': index % 3 == 0,
                  'rightLaneVisible': index % 4 == 0, 'leftLaneDepart': index % 3 == 0, 'rightLaneDepart': index % 3 != 0,
                  'leadVisible': index % 2 == 0, 'leadDistanceBars': index // 9 % 5},
      orientationNED=[0., (-0.15, 0., 0.15)[index // 31 % 3], 0.] if index % 7 else [])
    settings = {}
    if advanced:
      settings = {40: {'CustomSteerMax': '1200', 'CustomSteerDeltaUp': '20', 'CustomSteerDeltaDown': '30'},
                  100: {'CustomSteerMax': '0', 'CustomSteerDeltaUp': '-1', 'CustomSteerDeltaDown': '0'},
                  230: {'CustomSteerMax': '  +800tail', 'CustomSteerDeltaUp': '8', 'CustomSteerDeltaDown': '18'}}.get(index, {})
    steps.append({'now': now, 'packets': packets, 'control': list(control.to_bytes()), 'settings': settings, 'is_metric': index % 2 == 0,
                  'soft_hold': index % 4, 'commit': {'vCruise': 70., 'activateCruise': index % 2}})
  return steps
