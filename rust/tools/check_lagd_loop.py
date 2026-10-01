#!/usr/bin/env python3
"""Original main-loop ordering, masks, recovery, block and cache/publication parity."""

import argparse
import json
from pathlib import Path
import subprocess
from openpilot.cereal import log
from lagd_frames import car_params, cached, messages
from lagd_loop_reference import run
from lagd_reference import ROOT, compare, normalized


def decode(row):
  result = dict(row)
  if result['packet'] is not None:
    with log.Event.from_bytes(bytes(result['packet'])) as event:
      result['packet'] = normalized(event.to_dict())
  return result


def scenario(binary, output, mode, seed):
  config = {'car': list(car_params()), 'saved': list(seed) if seed is not None else None, 'previous': list(car_params())}
  frames = []
  count = 1301 if mode == 'recovery' else 1201
  for index in range(count):
    row = {
      'configuration': config if index == 0 else None,
      'now': 100.0 + index * 0.05,
      'messages': [list(value) for value in messages(index, mode)],
      'debug': True,
    }
    if mode == 'ordering':
      row['messages'].reverse()
      for offset, data in enumerate(row['messages']):
        with log.Event.from_bytes(bytes(data)) as event:
          event = event.as_builder()
          event.logMonoTime += offset * 1000
          row['messages'][offset] = list(event.to_bytes())
    frames.append(row)
  result = subprocess.run([binary], input=''.join(json.dumps(row) + '\n' for row in frames), text=True, capture_output=True, timeout=60, check=True)
  native = [decode(json.loads(line)) for line in result.stdout.splitlines()]
  expected = [decode(row) for row in run(frames, config)]
  assert len(native) == len(expected) == count
  budget = json.loads((ROOT / 'rust/crates/lagd/tests/tolerances.json').read_text())['cereal_float32']
  for index, (want, actual) in enumerate(zip(expected, native, strict=True)):
    compare(want, actual, budget, f'{mode}[{index}]')
  output.mkdir(parents=True, exist_ok=True)
  (output / 'source.json').write_text(json.dumps([row for row in expected if row['publish']], indent=2) + '\n')
  (output / 'native.json').write_text(json.dumps([row for row in native if row['publish']], indent=2) + '\n')
  persisted = [row['frame'] for row in native if row['persist']]
  assert persisted == [0, 1200]
  (output / 'result.json').write_text(
    json.dumps({'frames': count, 'publications': sum(row['publish'] for row in native), 'persisted': persisted, 'pass': True}, indent=2) + '\n'
  )
  print(f'PASS: {mode}, {count} full source/native loop frames', flush=True)


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  for mode, seed in [('normal', None), ('recovery', cached()), ('ordering', cached(blocks=50))]:
    scenario(args.binary, args.output / mode, mode, seed)


if __name__ == '__main__':
  main()
