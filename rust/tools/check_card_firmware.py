# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run with the existing source-oracle environment and PYTHONPATH=.:opendbc_repo.
import argparse
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import subprocess
from can_source import ROOT
from card_firmware_source import Case, Firmware, setup, trace
from check_can import compare


def cases() -> list[Case]:
  data = json.loads((ROOT / 'rust/crates/card/data/firmware.json').read_text())
  result: list[Case] = [Case(op='match', versions=[], vin='00000000000000000', exact=True, fuzzy=True)]
  for model in data['models']:
    versions = [Firmware(ecu=ecu['ecu'], address=ecu['address'], sub_address=ecu['subaddress'] or 0,
                         fw_version=ecu['versions'][0], brand=model['brand'], logging=False) for ecu in model['firmware'] if ecu['versions']]
    vin = list('AAAAAAAAAAAAAAAAA')
    if model['wmis']: vin[:3] = model['wmis'][0]
    if model['lines']: vin[3] = model['lines'][0]
    if model['chassis']: vin[6:8] = model['chassis'][0]
    if model['years']: vin[9] = model['years'][0]
    for exact, fuzzy in ((True, False), (False, True), (True, True), (False, False)):
      result.append(Case(op='match', versions=versions, vin=''.join(vin), exact=exact, fuzzy=fuzzy))
    for index in range(len(versions)):
      result.append(Case(op='match', versions=versions[:index] + versions[index + 1:], vin=''.join(vin), exact=True, fuzzy=True))
      changed = [dict(entry) for entry in versions]
      changed[index]['logging'] = True
      result.append(Case(op='match', versions=changed, vin=''.join(vin), exact=True, fuzzy=True))
      changed = [dict(entry) for entry in versions]
      changed[index]['fw_version'] = [0, 0xfe, 0xff]
      result.append(Case(op='match', versions=changed, vin=''.join(vin), exact=True, fuzzy=True))
    result.append(Case(op='match', versions=versions, vin='00000000000000000', exact=False, fuzzy=True))
    if model['brand'] in ('ford', 'hyundai', 'toyota'):
      for expected in model['firmware']:
        for version in expected['versions']:
          for suffix in ([], [10], [0], [32], [255]):
            result.append(Case(op='codes', brand=model['brand'], versions=[version + suffix]))
  for name, platform in data['selected']:
    result.append(Case(op='select', name=name))
    result.append(Case(op='select', name=name + ' '))
    result.append(Case(op='select', name=name.lower()))
  return result


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
  try:
    compare(expected, actual)
  except AssertionError as error:
    (args.evidence / 'comparison-failure.txt').write_text(str(error) + '\n')
    raise
  result = dict(status='pass', cases=len(request), runtime_python=False,
                binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                catalog_sha256=hashlib.sha256((ROOT / 'rust/crates/card/data/firmware.json').read_bytes()).hexdigest(),
                observable='exact and fuzzy candidate sets/source, logging/missing/incorrect ECU handling, manual selections, brand platform-code/date extraction',
                scope='Firmware matching and choices; active firmware queries, complete controllers and card daemon remain pending')
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
