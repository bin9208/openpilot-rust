"""Comparison eligibility from observed immutable inputs; no IPC/native imports."""

from dataclasses import dataclass
import hashlib

TOPICS = ('pandaStates', 'carControl', 'onroadEvents', 'carrotMan', 'longitudinalPlan', 'radarState', 'modelV2', 'drivingModelData', 'liveParameters', 'livePose', 'selfdriveState', 'lateralPlan', 'customReservedRawData0')
MAX_ATTEMPTS = 3
MAX_TRACE_FRAMES = 2000


@dataclass(frozen=True)
class Attempt:
  source_history: list
  native_history: list
  expected: object
  actual: object


class AdmissionMismatch(AssertionError):
  pass


def build_registry(rows, decode):
  """Bind each positive topic ID to exactly one frozen prebuilt event."""
  registry = {}
  assert rows, 'missing immutable inputs'
  for row in rows:
    assert set(row['messages']) == set(TOPICS[:-1]), 'unexpected published topic set'
    for name, values in row['messages'].items():
      wire = bytes(values)
      topic, stamp, valid = decode(wire)
      assert topic == name, ('wrong wire topic', name, topic)
      assert type(stamp) is int and stamp > 0 and type(valid) is bool, 'invalid wire identity'
      key = (name, stamp)
      assert key not in registry, ('duplicate immutable input identity', key)
      registry[key] = {'log_mono_time': stamp, 'wire_sha256': hashlib.sha256(wire).hexdigest(), 'valid': valid}
  return registry


def validate_history(rows, registry, end_frame):
  """Validate every receipt; compare only frame zero through measured end."""
  assert type(end_frame) is int and 0 <= end_frame < MAX_TRACE_FRAMES
  assert end_frame < len(rows) <= MAX_TRACE_FRAMES, 'missing or excessive trace frames'
  normalized = []
  previous = {}
  for frame, row in enumerate(rows):
    assert type(row['frame']) is int and row['frame'] == frame, 'non-contiguous admission trace'
    topics = row['admission']
    assert len(topics) == len(TOPICS) and {t['name'] for t in topics} == set(TOPICS), 'invalid admission topic set'
    by_name = {t['name']: t for t in topics}
    current = []
    for name in TOPICS:
      topic = by_name[name]
      seen, updated = topic['seen'], topic['updated']
      received, stamp = topic['receive_frame'], topic['log_mono_time']
      assert type(seen) is bool and type(updated) is bool
      assert type(received) is int and 0 <= received <= frame
      assert type(stamp) is int and stamp >= 0
      if not seen:
        assert not updated and received == stamp == 0, 'invalid unseen sentinel'
        assert name not in previous or not previous[name]['seen'], 'seen topic became unseen'
        identity = None
      else:
        assert (name, stamp) in registry, ('unknown admitted identity', name, stamp)
        identity = registry[(name, stamp)]
        if updated:
          assert received == frame, 'updated topic has wrong receipt frame'
        else:
          assert name in previous and previous[name]['seen'], 'first receipt not updated'
          assert (stamp, received) == (previous[name]['log_mono_time'], previous[name]['receive_frame']), 'stale receipt changed'
      current.append({'name': name, 'seen': seen, 'updated': updated, 'receive_frame': received, 'identity': identity})
    previous = by_name
    if frame <= end_frame:
      normalized.append({'frame': frame, 'admission': current})
  return normalized


def history_differences(source, native):
  result = []
  if len(source) != len(native):
    result.append({'kind': 'frame_count', 'source': len(source), 'native': len(native)})
  for frame, (left, right) in enumerate(zip(source, native, strict=False)):
    if left != right:
      result.append({'frame': frame, 'source': left, 'native': right})
      if len(result) == 10:
        break
  return result


def run_aligned(prepare, compare, record):
  """Only valid but unequal input histories permit another independent pair."""
  for number in range(1, MAX_ATTEMPTS + 1):
    record(number, {'status': 'checking_strict_prerequisites'})
    try:
      attempt = prepare(number)
    except Exception as error:
      record(number, {'status': 'hard_failure', 'type': type(error).__name__, 'error': str(error)[:4096]})
      raise
    differences = history_differences(attempt.source_history, attempt.native_history)
    if differences:
      record(number, {'status': 'uncomparable', 'input_history_differences': differences})
      continue
    record(number, {'status': 'comparable'})
    try:
      compare(attempt.expected, attempt.actual)
    except Exception as error:
      record(number, {'status': 'output_failure', 'type': type(error).__name__, 'error': str(error)[:4096]})
      raise
    record(number, {'status': 'pass'})
    return number
  record(MAX_ATTEMPTS, {'status': 'input_history_bound_exhausted'})
  raise AdmissionMismatch('no comparable input history in three independent pairs')
