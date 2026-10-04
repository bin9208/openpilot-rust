from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil

from card_runtime_source import load_binding
from radarcan_runtime_cases import joined_cases, normal_cases


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--dbc', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  growth = 64 * 2**20
  free = shutil.disk_usage(args.output.parent).free
  assert free >= 25 * 2**30 + growth, (free, growth)
  args.output.mkdir()
  load_binding(args.binding)
  values = normal_cases(args.dbc.resolve())
  joined = joined_cases(next(case for case in values if case['candidate'] == 'VOLKSWAGEN_ID4_MK1'))
  corner = copy.deepcopy(next(case for case in values if case['name'] == 'ipc-hyundai-canfd-corners235-180'))
  corner.update(name='ipc-corners-front-only-latched-flip', flip=True, params_after_first={'RadarTrackFlip': '0'})
  joined.append(corner)
  for name, cases in [('normal', values), ('joined', joined)]:
    (args.output / (name + '.json')).write_text(json.dumps(cases) + '\n')
  (args.output / 'provenance.json').write_text(json.dumps({'free_before': free, 'growth_bound': growth,
    'normal_cases': len(values), 'joined_cases': len(joined), 'counts_from_original_interface': True,
    'params_binding_sha256': hashlib.sha256(args.binding.read_bytes()).hexdigest(),
    'assets_manifest_sha256': hashlib.sha256((args.dbc / 'radarcan-assets.json').read_bytes()).hexdigest()}, indent=2))


if __name__ == '__main__':
  main()
