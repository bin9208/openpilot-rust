from __future__ import annotations

from pathlib import Path
import copy
import tempfile
from pytest import MonkeyPatch


def trace(case):
  from opendbc.can.parser import CANParser, MessageState
  from opendbc.car.carlog import carlog
  with tempfile.TemporaryDirectory(prefix='radarcan-parser-') as directory:
    dbc = Path(directory) / (case['dbc_name'] + '.dbc')
    dbc.write_text(case['dbc'])
    with MonkeyPatch.context() as fixture:
      fixture.setattr('opendbc.can.parser.time.monotonic_ns', lambda: case['constructor_ns'])
      parser = CANParser(str(dbc), case['messages'], case['bus'])
    accumulated = set()
    output = []
    original_parse = MessageState.parse
    arrivals, warnings = [], []

    def capture_parse(state, nanos, data):
      accepted = original_parse(state, nanos, data)
      if accepted and state.address not in arrivals:
        arrivals.append(state.address)
      return accepted

    with MonkeyPatch.context() as fixture:
      fixture.setattr(MessageState, 'parse', capture_parse)
      fixture.setattr(carlog, 'warning', lambda message: warnings.append(message))
      for action in case['actions']:
        arrivals.clear()
        if action.get('clear'):
          accumulated.clear()
        packets = [(packet['mono_time'], [(frame['address'], bytes(frame['data']), frame['bus'])
          for frame in packet['frames']]) for packet in action['packets']]
        updated = parser.update(packets)
        accumulated.update(updated)
        valid = parser.can_valid
        states = {str(address): {'values': [parser.vl[address][signal.name] for signal in state.signals],
          'all_values': [parser.vl_all[address][signal.name] for signal in state.signals], 'timestamps': list(state.timestamps),
          'counter': state.counter, 'counter_fail': state.counter_fail, 'first_seen': state.first_seen_nanos,
          'last_warning': state.last_warning_log_nanos} for address, state in parser.message_states.items()}
        output.append(copy.deepcopy({'arrivals': list(arrivals), 'updated': list(updated), 'accumulated': list(accumulated),
          'states': states, 'valid': valid, 'invalid_count': parser.can_invalid_cnt,
          'last_nonempty': parser.last_nonempty_nanos, 'last_update': parser._last_update_nanos,
          'warnings': list(warnings)}))
        warnings.clear()
    return output


def cases():
  from opendbc.can.packer import CANPacker
  addresses = [706, 714, 722, 674, 682, 690, 698, 730, 738, 746, 754, 762]
  dbc = '\n'.join(f'BO_ {address} MSG_{address}: 8 XXX\n' +
      ' SG_ VALUE : 0|8@1+ (1,0) [0|255] "" XXX\n' +
      ' SG_ COUNTER : 60|2@1+ (1,0) [0|3] "" XXX\n' +
      ' SG_ CHECKSUM : 56|4@1+ (1,0) [0|15] "" XXX' for address in addresses) + '\n'
  with tempfile.TemporaryDirectory(prefix='radarcan-packer-') as directory:
    name = 'honda_radarcan_order'
    path = Path(directory) / (name + '.dbc')
    path.write_text(dbc)
    packer = CANPacker(str(path))

    def frame(address, value=1, counter=None):
      values = {'VALUE': value}
      if counter is not None:
        values['COUNTER'] = counter
      encoded = packer.make_can_msg(f'MSG_{address}', 1, values)
      return {'address': encoded[0], 'data': list(encoded[1]), 'bus': encoded[2]}

    tick = 1_000_000_000

    def action(frames, clear=False):
      nonlocal tick
      tick += 10_000_000
      return {'packets': [{'mono_time': tick, 'frames': frames}], 'clear': clear}

    checksum_bad = frame(714)
    checksum_bad['data'][7] ^= 1
    wrong_bus = frame(722)
    wrong_bus['bus'] = 0
    repeated = frame(674, counter=0)
    actions = [action([frame(722), frame(706), frame(722), checksum_bad, frame(714)]),
               action([wrong_bus, {'address': 706, 'data': [0] * 65, 'bus': 1}, frame(738), frame(690)]),
               action([frame(address) for address in reversed(addresses)]),
               action([frame(754), frame(706)], clear=True), action([frame(682), frame(730)]),
               *[action([repeated]) for _ in range(7)], action([], clear=True)]
    return [{'name': 'parser-accepted-arrival-and-set-merges', 'op': 'parser_sets', 'dbc_name': name, 'dbc': dbc,
             'messages': [[address, 100] for address in addresses], 'bus': 1,
             'constructor_ns': 1_000_000_000, 'actions': actions}]
