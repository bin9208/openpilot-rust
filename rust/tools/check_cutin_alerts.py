import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import struct
import subprocess

from openpilot.selfdrive.controls.lib.cutin_alert import CutinAlertCandidate, CutinAlertTracker, promoted_cutin_candidates

ROOT = Path(__file__).resolve().parents[2]


def encode(candidate):
  return {'track_id': candidate.track_id, 'bits': [struct.unpack('<Q', struct.pack('<d', value))[0]
          for value in (candidate.d_rel, candidate.y_rel, candidate.v_rel)]}


def decode(candidate):
  return CutinAlertCandidate(candidate['track_id'], *(struct.unpack('<d', struct.pack('<Q', value))[0] for value in candidate['bits']))


def cases():
  result = []

  def add(candidates, lead=None, enabled=True, reset=False, promote=False):
    result.append({'candidates': [encode(value) for value in candidates], 'lead_two': encode(lead) if lead is not None else None,
                   'enabled': enabled, 'reset': reset, 'promote': promote})

  origin = CutinAlertCandidate(1, 0., 0., 0.)
  for identity in (-2147483648, -1, 0, 1, 2, 2147483647):
    for axis, limit in enumerate((3., 1., 5.) if identity == 1 else (1.5, .75, 2.5)):
      for sign in (-1., 1.):
        for offset in (math.nextafter(limit, -math.inf), limit, math.nextafter(limit, math.inf)):
          coordinates = [0., 0., 0.]
          coordinates[axis] = sign * offset
          add([origin], reset=True)
          add([CutinAlertCandidate(identity, *coordinates)])
  for identity in (-2, -1, 0, 1):
    for axis in range(3):
      for offset in (-math.inf, -.1, -0., 0., math.nextafter(.1, -math.inf), .1, math.nextafter(.1, math.inf), math.inf, math.nan):
        coordinates = [0., 0., 0.]
        coordinates[axis] = offset
        candidate = CutinAlertCandidate(identity, *coordinates)
        lead = CutinAlertCandidate(identity, 0., 0., 0.)
        add([candidate, candidate, origin], lead, reset=True, promote=True)
        add([candidate], lead, promote=True)
        add([candidate], lead, enabled=False, promote=True)
        add([candidate], lead, promote=True)
        add([candidate], None, promote=True)
  for value in (math.nan, math.inf, -math.inf, -0., 0.):
    for axis in range(3):
      coordinates = [0., 0., 0.]
      coordinates[axis] = value
      candidate = CutinAlertCandidate(1, *coordinates)
      add([candidate], reset=True)
      add([candidate])
  rng = random.Random(168)
  previous = [origin]
  for index in range(5000):
    candidates = [CutinAlertCandidate(rng.randrange(-2, 5), *(rng.uniform(-10., 10.) for _ in range(3))) for _ in range(rng.randrange(5))]
    if index % 3 == 0:
      candidates += previous
    add(candidates, rng.choice(candidates) if candidates and index % 7 else None, index % 11 != 0, index % 89 == 0, index % 2 == 0)
    previous = candidates[:2]
  return result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  rows, expected = cases(), []
  tracker = CutinAlertTracker()
  for row in rows:
    if row['reset']:
      tracker.reset()
    candidates = tuple(decode(value) for value in row['candidates'])
    lead = decode(row['lead_two']) if row['lead_two'] is not None else None
    selected = promoted_cutin_candidates(candidates, lead) if row['promote'] else candidates
    alert = tracker.update(selected, row['enabled'])
    expected.append({'alert': alert, 'selected': [encode(value) for value in selected], 'previous': [encode(value) for value in tracker.previous]})
  payload = ''.join(json.dumps(row) + '\n' for row in rows)
  (args.output / 'input.jsonl').write_text(payload)
  (args.output / 'source.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in expected))
  native = subprocess.run([args.binary.resolve()], input=payload, text=True, capture_output=True, timeout=20)
  (args.output / 'native.jsonl').write_text(native.stdout)
  (args.output / 'native.stderr').write_text(native.stderr)
  assert native.returncode == 0, native.stderr
  actual = [json.loads(line) for line in native.stdout.splitlines()]
  assert len(actual) == len(expected), (len(actual), len(expected))
  for index, (want, got) in enumerate(zip(expected, actual, strict=True)):
    assert got == want, (index, rows[index], want, got)
  source = ROOT / 'openpilot/selfdrive/controls/lib/cutin_alert.py'
  report = {'result': 'pass', 'frames': len(rows), 'exact_alert_selection_state': True,
            'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
            'scope': 'unchanged source cut-in audio alert policy; no detection/lead-selection or device changes'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
