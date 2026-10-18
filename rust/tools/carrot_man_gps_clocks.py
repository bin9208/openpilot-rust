"""Validate live fix freshness before comparing independent process clocks.

Deterministic GPS/source fixtures compare exact receipt times separately. The
owned source/native processes run at different wall times, so their absolute
monotonic epochs cannot be equal at this boundary.
"""


def normalize(data):
  fix = data['positionFixMonoTime']
  now = data['positionMonoTime']
  source = data['positionSource']
  assert type(fix) is int and type(now) is int and 0 <= fix <= now
  assert source in ('none', 'external_navi', 'phone', 'vehicle', 'device')
  if source == 'none':
    assert fix == 0 and not data['positionHeadingValid']
  else:
    assert fix > 0 and now-fix < (2_500_000_000 if source == 'vehicle' else 3_000_000_000)
  # Retain source, coordinates and heading validity in the exact comparison.
  data.pop('positionFixMonoTime')
  data.pop('positionMonoTime')
