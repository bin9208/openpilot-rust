"""Verify production prepared replay against complete pinned-source outputs.

Only source scenarios that need no test-only predictor/reset injection qualify.
The CLI receives ordinary production controller inputs and recorded inversion.
"""

import argparse
import json
import subprocess
from pathlib import Path
from radard_compare import differences


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--fixture', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  request = json.loads((args.fixture / 'input.json').read_text())
  expected = json.loads((args.fixture / 'expected.json').read_text())
  owners = {}
  for action, result in zip(request['actions'], expected, strict=True):
    owner = action['owner']
    if action['op'] == 'create':
      owners[owner] = {'options': action['options'], 'frames': [], 'expected': [], 'eligible': True}
    else:
      group = owners[owner]
      group['eligible'] &= (
        not action.get('predictor_override') and not action.get('primary_reset') and action['mode'] == group['options']['enable_radar_tracks']
      )
      group['frames'].append({'input': action['input'], 'recorded_flipped': False})
      group['expected'].append(result['output'])
  frames = 0
  counts = 0
  for owner, group in owners.items():
    if not group['eligible'] or not group['frames']:
      continue
    path = args.output / f'owner-{owner}.jsonl'
    path.write_text('\n'.join(json.dumps(v, allow_nan=False) for v in [{'options': group['options']}, *group['frames']]) + '\n')
    result = subprocess.run([str(args.binary.resolve()), 'prepared', str(path.resolve())], check=True, capture_output=True, text=True)
    actual = [json.loads(line)['output'] for line in result.stdout.splitlines()[1:]]
    mismatch = differences(group['expected'], actual)
    assert not mismatch, f'complete replay output differs for source owner {owner}: {mismatch[:3]}'
    frames += len(actual)
    counts += 1
  assert frames > 100, 'insufficient production-compatible final source coverage'
  receipt = {
    'source_commit': 'e8e152dbcfd9574a1791ed12026d1880e87689b3',
    'owners': counts,
    'frames': frames,
    'status': 'passed',
    'scope': 'full outputs for production-compatible scenarios; test-only predictor/reset scenarios excluded',
  }
  (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()
