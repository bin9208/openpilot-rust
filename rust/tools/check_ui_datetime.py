"""Compare native Params TIME parsing against the pinned CPython datetime parser."""

import argparse
import datetime
import itertools
import json
import random
import subprocess
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
cases = ['', '2026', '2026-10', '2026-10-01T', '0000-01-01', '9999-W52-7']
dates = [
  '2026-10-01',
  '20261001',
  '2026-W40',
  '2026-W40-4',
  '2026W40',
  '2026W404',
  '2020-W53-7',
  '2021-W53-1',
  '2000-02-29',
  '1900-02-29',
  '0001-01-01',
  '9999-12-31',
]
times = [
  '00',
  '01:02',
  '0102',
  '23:59:59',
  '235959',
  '24:00',
  '12:60',
  '12:34:60',
  '1',
  '12:3',
  '12:3456',
  '1234:56',
  '12.1',
  '12,1234567',
  '12:34.1',
  '1234,99',
  '12:34:56.123456789',
  '12:34:56.',
  '12:34:56.a',
  '12:34:56',
]
zones = [
  '',
  'Z',
  '+00',
  '-00:00',
  '+00:00:00.5',
  '+01:30',
  '-02:30:05.1234567',
  '+01.5',
  '+00:90',
  '+23:59:59.999999',
  '+24:00',
  '+Z',
  '+1',
  '+01:',
  '+010203',
  '+01:02:03',
  '+01:02:03,4',
]
cases += dates
cases += [d + s + t + z for d, s, t, z in itertools.product(dates, ['T', ' ', '🐍'], times, zones)]
rng = random.Random(148)
for _ in range(5000):
  text = rng.choice(cases)
  if text:
    i = rng.randrange(len(text))
    text = text[:i] + rng.choice(['', '0', '-', 'W', 'Z', ':', '.', ',', '9', 'T', '１', '\x00']) + text[i + 1 :]
  cases.append(text)
expected = []
for text in cases:
  try:
    v = datetime.datetime.fromisoformat(text)
    expected.append(
      {
        'local': f'{v.year:04}-{v.month:02}-{v.day:02}T{v.hour:02}:{v.minute:02}:{v.second:02}.{v.microsecond:06}',
        'offset': int((v.utcoffset() or datetime.timedelta()).total_seconds() * 1_000_000),
        'year': v.year,
      }
    )
  except ValueError:
    expected.append(None)
r = subprocess.run([str(a.binary)], input=''.join(json.dumps(x) + '\n' for x in cases), text=True, capture_output=True, check=True)
actual = [json.loads(x) for x in r.stdout.splitlines()]
assert len(actual) == len(expected)
differences = [{'input': t, 'source': s, 'native': n} for t, s, n in zip(cases, expected, actual, strict=True) if s != n]
a.output.parent.mkdir(parents=True, exist_ok=True)
a.output.write_text(json.dumps({'cases': len(cases), 'differences': differences}, indent=2))
assert not differences, differences[:20]
print(f'PASS: {len(cases)} CPython/native datetime conversions')
