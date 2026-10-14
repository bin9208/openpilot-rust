"""Input eligibility tests; no live IPC or project Params/msgq bindings."""

import ast
import copy
import json
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock  # noqa: TID251 - deterministic comparison callback controls

import pytest

from card_qa.runtime_admission import AdmissionMismatch, Attempt, TOPICS, build_registry, run_aligned, validate_history


def registry_rows():
  return [{'messages': {name: list(json.dumps([name, 10 + index, True]).encode()) for index, name in enumerate(TOPICS[:-1])}}]


def registry():
  return build_registry(registry_rows(), lambda raw: tuple(json.loads(raw)))


def history():
  return [
    {
      'frame': frame,
      'admission': [
        {
          'name': name,
          'seen': name != TOPICS[-1],
          'updated': name != TOPICS[-1] and frame != 1,
          'receive_frame': (0 if frame <= 1 else frame) if name != TOPICS[-1] else 0,
          'log_mono_time': 10 + index if name != TOPICS[-1] else 0,
        }
        for index, name in enumerate(TOPICS)
      ],
    }
    for frame in range(3)
  ]


def test_complete_history_allows_repeated_wire_id_with_new_receipt():
  value = validate_history(history(), registry(), 2)
  assert len(value) == 3
  assert value[0]['admission'][-1]['identity'] is None
  assert value[0]['admission'][0]['identity']['valid'] is True
  assert value[1]['admission'][0]['receive_frame'] == 0
  assert value[2]['admission'][0]['receive_frame'] == 2


@pytest.mark.parametrize(
  'change',
  [
    'unknown',
    'duplicate_topic',
    'missing_topic',
    'unseen_updated',
    'stale_changed',
    'stale_frame',
    'future_frame',
    'missing_frame',
    'duplicate_frame',
    'truncated',
    'invalid_type',
  ],
)
def test_malformed_or_unbound_history_is_hard_failure(change):
  rows = history()
  if change == 'unknown':
    rows[0]['admission'][0]['log_mono_time'] = 999
  elif change == 'duplicate_topic':
    rows[0]['admission'][-1] = rows[0]['admission'][0].copy()
  elif change == 'missing_topic':
    rows[0]['admission'].pop()
  elif change == 'unseen_updated':
    rows[0]['admission'][-1]['updated'] = True
  elif change == 'stale_changed':
    rows[1]['admission'][0]['log_mono_time'] = 11
  elif change == 'stale_frame':
    rows[1]['admission'][0]['receive_frame'] = 1
  elif change == 'future_frame':
    rows[0]['admission'][0]['receive_frame'] = 1
  elif change == 'missing_frame':
    rows.pop(1)
  elif change == 'duplicate_frame':
    rows[1]['frame'] = 0
  elif change == 'truncated':
    rows.pop()
  elif change == 'invalid_type':
    rows[0]['admission'][0]['seen'] = 1
  with pytest.raises(AssertionError):
    validate_history(rows, registry(), 2)


def test_unknown_duplicate_and_wrong_topic_wire_identities_fail():
  rows = registry_rows()
  with pytest.raises(AssertionError):
    build_registry(rows * 2, lambda raw: tuple(json.loads(raw)))
  rows[0]['messages']['carControl'] = list(json.dumps(['carState', 1, True]).encode())
  with pytest.raises(AssertionError):
    build_registry(rows, lambda raw: tuple(json.loads(raw)))


def test_validity_is_bound_to_wire_not_derived_runtime_policy():
  rows = registry_rows()
  rows[0]['messages']['carControl'] = list(json.dumps(['carControl', 11, False]).encode())
  values = history()
  for row in values:
    row['frequency_ok'] = False  # Not an admission eligibility criterion.
  normalized = validate_history(values, build_registry(rows, lambda raw: tuple(json.loads(raw))), 2)
  assert normalized[0]['admission'][1]['identity']['valid'] is False


def test_complete_history_does_not_assume_two_startup_steps():
  rows = history()
  for frame in range(3, 7):
    row = copy.deepcopy(rows[-1])
    row['frame'] = frame
    for topic in row['admission'][:-1]:
      topic['receive_frame'] = frame
    rows.append(row)
  assert len(validate_history(rows, registry(), 5)) == 6


def test_poststream_rows_are_validated_but_not_compared():
  rows = history()
  assert len(validate_history(rows, registry(), 1)) == 2
  rows[2]['frame'] = 99
  with pytest.raises(AssertionError):
    validate_history(rows, registry(), 1)


def test_input_mismatch_then_match_uses_two_attempts_only():
  prepare = Mock(side_effect=[Attempt([1], [2], 'different', 'output'), Attempt([1], [1], 'same', 'same')])
  compare = Mock()
  events = []
  assert run_aligned(prepare, compare, lambda i, value: events.append((i, value))) == 2
  assert prepare.call_count == 2
  compare.assert_called_once_with('same', 'same')
  assert any(v['status'] == 'uncomparable' for _, v in events)


def test_three_input_mismatches_exhaust_bound_without_selecting_outputs():
  prepare = Mock(return_value=Attempt([1], [2], 'same', 'same'))
  compare = Mock()
  with pytest.raises(AdmissionMismatch):
    run_aligned(prepare, compare, lambda *_: None)
  assert prepare.call_count == 3
  compare.assert_not_called()


@pytest.mark.parametrize('failure', ['80 packets', 'CP mismatch', 'lifecycle', 'timeout', 'malformed trace', 'readiness'])
def test_strict_prerequisite_failure_never_retries(failure):
  prepare = Mock(side_effect=AssertionError(failure))
  with pytest.raises(AssertionError, match=failure):
    run_aligned(prepare, Mock(), lambda *_: None)
  assert prepare.call_count == 1


def test_matched_input_tampered_output_fails_without_retry():
  expected = {'carOutput': [{'torque': -0.4, 'valid': True}]}
  actual = copy.deepcopy(expected)
  actual['carOutput'][0]['torque'] = -0.3
  prepare = Mock(return_value=Attempt([1], [1], expected, actual))

  def compare(left, right):
    assert left == right

  with pytest.raises(AssertionError):
    run_aligned(prepare, compare, lambda *_: None)
  assert prepare.call_count == 1


@pytest.mark.parametrize('failure', ['80 packets', 'CP mismatch', 'lifecycle', 'malformed trace', 'output mismatch'])
def test_actual_case_wiring_never_retries_strict_failures(tmp_path, failure):
  # Execute the real orchestration function without importing native IPC.
  path = Path(__file__).resolve().parents[1] / 'check_card_runtime_pumped.py'
  tree = ast.parse(path.read_text())
  node = next(item for item in tree.body if isinstance(item, ast.FunctionDef) and item.name == 'compare_case')
  capture = Mock(return_value={'params': 'params', 'passive': False})
  measured = Mock(return_value='output')
  history = Mock(return_value=[])
  compare = Mock()
  if failure == '80 packets':
    measured.side_effect = AssertionError(failure)
  elif failure == 'CP mismatch':
    compare.side_effect = AssertionError(failure)
  elif failure == 'lifecycle':
    capture.side_effect = AssertionError(failure)
  elif failure == 'malformed trace':
    history.side_effect = AssertionError(failure)
  else:
    compare.side_effect = [None, AssertionError(failure)]
  scope = {
    'RuntimeInvocation': object,
    'Scenario': object,
    'json': json,
    'Attempt': Attempt,
    'run_aligned': run_aligned,
    'build_registry': lambda *_: {},
    'wire_identity': lambda _: None,
    'replace': lambda _, evidence: SimpleNamespace(evidence=evidence),
    'capture': capture,
    'measured': measured,
    'input_history': history,
    'compare': compare,
  }
  exec(compile(ast.Module(body=[node], type_ignores=[]), str(path), 'exec'), scope)
  with pytest.raises(AssertionError, match=failure):
    scope['compare_case'](SimpleNamespace(evidence=tmp_path), SimpleNamespace(candidate='NISSAN_XTRAIL', enabled=True), b'[]')
  case = tmp_path / 'NISSAN_XTRAIL-True'
  assert (case / 'attempt-01').is_dir() and not (case / 'attempt-02').exists()
  events = json.loads((case / 'attempts.json').read_text())
  assert events[-1]['status'] == ('output_failure' if failure == 'output mismatch' else 'hard_failure')
  assert all(event['attempt'] == 1 for event in events)


def test_unchanged_original_submaster_exposes_control_admission_boundary():
  from check_message_state import source
  from openpilot.cereal import log

  wire_rows = []
  for index, torque in enumerate([-0.4, -0.3]):
    messages = {}
    for offset, name in enumerate(TOPICS[:-1]):
      event = log.Event.new_message(logMonoTime=100 * (index + 1) + offset, valid=True)
      if name in ('pandaStates', 'onroadEvents'):
        event.init(name, 0)
      else:
        event.init(name)
      if name == 'carControl':
        event.carControl.actuators.torque = torque
      messages[name] = list(event.to_bytes())
    wire_rows.append({'messages': messages})

  def decode(raw):
    with log.Event.from_bytes(raw) as event:
      return event.which(), event.logMonoTime, event.valid

  identities = build_registry(wire_rows, decode)
  histories, selected = [], []
  for ahead in [False, True]:
    scope, _ = source()
    state = scope['SubMaster'](list(TOPICS))
    rows = []
    for frame in range(3):
      if frame == 0:
        raw = list(wire_rows[0]['messages'].values())
      elif frame == 1:
        raw = [wire_rows[int(ahead)]['messages']['carControl']]
      else:
        raw = [] if ahead else [wire_rows[1]['messages']['carControl']]
      state.update_msgs(10 + frame * 0.01, list(log.Event.read_multiple_bytes(b''.join(bytes(value) for value in raw))))
      if frame == 1:
        selected.append((state.logMonoTime['carControl'], state['carControl'].actuators.torque))
      rows.append(
        {
          'frame': state.frame,
          'admission': [
            {
              'name': name,
              'seen': state.seen[name],
              'updated': state.updated[name],
              'receive_frame': state.recv_frame[name],
              'log_mono_time': state.logMonoTime[name],
            }
            for name in TOPICS
          ],
        }
      )
    histories.append(validate_history(rows, identities, 2))
  assert selected[0][0] == 101 and selected[1][0] == 201
  assert selected[0][1] != selected[1][1]
  compare = Mock()
  with pytest.raises(AdmissionMismatch):
    run_aligned(lambda _: Attempt(histories[0], histories[1], selected[0][1], selected[1][1]), compare, lambda *_: None)
  compare.assert_not_called()
