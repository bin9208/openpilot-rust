"""Require the exact #317 recovery without exempting any old frame or field."""
import copy
import json
import subprocess
import sys
import pytest
from check_ublox import assert_equivalent


@pytest.fixture(scope='module')
def traces():
  # Existing source_parser intentionally shims messaging; keep it in a fresh
  # interpreter so this test cannot replace other tests' runtime IPC imports.
  script = '\n'.join([
    'import json',
    'from check_ublox import source_trace, corrected_oracle',
    'from ublox_fixture import chunks',
    'request = chunks()',
    'original = source_trace(request)',
    'print(json.dumps([request, original, corrected_oracle(request, original)]))',
  ])
  child = subprocess.run([sys.executable, '-c', script], check=True, capture_output=True, text=True)
  return json.loads(child.stdout)



def test_corrected_source_oracle_retains_all_original_results(traces):
  request, original, corrected = traces
  assert assert_equivalent(original, corrected, corrected) == corrected
  old = [row for chunk in original for row in chunk['results']]
  new = [row for chunk in corrected for row in chunk['results']]
  assert new[1:] == old and len(old) == 57
  assert new[0]['service'] == 'gpsLocationExternal'
  assert new[0]['event']['gpsLocationExternal']['hasFix'] is True


def test_unchanged_buggy_native_trace_is_rejected(traces):
  request, original, corrected = traces
  with pytest.raises(AssertionError):
    assert_equivalent(original, corrected, original)


@pytest.mark.parametrize('change', ['missing-recovery', 'recovered-field', 'old-field', 'old-order',
                                  'old-frame', 'checksum', 'prefix-buffer', 'later-buffer', 'log-time',
                                  'extra-chunk', 'unknown-field'])
def test_only_the_exact_recovered_packet_delta_is_allowed(traces, change):
  request, original, corrected = traces
  native = copy.deepcopy(corrected)
  populated = [(i, j) for i, chunk in enumerate(native) for j, result in enumerate(chunk['results']) if 'event' in result]
  old_i, old_j = populated[1]
  if change == 'missing-recovery':
    native[1]['frames'].pop(0)
    native[1]['results'].pop(0)
  elif change == 'recovered-field':
    native[1]['results'][0]['event']['gpsLocationExternal']['latitude'] += 1
  elif change == 'old-field':
    native[old_i]['results'][old_j]['event']['valid'] = False
  elif change == 'old-order':
    first = populated[1]
    second = next(position for position in populated[2:]
                  if native[position[0]]['results'][position[1]] != native[first[0]]['results'][first[1]])
    a, b = native[first[0]]['results'][first[1]], native[second[0]]['results'][second[1]]
    native[first[0]]['results'][first[1]], native[second[0]]['results'][second[1]] = b, a
  elif change == 'old-frame':
    native[old_i]['frames'][old_j][6] ^= 1
  elif change == 'checksum':
    native[1]['frames'][0][-1] ^= 1
  elif change == 'prefix-buffer':
    native[0]['buffer'] = []
  elif change == 'later-buffer':
    native[-1]['buffer'] = []
  elif change == 'log-time':
    native[old_i]['last_log_time'] += 1
  elif change == 'extra-chunk':
    native.append(copy.deepcopy(native[-1]))
  else:
    native[old_i]['unknown'] = True
  with pytest.raises((AssertionError, ValueError)):
    assert_equivalent(original, corrected, native)
