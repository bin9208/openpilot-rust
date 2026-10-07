import argparse
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import subprocess
from card_identification_source import trace
from card_firmware_query_source import Input
from can_source import ROOT
from check_card_vehicle import compare
from check_card_firmware_query import normalized


def cases() -> list[dict]:
  catalog = json.loads((ROOT / 'rust/crates/card/data/firmware.json').read_text())
  common = dict(fixed='', selected=None, skip=False, disable_cache=False, pandas=1, cache=None, passive=[], io=Input(replies=[], clock_step=1 / 1024))
  result = []
  for model in catalog['models']:
    firmware = [dict(ecu=ecu['ecu'], address=ecu['address'], sub_address=ecu['subaddress'] or 0, brand=model['brand'], fw_version=ecu['versions'][0]) for ecu in model['firmware'] if ecu['versions']]
    for vin in ('00000000000000000', 'malformed', '1HGCM82633A004352'):
      result.append(common | dict(cache=dict(brand=model['brand'], vin=vin, firmware=firmware)))
  for selected, _ in catalog['selected']:
    result.append(common | dict(selected=selected, fixed='MOCK'))
  for skip, fixed, selected in ((True, '', None), (False, 'MOCK', None), (True, '', 'unrecognized')):
    result.append(common | dict(skip=skip, fixed=fixed, selected=selected))
  result.append(common)
  result.append(common | dict(disable_cache=True, cache=dict(brand='body', vin='00000000000000000', firmware=result[0]['cache']['firmware'])))
  return result


def normalize(value: dict) -> dict:
  result = value['result'].copy()
  result['firmware'] = sorted(result['firmware'], key=lambda fw: json.dumps(fw, sort_keys=True))
  result['ecu_responses'] = sorted(result['ecu_responses'], key=lambda target: (target[0], -1 if target[1] is None else target[1], target[2]))
  transport = normalized({'result': [], 'io': value['io']}, {'op':'firmware'})['io']
  logs = value['logs']
  for _, message in logs:
    if isinstance(message, dict) and message.get('event') == 'fingerprinted':
      message['ecu_responses'] = sorted(message['ecu_responses'], key=lambda target: (target[0], -1 if target[1] is None else target[1], target[2]))
  return dict(result=result, io=transport, logs=logs)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  request = cases()
  source_log = io.StringIO()
  with redirect_stdout(source_log), redirect_stderr(source_log):
    expected = [trace(case) for case in request]
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  (args.evidence / 'source.log').write_text(source_log.getvalue() or 'no source diagnostics\n')
  target = args.evidence / 'native.json'
  child = subprocess.run([args.binary.resolve(), target.resolve()], input=json.dumps(request), text=True, capture_output=True, check=False)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(target.read_text())
  left, right = [normalize(value) for value in expected], [normalize(value) for value in actual]
  (args.evidence / 'source-normalized.json').write_text(json.dumps(left) + '\n')
  (args.evidence / 'native-normalized.json').write_text(json.dumps(right) + '\n')
  try:
    compare(left, right)
  except AssertionError as error:
    (args.evidence / 'comparison-failure.txt').write_text(str(error) + '\n')
    raise
  result = dict(status='pass', cases=len(request), runtime_python=False, binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                observable='complete startup identification, VIN/cache/matching/manual/environment precedence, passive fingerprint, transport receipt',
                boundary='get_car vehicle factory replaced after identification; native brand construction validated separately',
                source_sha256=hashlib.sha256((ROOT / 'opendbc_repo/opendbc/car/car_helpers.py').read_bytes()).hexdigest())
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
