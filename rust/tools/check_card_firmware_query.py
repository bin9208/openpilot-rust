# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run using the existing oracle environment with PYTHONPATH=.:opendbc_repo.
import argparse
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import subprocess
from can_source import ROOT
from card_firmware_query_source import Case, Input, Reply, setup, trace
from check_can import compare


def cases() -> list[Case]:
  data = json.loads((ROOT / 'rust/crates/card/data/firmware.json').read_text())
  output: list[Case] = []
  for config in data['brands']:
    keys = {(ecu['ecu'], ecu['address'], ecu['subaddress']) for model in data['models'] if model['brand'] == config['brand'] for ecu in model['firmware']}
    keys |= {(ecu['ecu'], ecu['address'], ecu['subaddress']) for ecu in config['extra']}
    replies: list[Reply] = []
    present = []
    for request in config['requests']:
      for ecu, address, subaddress in sorted(keys, key=lambda key: (key[0], key[1], -1 if key[2] is None else key[2])):
        if request['whitelist'] and ecu not in request['whitelist']:
          continue
        mapped = (address & 0xffff0000) + ((address << 8) & 0xff00) + ((address >> 8) & 0xff) if address > 0x10000000 else address + request['offset']
        present.append((mapped, subaddress, request['bus']))
        replies.append(Reply(target=(address, subaddress), bus=request['bus'], offset=request['offset'], request=[0x3e, 0], response=[0x7e, 0]))
        for index, (sent, prefix) in enumerate(zip(request['request'], request['response'], strict=True)):
          payload = [0x55, 0xaa, 0] if index == len(request['request']) - 1 else []
          replies.append(Reply(target=(address, subaddress), bus=request['bus'], offset=request['offset'], request=sent, response=prefix + payload))
    for pandas in (0, 1, 2):
      output.append(Case(op='firmware', brand=config['brand'], pandas=pandas, timeout=1., io=Input(replies=replies, clock_step=1 / 1024)))
      output.append(Case(op='presence', pandas=pandas, io=Input(replies=replies, clock_step=1 / 1024)))
    output.append(Case(op='brand_matches', present=present))
    output.append(Case(op='ordered', present=present, vin='00000000000000000', pandas=2, timeout=1., io=Input(replies=replies, clock_step=1 / 1024)))
  output.append(Case(op='firmware', brand=None, pandas=0, timeout=0., io=Input(replies=[], clock_step=1 / 1024)))
  return output


def normalized(result, case: Case):
  catalog = json.loads((ROOT / 'rust/crates/card/data/firmware.json').read_text())
  subaddresses_by_address = {}
  for expected in [ecu for model in catalog['models'] for ecu in model['firmware']] + [ecu for brand in catalog['brands'] for ecu in brand['extra']]:
    if expected['subaddress'] is not None:
      subaddresses_by_address.setdefault(expected['address'], set()).add(expected['subaddress'])
  observed = result['result']
  match case['op']:
    case 'firmware' | 'ordered':
      observed = sorted(observed, key=lambda version: json.dumps(version, sort_keys=True))
    case 'presence':
      observed = sorted(observed, key=lambda target: (target[0], -1 if target[1] is None else target[1], target[2]))
    case 'brand_matches':
      observed = sorted(observed)
    case unreachable:
      from typing import assert_never
      assert_never(unreachable)
  groups = {}
  for frame in result['io']['sent']:
    subaddresses = subaddresses_by_address.get(frame['address'], set())
    subaddress = frame['data'][0] if frame['data'] and frame['data'][0] in subaddresses else None
    key = frame['address'], frame['bus'], -1 if subaddress is None else subaddress
    groups.setdefault(key, []).append(frame['data'])
  frames = [[list(key), values] for key, values in sorted(groups.items())]
  return dict(result=observed, io=dict(frames=frames, delays=result['io']['delays'], receives=result['io']['receives'],
                                     obd=result['io']['obd'], now=result['io']['now'], clock_reads=result['io']['clock_reads']))


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  request = cases()
  source_log = io.StringIO()
  with redirect_stdout(source_log), redirect_stderr(source_log):
    setup()
    expected = [trace(case) for case in request]
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  (args.evidence / 'source.log').write_text(source_log.getvalue() or 'source produced no diagnostics\n')
  target = args.evidence / 'native.json'
  child = subprocess.run([args.binary.resolve(), target.resolve()], input=json.dumps(request), text=True, capture_output=True, check=False)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(target.read_text())
  left = [normalized(value, case) for value, case in zip(expected, request, strict=True)]
  right = [normalized(value, case) for value, case in zip(actual, request, strict=True)]
  (args.evidence / 'source-normalized.json').write_text(json.dumps(left) + '\n')
  (args.evidence / 'native-normalized.json').write_text(json.dumps(right) + '\n')
  try:
    compare(left, right)
  except AssertionError as error:
    (args.evidence / 'comparison-failure.txt').write_text(str(error) + '\n')
    raise
  result = dict(status='pass', cases=len(request), runtime_python=False,
                binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                observable='firmware record fields, ECU presence, brand scores/order, pandas filtering, OBD callback sequence, per-target CAN protocol sequence, receive waits/delays/clock',
                unordered='source ECU set ordering and firmware record ordering normalized; each CAN target keeps its exact frame sequence',
                scope='Owned synthetic firmware-query transport; physical CAN, brand controllers and card daemon remain pending')
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
