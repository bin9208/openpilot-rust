from __future__ import annotations

import contextlib
import io
from pathlib import Path
from pytest import MonkeyPatch

from radarcan_decoder_source import metadata


def cases(directory: Path):
  from opendbc.can.dbc import DBC
  from opendbc.can.packer import CANPacker
  profiles = [('chrysler', 'CHRYSLER_PACIFICA_2018'), ('gm', 'CHEVROLET_VOLT'), ('honda', 'HONDA_CIVIC'),
    ('rivian', 'RIVIAN_R1_GEN1'), ('tesla', 'TESLA_MODEL_3'), ('toyota', 'TOYOTA_PRIUS'), ('toyota', 'TOYOTA_RAV4_TSS2'),
    ('volkswagen', 'VOLKSWAGEN_ID4_MK1'), ('ford', 'FORD_FOCUS_MK4')]
  output = []
  for brand, candidate in profiles:
    case = {'name': f'decoder-{candidate}-lifecycle', 'op': 'decoder', 'candidate': candidate, 'brand': brand,
      'delay': .02, 'period': .05, 'unavailable': False, 'constructor_ns': 1_000_000_000,
      'dbc_root': str(directory.resolve()), 'actions': []}
    if brand == 'volkswagen':
      case.update(flags=16, delay=.8, period=.04)
    with contextlib.redirect_stdout(io.StringIO()):
      case['parser'] = metadata(case)
      with MonkeyPatch.context() as fixture:
        fixture.setattr('opendbc.can.dbc.DBC_PATH', str(directory.resolve()))
        packer = CANPacker(case['parser']['dbc'])
    addresses = [message[0] for message in case['parser']['messages']]
    for tick in range(65):
      frames = []
      for address in sorted(addresses):
        name = packer.dbc.addr_to_msg[address].name
        values = signals(brand, address, name, tick)
        encoded = packer.make_can_msg(name, case['parser']['bus'], values)
        frames.append({'address': encoded[0], 'data': list(encoded[1]), 'bus': encoded[2]})
      if tick == 8:
        frames = frames[:-1]
      if tick == 9:
        frames = frames[-1:]
      if tick == 10:
        frames.reverse()
        frames.insert(1, frames[0].copy())
      if tick == 15:
        frames[0] = {**frames[0], 'bus': 7}
      if tick == 16:
        frames[0] = {**frames[0], 'data': [0] * 65}
      if tick == 17:
        frames[0] = {**frames[0], 'data': frames[0]['data'][:-1]}
      if tick == 18:
        frames[0] = {**frames[0], 'data': []}
      step_ns = 40_000_000 if brand == 'volkswagen' else 30_303_030 if brand == 'ford' else 50_000_000
      case['actions'].append({'v_ego': 12.0 + .01 * tick, 'a_ego': -.2,
        'time': (1_000_000_000 + tick * step_ns) * 1e-9,
        'packets': [{'mono_time': 1_000_000_000 + tick * step_ns, 'frames': frames}]})
    output.append(case)
    fallback = {**case, 'name': f'decoder-{candidate}-unavailable', 'unavailable': True, 'actions': case['actions'][:12]}
    with contextlib.redirect_stdout(io.StringIO()):
      fallback['parser'] = metadata(fallback)
    output.append(fallback)
    DBC.cache_clear()
  return output


def signals(brand, address, name, tick):
  active = tick < 30 or tick >= 35
  distance = 20.0 - tick * .03
  match brand:
    case 'chrysler':
      if address >= 0x2c2:
        return {'LONG_DIST': distance if address == 0x2c2 and active else 0, 'LAT_DIST': 1.2}
      return {'REL_SPEED': -.5 + tick * .01}
    case 'gm':
      if address == 1120:
        return {'FLRRNumValidTargets': int(active), 'FLRRSnsrBlckd': int(tick == 24)}
      return {'TrkRange': distance if address == 1121 and active else 0, 'TrkObjectID': 7 if tick < 40 else 9,
              'TrkAzimuth': 5.0, 'TrkRangeRate': -.5 + tick * .01}
    case 'honda':
      if address == 0x400:
        return {'RADAR_STATE': 0x69 if tick == 24 else 0x79}
      return {'LONG_DIST': distance if address == 0x430 and active else 255, 'LAT_DIST': 1.2,
              'REL_SPEED': -.5 + tick * .01, 'NEW_TRACK': int(tick == 40)}
    case 'rivian':
      return {'STATE': 3 if address == 0x500 and active else 0, 'STATE_2': 1, 'LONG_DIST': distance,
              'AZIMUTH': 5.0, 'REL_SPEED': -.5 + tick * .01}
    case 'tesla':
      if name == 'RadarStatus':
        return {'shortTermUnavailable': int(tick == 23), 'sensorBlocked': int(tick == 24)}
      if name.endswith('_A'):
        return {'Tracked': int(name == 'RadarPoint0_A' and active), 'Index': 1,
                'LongDist': distance, 'LatDist': 1.2, 'LongSpeed': -.5 + tick * .01,
                'LongAccel': -.2, 'Meas': int(tick != 25)}
      return {'Index2': 2 if tick == 26 else 1, 'LatSpeed': .1}
    case 'toyota':
      if 'LONG_DIST' in name:
        raise AssertionError(name)
      if name.startswith('TRACK_B'):
        return {'SCORE': 60 if tick != 26 else 0}
      return {'LONG_DIST': distance if address in (0x180, 0x210) and active else 255,
              'VALID': int(address in (0x180, 0x210) and active and tick != 25),
              'LAT_DIST': 1.2, 'REL_SPEED': -.5 + tick * .01, 'NEW_TRACK': int(tick == 40)}
    case 'volkswagen':
      values = {'Distance_Status': int(tick == 24)}
      for lane in ('Same_Lane', 'Left_Lane', 'Right_Lane'):
        for index in (1, 2):
          prefix = f'{lane}_0{index}'
          identity = 10 if lane == 'Same_Lane' and (index == 1 or tick == 25) and active else 0
          values.update({prefix + '_ObjectID': identity, prefix + '_Long_Distance': distance,
            prefix + '_Lat_Distance': 1.2 + tick * .03, prefix + '_Rel_Velo': -.5 + tick * .01})
      return values
    case 'ford':
      scan = 3 if 30 <= tick < 37 else (tick + 1) % 4
      if name == 'MRR_Header_InformationDetections':
        return {'CAN_SCAN_INDEX': scan}
      if name == 'MRR_Header_SensorCoverage':
        return {'CAN_RANGE_COVERAGE': 0 if tick >= 44 else {0:42, 1:164, 2:45, 3:175}[scan]}
      index = int(name.rsplit('_', 1)[1])
      return {f'CAN_SCAN_INDEX_2LSB_{index:02d}': scan,
              f'CAN_DET_VALID_LEVEL_{index:02d}': int(index <= 2 and active),
              f'CAN_DET_RANGE_{index:02d}': 40.0 - tick * .03 + index,
              f'CAN_DET_AZIMUTH_{index:02d}': .02 * index,
              f'CAN_DET_RANGE_RATE_{index:02d}': -.5 + tick * .01}
    case _:
      raise AssertionError(brand)
