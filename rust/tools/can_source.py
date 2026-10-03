"""Unchanged source CAN oracle. Only Params construction and the clock are boundaries."""

import ast
import hashlib
import json
from pathlib import Path
import sys
import types


ROOT = Path(__file__).resolve().parents[2]


class UnusedParams:
  def __init__(self, *args, **kwargs):
    raise RuntimeError('CAN codec oracle unexpectedly constructed Params')


def load():
  parameters = types.ModuleType('openpilot.common.params')
  parameters.Params = UnusedParams
  sys.modules[parameters.__name__] = parameters
  from opendbc.can.dbc import DBC
  from opendbc.can.packer import CANPacker
  from opendbc.can.parser import CANParser
  return DBC, CANPacker, CANParser


def metadata(dbc):
  def signal(s):
    kind = ('Default', 'Counter', 'Honda', 'Toyota', 'Body', 'Volkswagen', 'Xor', 'Subaru', 'Chrysler', 'HyundaiFd', 'Giorgio', 'Tesla', 'Psa', 'VolkswagenMlb')[s.type]
    if s.calc_checksum is not None and s.calc_checksum.__name__ == 'volkswagen_mqb_meb_gen2_checksum':
      kind = 'VolkswagenGen2'
    return dict(name=s.name, start_bit=s.start_bit, msb=s.msb, lsb=s.lsb, size=s.size, signed=s.is_signed,
                factor=s.factor, offset=s.offset, little_endian=s.is_little_endian, kind=kind)

  messages = {str(addr): dict(name=m.name, address=m.address, size=m.size, signals=[signal(s) for s in m.sigs.values()])
              for addr, m in dbc.msgs.items()}
  return dict(name=dbc.name, messages=messages, names={name: m.address for name, m in dbc.name_to_msg.items()},
              definitions=[dict(name=v.name, address=v.address, values=v.def_val) for v in dbc.vals])


def snapshot(parser, updated, checks):
  states = parser.message_states
  values = {str(a): list(parser.vl[a].values()) for a in states}
  return dict(updated=sorted(updated), checks=checks, bus_timeout=parser.bus_timeout, values=values,
              all_values={str(a): [list(parser.vl_all[a].get(s.name, [])) for s in state.signals] for a, state in states.items()},
              counters={str(a): str(s.counter) for a, s in states.items()}, failures={str(a): s.counter_fail for a, s in states.items()},
              frequencies={str(a): s.frequency for a, s in states.items()}, thresholds={str(a): s.timeout_threshold for a, s in states.items()},
              timestamps={str(a): list(s.timestamps) for a, s in states.items()}, raw={str(a): list(dat) for a, dat in parser.dat.items()},
              seen=sorted(parser.seen_addresses), last_nonempty=parser.last_nonempty_nanos, last_update=parser._last_update_nanos,
              invalid_count=parser.can_invalid_cnt)


def trace(case):
  DBC, CANPacker, CANParser = load()
  import opendbc.can.parser as source_parser
  clock = types.SimpleNamespace(monotonic_ns=lambda: case['now'])
  source_parser.time = clock
  parser = CANParser(case['path'], [], case['bus'])
  for name, frequency, ignore_counter in case['messages']:
    parser._add_message(name, frequency, ignore_counter)
  for name in case.get('ignored_messages', []):
    parser._add_message(name, float('nan'))
  packer = CANPacker(case['path'])
  outputs = []
  for step in case['steps']:
    try:
      match step['op']:
        case 'lazy':
          clock.monotonic_ns = lambda: step['now']
          outputs.append(dict(value=parser.vl[step['name']][step['signal']]))
        case 'define':
          from opendbc.can.parser import CANDefine
          definitions = CANDefine(case['path']).dv
          outputs.append(dict(definitions={str(a): {name: {str(value): text for value, text in entries.items()} for name, entries in signals.items()}
                                         for a, signals in definitions.items() if isinstance(a, int)}))
        case 'pack':
          outputs.append(dict(packed=list(packer.pack(step['address'], dict(step['values']), step['rx_counter']))))
        case 'ready':
          parser.controls_ready = step['enabled']
          outputs.append(dict(success=True))
        case 'add':
          clock.monotonic_ns = lambda: step['now']
          parser._add_message(step['name'], step['frequency'], step['ignore_counter'])
          outputs.append(dict(success=True))
        case 'update':
          packets = [[p['mono_time'], [(f['address'], bytes(f['data']), f['bus']) for f in p['frames']]] for p in step['packets']]
          updated = parser.update(packets)
          checks = [parser.can_valid for _ in range(step['checks'])]
          outputs.append(snapshot(parser, updated, checks))
        case unknown:
          raise ValueError(f'unknown oracle operation {unknown}')
    except (TypeError, ValueError, KeyError, IndexError, AssertionError, OverflowError, ZeroDivisionError) as error:
      outputs.append(dict(error=type(error).__name__, detail=str(error)))
  return dict(dbc=metadata(DBC(case['path'])), steps=outputs)


def generate_constants():
  source = ROOT / 'opendbc_repo/opendbc/car/volkswagen/mqbcan.py'
  tree = ast.parse(source.read_text())
  names = ('VOLKSWAGEN_MQB_MEB_CONSTANTS', 'VOLKSWAGEN_MQB_MEB_GEN2_CONSTANTS')
  nodes = [n for n in tree.body if isinstance(n, ast.AnnAssign) and isinstance(n.target, ast.Name) and n.target.id in names]
  scope = {}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(source), 'exec'), scope)
  output = ['// Generated from unchanged source checksum data; generator/provenance in rust/tools/can_source.py.',
            'pub const MQB: &[(u32, [u8; 16])] = &[']
  output.extend(f'    ({addr}, {values}),' for addr, values in scope[names[0]].items())
  output.extend(['];', 'pub const GEN2: &[(u32, usize, [u8; 16])] = &['])
  output.extend(f'    ({addr}, {entry["length"]}, {entry["magic"]}),' for addr, entry in scope[names[1]].items())
  output.append('];')
  target = ROOT / 'rust/crates/can/src/volkswagen_constants.rs'
  target.write_text('\n'.join(output) + '\n')
  files = list((ROOT / 'opendbc_repo/opendbc/can').glob('*.py'))
  files.extend(ROOT / f'opendbc_repo/opendbc/car/{p}' for p in ['crc.py', 'honda/hondacan.py', 'toyota/toyotacan.py', 'subaru/subarucan.py',
               'chrysler/chryslercan.py', 'hyundai/hyundaicanfd.py', 'volkswagen/mqbcan.py', 'volkswagen/mlbcan.py', 'tesla/teslacan.py', 'body/bodycan.py', 'psa/psacan.py'])
  provenance = dict(source_commit='31d7306882218e9fecc44aba0d5c034f0d1ca188', runtime_python=False,
                    source_sha256={str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files})
  (target.parent.parent / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')


if __name__ == '__main__':
  generate_constants()
