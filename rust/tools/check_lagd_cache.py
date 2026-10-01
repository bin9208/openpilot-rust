#!/usr/bin/env python3
"""Unchanged cached-lag restore/reject/remove policy and actual private Params files."""

import argparse
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
from openpilot.cereal import car
from lagd_frames import car_params, cached
from lagd_reference import source, normalized


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  cp = car_params()
  cases = [
    ('missing', None, cp),
    ('empty', b'', cp),
    ('valid', cached(), cp),
    ('unestimated', cached(blocks=1, status='unestimated'), cp),
    ('invalid', cached(status='invalid'), cp),
    ('excess', cached(blocks=51), cp),
    ('negative', cached(blocks=-1), cp),
    ('wrong-car', cached(), car_params('OTHER')),
    ('missing-previous', cached(), None),
    ('bad-previous', cached(), b'bad'),
    ('truncated', b'bad', cp),
    ('nan', cached(lag=float('nan')), cp),
    ('infinity', cached(lag=float('inf')), cp),
    ('unknown-status', cached(status=3), cp),
  ]
  records = []
  for name, saved, previous in cases:
    stored = {'LiveDelay': saved, 'CarParamsPrevRoute': previous}
    scope = source()
    scope['cloudlog'] = SimpleNamespace(error=lambda value: None)
    params = SimpleNamespace(get=lambda key, stored=stored: stored.get(key) or None, remove=lambda key, stored=stored: stored.pop(key, None))
    with car.CarParams.from_bytes(cp) as current:
      seed = scope['retrieve_initial_lag'](params, current)
    expected = {'seed': normalized(seed), 'remaining': len(stored['LiveDelay']) if stored.get('LiveDelay') is not None else None}
    with tempfile.TemporaryDirectory(prefix='lagd-cache-') as root:
      request = {
        'root': root,
        'car': list(cp),
        'saved': list(saved) if saved is not None else None,
        'previous': list(previous) if previous is not None else None,
      }
      result = subprocess.run([args.binary], input=json.dumps(request) + '\n', text=True, capture_output=True, check=True, timeout=5)
      actual = json.loads(result.stdout)
    assert actual == expected, (name, expected, actual)
    records.append({'case': name, 'source': expected, 'native': actual, 'stderr': result.stderr, 'pass': True})
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(records, indent=2) + '\n')
  print(f'PASS: {len(records)} source/native Params restore/remove cases')


if __name__ == '__main__':
  main()
