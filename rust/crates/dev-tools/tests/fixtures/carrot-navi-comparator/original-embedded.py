import json, os
from pathlib import Path
from radarcan_exact import assert_exact
root = Path(os.environ['RUNNER_TEMP']) / 'carrot-navi'
def read(path):
  return json.loads((root / path).read_text())
def normalize(rows):
  sessions = {}
  def walk(value, key=''):
    if isinstance(value, dict):
      return {k: walk(v, k) for k, v in value.items() if k != 'cereal_publish_count'}
    if isinstance(value, list):
      return [walk(v) for v in value]
    if key == 'session_id' and isinstance(value, str):
      sessions.setdefault(value, f'<session{len(sessions)}>')
    if isinstance(value, str):
      for token, replacement in sessions.items():
        value = value.replace(token, replacement)
    if key == 'port' and value != 7714:
      return '<bound-port>'
    if key in ('last_received_at_ms', 'received_at_ms', 'last_cereal_publish_mono_ns') and value:
      assert isinstance(value, int) and value > 0
      return '<positive-clock>'
    return value
  return walk(rows)
assert_exact(read('policy-native.json'), read('policy-source.json'), 'policy')
for lane, count in (('socket', 56), ('lifecycle', 21)):
  source = normalize(read(f'{lane}-source/rows.json'))
  native = normalize(read(f'{lane}-native/rows.json'))
  assert len(source) == len(native) == count
  assert_exact(native, source, lane)
  assert_exact(read(f'{lane}-native/result.json'), read(f'{lane}-source/result.json'), lane + '/result')
  for kind, rows in (('source', source), ('native', native)):
    (root / f'{lane}-{kind}-normalized.json').write_text(json.dumps(rows, indent=2))
for lane in ('startup', 'exhaust'):
  assert_exact(read(f'{lane}-native/result.json'), read(f'{lane}-source/result.json'), lane)
source_packets = read('startup-source/discovery.json')
native_packets = read('startup-native/discovery.json')
assert_exact([row['wire_hex'] for row in native_packets], [row['wire_hex'] for row in source_packets], 'discovery/wire')
(root / 'comparison.json').write_text(json.dumps({'status': 'pass', 'policy_cases': len(read('policy-source.json')), 'socket_observations': 56, 'lifecycle_observations': 21, 'discovery_byte_equal': True}, indent=2))
