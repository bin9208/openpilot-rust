#!/usr/bin/env python3
import argparse
import hashlib
import json
from pathlib import Path

from controlsd_parameters import Store
from controlsd_source import load


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, default=Path('rust/crates/control-policy/data/registry.json'))
  args = parser.parse_args()
  source, _, _, _ = load(Store())
  from opendbc.car.gm.interface import NON_LINEAR_TORQUE_PARAMS
  from opendbc.car.honda.values import HONDA_BOSCH

  entries = []
  for name, interface in source['interfaces'].items():
    brand = interface.__module__.split('.')[-2]
    accel = ('honda_bosch' if name in HONDA_BOSCH else 'honda_nidec') if brand == 'honda' else brand if brand in ('gm', 'ford', 'toyota') else 'base'
    entries.append(
      {
        'fingerprint': name,
        'interface': brand,
        'accel': accel,
        'volt_feedforward': name in ('CHEVROLET_VOLT', 'CHEVROLET_VOLT_CC'),
        'siglin': NON_LINEAR_TORQUE_PARAMS.get(name),
      }
    )
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(sorted(entries, key=lambda item: item['fingerprint']), indent=2) + '\n')
  files = ['opendbc_repo/opendbc/car/values.py', 'opendbc_repo/opendbc/car/car_helpers.py']
  files += [str(path) for path in Path('opendbc_repo/opendbc/car').glob('*/values.py')]
  files += [str(path) for path in Path('opendbc_repo/opendbc/car').glob('*/interface.py')]
  provenance = {'count': len(entries), 'source_sha256': {name: hashlib.sha256(Path(name).read_bytes()).hexdigest() for name in files}}
  args.output.with_name('registry-provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
  print('Generated source interface registry:', len(entries))


if __name__ == '__main__':
  main()
