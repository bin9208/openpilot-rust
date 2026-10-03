#!/usr/bin/env python3
import argparse
import copy
import json
import os
from pathlib import Path
import subprocess

from controlsd_compare import compare_traces
from controlsd_fixture import cases
from controlsd_scenarios import cases as scenarios
from controlsd_source import trace


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  parser.add_argument('--numerics', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  request = {'cases': cases() + scenarios()}
  for token in ['1_0', '0x_A', '0Xf_f', '0b_10', '0o_10', '0_0', '00', '- 1', '+ 1', '-2147483648']:
    case = copy.deepcopy(request['cases'][3])
    case['name'] = 'fingerprints-valid-' + token
    case['frames'] = case['frames'][:5]
    case['params']['FingerPrints'] = list(('{0: {' + token + ': 8}, 1: {}, 2: {}, 3: {}}').encode())
    request['cases'].append(case)
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  expected = [trace(case) for case in request['cases']]
  (args.evidence / 'source.json').write_text(json.dumps(expected, indent=2) + '\n')
  output = args.evidence / 'native-raw.json'
  child = subprocess.run(
    [args.trace.resolve(), output.resolve()],
    input=json.dumps(request),
    text=True,
    capture_output=True,
    env=os.environ | {'CONTROLS_NUMERICS': str(args.numerics.resolve())},
  )
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(output.read_text())
  print(json.dumps(compare_traces(expected, actual, args.evidence)))


if __name__ == '__main__':
  main()
