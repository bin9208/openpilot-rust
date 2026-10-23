from __future__ import annotations

import itertools
from can_source import load


def cases():
  load()
  from opendbc.can import CANPacker
  from opendbc.car import structs
  result = []
  for alpha, nnff, lite, disable, fw in itertools.product((False, True), repeat=5):
    result.append(dict(name=f'params-{alpha}-{nnff}-{lite}-{disable}-{fw}', op='params',
      candidate='PSA_PEUGEOT_208', alpha_long=alpha, fingerprints=[(0, [(0x123, 8)])],
      firmware=[dict(ecu='eps', fw_version=list(b'owned-psa-eps'))] if fw else [],
      settings={'NNFF': str(int(nnff)), 'NNFFLite': str(int(lite)), 'DisableMinSteerSpeed': str(int(disable))},
      now=2_000_000_000, steps=[]))
  for nnff, alpha in itertools.product((False, True), repeat=2):
    packer = CANPacker('psa_aee2010_r3')
    steps = []
    for tick in range(360):
      now = 2_000_000_000 + tick * 10_000_000
      speed = (0., 0.01, 17.99, 18., 18.01, 89.99, 90., 90.01, 126.)[(tick // 11) % 9]
      messages = [
        (0, 'Dyn4_FRE', dict(zip(('P263_VehV_VPsvValWhlFrtL', 'P264_VehV_VPsvValWhlFrtR',
                                'P265_VehV_VPsvValWhlBckL', 'P266_VehV_VPsvValWhlBckR'),
                               (speed, speed+.01, speed+.02, speed+.03)))),
        (1, 'HS2_DYN_UCF_MDD_32D', {'VITESSE_LACET_BRUTE': (tick % 17-8)*.5,
          'VEHICLE_STANDSTILL': int(tick % 40 < 10), 'ACC_ETAT_DECEL_OR_ESP_STATUS': (tick//9)%4}),
        (0, 'Dyn_CMM', {'P002_Com_rAPP': (tick//7)%4}),
        (2, 'Dat_BSI', {'P013_MainBrake': int(tick%30<8), 'P103_Com_bRevGear': int(tick%50<10),
          'DRIVER_DOOR': int(tick%35<5), 'PASSENGER_DOOR': int(tick%37<7)}),
        (0, 'Dyn_EasyMove', {'P337_Com_stPrkBrk': (tick//5)%4}),
        (0, 'STEERING_ALT', {'ANGLE': (tick%41-20)*12.5, 'RATE': tick%120, 'RATE_SIGN': tick%2}),
        (0, 'STEERING', {'DRIVER_TORQUE': (0., 5., 6., -5., -6., 10.)[(tick//13)%6]}),
        (0, 'IS_DAT_DIRA', {'EPS_TORQUE': (tick%21-10)*.25, 'EPS_STATE_LKA': (tick//17)%5}),
        (1, 'HS2_DAT_MDD_CMD_452', {'SPEED_SETPOINT': (0., 30., 90., 255.)[(tick//13)%4],
          'RVV_ACC_ACTIVATION_REQ': (tick//7)%4, 'LONGITUDINAL_REGULATION_TYPE': (tick//11)%4}),
        (1, 'HS2_DYN1_MDD_ETAT_2B6', {'ACC_STATUS': (tick//11)%8}),
        (2, 'HS2_DAT7_BSI_612', {'CDE_CLG_ET_HDC': (tick//7)%4}),
        (2, 'RESTRAINTS', {'DRIVER_SEATBELT': (tick//9)%4}),
      ]
      frames = []
      for bus, name, values in messages:
        address, data, bus = packer.make_can_msg(name, bus, values)
        if tick%61 == 60 and name == 'Dyn4_FRE':
          data = bytes([data[0]^1, *data[1:]])
        frames.append({'address': address, 'data': list(data), 'bus': bus})
      if tick%47 == 46:
        frames = [{**frame, 'bus': 3} for frame in frames]
      packets = [{'mono_time': now, 'frames': frames}] if tick else []
      if 230 <= tick < 260: packets = [{'mono_time': now, 'frames': []}]
      if tick%71 == 70: packets = []
      control = structs.CarControl.new_message(enabled=tick%100<80, latActive=10<=tick%100<90,
        longActive=True, actuators={'steeringAngleDeg': (-500., -30., 0., 30., 500.)[(tick//19)%5], 'accel': .5})
      steps.append(dict(now=now, packets=packets, control=list(control.to_bytes()), soft_hold=tick%4))
    result.append(dict(name=f'runtime-PSA_PEUGEOT_208-{nnff}-{alpha}', op='runtime',
      candidate='PSA_PEUGEOT_208', alpha_long=alpha, fingerprints=[], firmware=[],
      settings={'NNFF': str(int(nnff))}, now=2_000_000_000, steps=steps))
  normal = next(case for case in result if case['op']=='runtime')
  for active in (False, True):
    control = structs.CarControl.new_message(latActive=active, actuators={'steeringAngleDeg': 20.})
    result.append({**normal, 'name': f'before-update-{active}', 'op': 'before_update',
                   'steps': [{**normal['steps'][0], 'control': list(control.to_bytes())}]})
  control = structs.CarControl.new_message(latActive=True, actuators={'steeringAngleDeg': float('nan')})
  result.append({**normal, 'name': 'angle-nan', 'op': 'numeric_error',
                 'steps': [{**normal['steps'][1], 'control': list(control.to_bytes())}]})
  return result
