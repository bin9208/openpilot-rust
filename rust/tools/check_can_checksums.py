"""Exercise every unchanged source checksum plugin, including GEN2 fallback and known source failures."""

import argparse
import ast
from contextlib import redirect_stderr, redirect_stdout
import io
import json
from pathlib import Path
import random
import subprocess
import types

from can_source import ROOT, load


def cases():
  load()
  from opendbc.car.volkswagen.mqbcan import VOLKSWAGEN_MQB_MEB_CONSTANTS, VOLKSWAGEN_MQB_MEB_GEN2_CONSTANTS
  from opendbc.car.crc import CRC8H2F
  names = ['honda_test', 'toyota_test', 'comma_body', 'vw_mqb', 'vw_meb_2024', 'vw_pq', 'subaru_global_test', 'chrysler_test',
           'hyundai_canfd_generated', 'fca_giorgio', 'tesla_model3_party', 'psa_test', 'vw_mlb']
  rng = random.Random(177)
  result = []
  for name in names:
    for length in [0, 1, 2, 3, 4, 5, 6, 7, 8, 16, 24, 32, 48, 64]:
      for index in range(50):
        data = [rng.randrange(256) for _ in range(length)]
        address = rng.choice([0x109, 0x111, 0x30c, 0x324, 0x10b, 0x10d, 0x10f, 0x311, 0x397, 0x10c, 0xde, 0x106, 0x122, 0x452, 0x38d, 0x42d, 0x7ff, 0x18DAF110])
        result.append(dict(dbc=name, address=address, start_bit=0 if index % 2 else 7, data=data))
  for address in VOLKSWAGEN_MQB_MEB_CONSTANTS:
    for counter in range(16):
      data = [rng.randrange(256) for _ in range(8)]
      data[1] = counter
      result.append(dict(dbc='vw_mqb', address=address, start_bit=0, data=data))
  for address, entry in VOLKSWAGEN_MQB_MEB_GEN2_CONSTANTS.items():
    for counter in range(16):
      data = [rng.randrange(256) for _ in range(64)]
      data[1] = counter
      crc = 255
      for byte in data[1:entry['length']]:
        crc = CRC8H2F[crc ^ byte]
      data[0] = CRC8H2F[crc ^ entry['magic'][counter]] ^ 255
      result.append(dict(dbc='vw_meb_2024', address=address, start_bit=0, data=data))
      changed = data.copy()
      changed[0] ^= 1
      result.append(dict(dbc='vw_meb_2024', address=address, start_bit=0, data=changed))
  known = ast.parse((ROOT / 'opendbc_repo/opendbc/can/tests/test_checksums.py').read_text())
  groups = {'verify_fca_giorgio_crc': 'fca_giorgio', 'verify_volkswagen_mqb_crc': 'vw_mqb'}
  for node in ast.walk(known):
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr in groups and len(node.args) == 4:
      if isinstance(node.args[2], ast.Constant) and isinstance(node.args[3], ast.List):
        address, vectors = ast.literal_eval(node.args[2]), ast.literal_eval(node.args[3])
        for data in vectors:
          result.append(dict(dbc=groups[node.func.attr], address=address, start_bit=0, data=list(data), known_good=True))
  return result


def oracle(case):
  from opendbc.can.dbc import get_checksum_state
  data = bytearray(case['data'])
  state = get_checksum_state(case['dbc'])
  signal = types.SimpleNamespace(start_bit=case['start_bit'])
  try:
    checksum = state.calc_checksum(case['address'], signal, data)
    return dict(checksum=checksum, data=list(data), error=None)
  except (TypeError, IndexError) as error:
    return dict(checksum=None, data=list(data), error=type(error).__name__ + ': ' + str(error))


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  log = io.StringIO()
  with redirect_stdout(log), redirect_stderr(log):
    request = cases()
    expected = [oracle(case) for case in request]
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  (args.evidence / 'source.log').write_text(log.getvalue())
  target = args.evidence / 'native.json'
  child = subprocess.run([args.binary.resolve(), target.resolve()], input=json.dumps(request), text=True, capture_output=True)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(target.read_text())
  assert len(expected) == len(actual)
  known_failures = []
  for index, (one, two) in enumerate(zip(expected, actual, strict=True)):
    assert one['checksum'] == two['checksum'], (index, request[index], one, two)
    assert one['data'] == two['data'], (index, request[index], one, two)
    assert (one['error'] is None) == (two['error'] is None), (index, request[index], one, two)
    if request[index].get('known_good'):
      checksum_byte = -1 if request[index]['dbc'] == 'fca_giorgio' else 0
      recorded = request[index]['data'][checksum_byte]
      if one['checksum'] != recorded:
        # Pin the verified inherited vector defect; new source/vector changes fail this gate.
        assert (request[index]['dbc'], request[index]['address'], request[index]['data'], one['checksum']) == ('fca_giorgio', 0x122, [0x7b, 0xf0, 0x02, 0x6e], 0xe1)
        known_failures.append(dict(input=request[index], source=one, native=two, recorded_checksum=recorded))
      else:
        assert two['checksum'] == recorded, (index, request[index], two)
    if one['error'] is not None and one['error'].startswith('TypeError'):
      assert 'inherited Volkswagen MLB' in two['error'], (index, one, two)
  (args.evidence / 'known-source-vector-failures.json').write_text(json.dumps(known_failures, indent=2) + '\n')
  result = dict(status='native_matches_source_with_known_vector_failure', cases=len(request), kinds=len({case['dbc'] for case in request}),
                source_vectors=sum(case.get('known_good', False) for case in request),
                verified_known_good_vectors=sum(case.get('known_good', False) for case in request) - len(known_failures),
                inherited_vector_failures=len(known_failures),
                source_failures=sum(case['error'] is not None for case in expected), observable='checksum integer, payload mutation and failure presence exact')
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
