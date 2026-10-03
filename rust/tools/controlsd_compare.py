"""Decode native trace packets and enforce the fixed source controlsd contract."""

import argparse
import json
from pathlib import Path

from openpilot.cereal import log
from check_paramsd import compare


def compare_traces(expected, actual, evidence):
  for case in actual:
    for row in case['rows']:
      row['publications'] = []
      for packet in row.pop('packets'):
        with log.Event.from_bytes(bytes(packet)) as message:
          row['publications'].append({'service': message.which(), 'event': message.to_dict()})
  (evidence / 'native.json').write_text(json.dumps(actual, indent=2) + '\n')
  errors = []
  for case in expected:
    # These two CAN-parser initialization diagnostics belong to card's unused
    # constructor dependencies. Raw source.json retains them for inspection.
    case['printed'] = ''.join(
      line for line in case['printed'].splitlines(keepends=True) if not line.startswith('DBC: ') and line != 'Using Hyundai CAN FD checksum\n'
    )
  compare(expected, actual, 'controls', errors)
  result = {
    'pass': True,
    'cases': len(actual),
    'frames': sum(len(case['rows']) for case in actual),
    'numeric_comparisons': len(errors),
    'max_absolute_error': max(errors),
  }
  (evidence / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
  return result


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  source = json.loads((args.evidence / 'source.json').read_text())
  native = json.loads((args.evidence / 'native-raw.json').read_text())
  print(json.dumps(compare_traces(source, native, args.evidence)))
