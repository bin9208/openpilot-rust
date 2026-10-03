# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run with PYTHONPATH=.:opendbc_repo in the existing source-oracle environment.
import argparse
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import subprocess
from can_source import ROOT
from card_diagnostic_cases import cases
from card_diagnostic_source import trace
from check_can import compare


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
  (args.evidence / 'source.log').write_text(source_log.getvalue() or 'source produced no diagnostics\n')
  target = args.evidence / 'native.json'
  child = subprocess.run([args.binary.resolve(), target.resolve()], input=json.dumps(request), text=True, capture_output=True, check=False)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(target.read_text())
  for case, result in zip(request, actual, strict=True):
    if case['op'] == 'scan':
      result['io']['sent'].sort(key=lambda frame: (frame['address'], frame['data'], frame['bus']))
  try:
    compare(expected, actual)
  except AssertionError as error:
    (args.evidence / 'comparison-failure.txt').write_text(str(error) + '\n')
    raise
  result = dict(status='pass', cases=len(request), runtime_python=False,
                source_sha256={name: hashlib.sha256((ROOT / 'opendbc_repo/opendbc/car' / name).read_bytes()).hexdigest()
                               for name in ('vin.py', 'ecu_addrs.py', 'disable_ecu.py')},
                binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                observable='VIN decode/validation, VIN query order/result, ECU response set, init retries, exact CAN address/bus/bytes, delay, receive waits and clock',
                unordered='ECU scan query-set frame order normalized; other query sends and waits retain exact order')
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
