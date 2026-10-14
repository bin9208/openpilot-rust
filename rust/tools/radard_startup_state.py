"""Original/native startup cadence and validity with identical receive clocks."""
import json
from pathlib import Path
import subprocess

from check_message_state import source


def check(binary: Path, output: Path) -> dict:
  output.mkdir(parents=True, exist_ok=True)
  scope, environment = source()
  environment['simulation'] = '0'
  names = ['modelV2', 'carState', 'liveTracks', 'livePose']
  request = output / 'clock-request.jsonl'
  expected = []
  frames = []
  cases = {
    'smooth': [10.0, 10.05, 10.10],
    'startup-hitch': [10.0, 10.065, 10.10],
    'invalid-recovery': [10.0, 10.05, 10.10, 10.15],
    'availability': [10.0, 10.499999, 10.5, 10.500001, 10.55],
  }
  for name, times in cases.items():
    state = scope['SubMaster'](names, poll='modelV2', ignore_alive=['livePose'], ignore_valid=['livePose'])
    for index, now in enumerate(times):
      messages = []
      for topic in names:
        if name == 'availability' and index in (1, 2, 3):
          continue
        event = scope['new_message'](topic, valid=not (name == 'invalid-recovery' and index == 2))
        event.logMonoTime = int(now * 1e9)
        messages.append(event)
      state.update_msgs(now, [event.as_reader() for event in messages])
      frames.append(
        {
          'configuration': {'services': names, 'options': {'poll': {'one': 'modelV2'}, 'ignore_alive': ['livePose'], 'ignore_valid': ['livePose']}}
          if index == 0
          else None,
          'time': now,
          'messages': [list(e.to_bytes()) for e in messages],
          'checks': [[]],
        }
      )
      expected.append(
        {
          'case': name,
          'index': index,
          'checks': [state.all_alive(), state.all_freq_ok(), state.all_valid(), state.all_checks()],
          'topics': {t: [state.alive[t], state.freq_ok[t], state.valid[t]] for t in names},
        }
      )
  request.write_text(''.join(json.dumps(f) + '\n' for f in frames))
  actual = output / 'clock-native.jsonl'
  subprocess.run([str(binary.resolve()), str(request), str(actual)], check=True)
  rows = [json.loads(line) for line in actual.read_text().splitlines()]
  for original, native in zip(expected, rows, strict=True):
    assert native['error'] is None
    assert original['checks'] == native['checks'][0], (original, native)
    assert len(native['topics']) == len(names) and {topic['name'] for topic in native['topics']} == set(names), native
    for topic in native['topics']:
      assert original['topics'][topic['name']] == [topic['alive'], topic['frequency_ok'], topic['valid']], (original, native)
  for case, sequence in [('smooth', [False, True, True]), ('startup-hitch', [False, False, True]), ('invalid-recovery', [False, True, False, True])]:
    assert [r['checks'][3] for r in expected if r['case'] == case] == sequence
  assert [r['topics']['liveTracks'][0] for r in expected if r['case'] == 'availability'] == [True, True, False, False, True]
  (output / 'clock-source.json').write_text(json.dumps(expected, indent=2) + '\n')
  report = {
    'passed': True,
    'identical_clock_frames': len(rows),
    'cases': list(cases),
    'startup_sequences': [[False, True, True], [False, False, True]],
    'availability_epsilon_s': 1e-6,
    'invalid_recovery_preserved': True,
  }
  (output / 'receipt.json').write_text(json.dumps(report, indent=2) + '\n')
  return report
